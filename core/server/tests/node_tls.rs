//! mTLS fra Core e nodi (ADR-0041): Core HTTP e porta TLS dei nodi reali, agente del nodo in un altro thread.
//! Il nodo si lega al proprio certificato all'associazione; dopo, il token da solo non basta, un altro
//! certificato non entra e la revoca ferma il nodo.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::node_agent::{run, AgentConfig, AgentError};
use jarvis_core::orchestrator::Jarvis;
use jarvis_core::tls::Identity;
use serde_json::{json, Value};

fn temp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "jarvis-mtls-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ))
}

struct Env {
    url: String,
    tls_port: u16,
    core_fp: String,
    _rt: tokio::runtime::Runtime,
}

fn core() -> Env {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let plain = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let tls = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let (port, tls_port) = (
        plain.local_addr().unwrap().port(),
        tls.local_addr().unwrap().port(),
    );
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port,
        data_dir: temp("core"),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let mut jarvis = Jarvis::new(cfg).unwrap();
    let identity = jarvis.enable_node_tls(tls_port).unwrap();
    let core_fp = identity.fingerprint().unwrap();
    let state = api::state(jarvis, "t".into(), port);
    let app = api::router(state.clone());
    rt.spawn(async move { axum::serve(plain, app).await.unwrap() });
    rt.spawn(async move { api::serve_node_tls(state, tls, identity).await.unwrap() });
    Env {
        url: format!("http://127.0.0.1:{port}"),
        tls_port,
        core_fp,
        _rt: rt,
    }
}

impl Env {
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let req = ureq::request(method, &format!("{}{path}", self.url)).set("X-Jarvis-Token", "t");
        let res = match body {
            Some(b) => req
                .set("Content-Type", "application/json")
                .send_string(&b.to_string()),
            None => req.call(),
        };
        let (status, text) = match res {
            Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(s, r)) => (s, r.into_string().unwrap_or_default()),
            Err(e) => panic!("{e}"),
        };
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn wait_online(&self, slug: &str) -> Value {
        for _ in 0..200 {
            let (_, nodes) = self.call("GET", "/api/nodes", None);
            if let Some(n) = nodes["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["slug"] == slug && n["online"] == true)
            {
                return n.clone();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("il nodo {slug} non è andato in linea");
    }
}

/// Client TLS verso la porta dei nodi con un'identità arbitraria e il Core fissato.
fn tls_client(identity: &Identity, core_fp: &str) -> ureq::Agent {
    let config = jarvis_core::tls::client_config(identity, core_fp).unwrap();
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .tls_config(Arc::new(config))
        .build()
}

#[test]
fn node_is_bound_to_its_certificate_and_stops_when_revoked() {
    let env = core();
    let (_, pairing) = env.call(
        "POST",
        "/api/devices/pairing",
        Some(json!({"scopes": ["node"]})),
    );
    let dir = temp("agent");
    let stop = Arc::new(AtomicBool::new(false));
    let cfg = AgentConfig {
        core_url: env.url.clone(),
        dir: dir.clone(),
        name: "Studio".into(),
        code: Some(pairing["code"].as_str().unwrap().into()),
        capabilities: vec!["system.info".into()],
        poll_wait_s: 1,
    };
    let s = stop.clone();
    let agent = std::thread::spawn(move || run(&cfg, s));
    let node = env.wait_online("studio");

    // Associazione: il nodo ha inviato il certificato, il Core lo ha registrato e il nodo usa la porta TLS.
    let state: Value =
        serde_json::from_slice(&std::fs::read(dir.join("node.json")).unwrap()).unwrap();
    assert_eq!(state["mtls"]["core_cert_sha256"], env.core_fp.as_str());
    assert_eq!(
        state["mtls"]["url"],
        format!("https://127.0.0.1:{}", env.tls_port)
    );
    let (_, devices) = env.call("GET", "/api/devices", None);
    let device = devices["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["device_id"] == node["device_id"])
        .cloned()
        .unwrap();
    let node_identity = Identity {
        cert_pem: state["mtls"]["cert_pem"].as_str().unwrap().into(),
        key_pem: state["mtls"]["key_pem"].as_str().unwrap().into(),
    };
    assert_eq!(
        device["cert_sha256"],
        node_identity.fingerprint().unwrap().as_str()
    );
    assert!(
        jarvis_contracts::validate(jarvis_contracts::Contract::Device, &device).is_valid(),
        "dispositivo conforme al contratto: {device}"
    );
    assert!(
        !devices.to_string().contains("PRIVATE KEY"),
        "la chiave privata non arriva al Core"
    );

    // Il nodo esegue gli strumenti attraverso la porta TLS.
    let (_, out) = env.call(
        "POST",
        "/api/chat",
        Some(json!({"message": "nodo studio system.info"})),
    );
    let result = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "tool_result")
        .unwrap();
    assert_eq!(result["status"], "succeeded", "{out}");

    // Il token rubato non basta: in chiaro il Core lo rifiuta per un nodo legato al certificato.
    let token = state["token"].as_str().unwrap();
    let plain = ureq::get(&format!("{}/api/nodes/next?wait=0", env.url))
        .set("X-Jarvis-Device", token)
        .call();
    assert!(
        matches!(plain, Err(ureq::Error::Status(401, _))),
        "token senza certificato: {plain:?}"
    );

    // Un altro certificato con lo stesso token non supera l'handshake.
    let stranger = tls_client(&Identity::generate("intruso").unwrap(), &env.core_fp);
    let res = stranger
        .get(&format!(
            "https://127.0.0.1:{}/api/nodes/next?wait=0",
            env.tls_port
        ))
        .set("X-Jarvis-Device", token)
        .call();
    assert!(
        matches!(res, Err(ureq::Error::Transport(_))),
        "certificato sconosciuto: {res:?}"
    );

    // Un Core con un altro certificato viene rifiutato dal nodo.
    let wrong_pin = tls_client(&node_identity, &"0".repeat(64));
    let res = wrong_pin
        .get(&format!(
            "https://127.0.0.1:{}/api/nodes/next?wait=0",
            env.tls_port
        ))
        .call();
    assert!(
        matches!(res, Err(ureq::Error::Transport(_))),
        "Core non fissato: {res:?}"
    );

    // Revoca: il nodo riceve 401 sulla porta TLS e si ferma.
    let id = node["device_id"].as_str().unwrap();
    let (status, _) = env.call("POST", &format!("/api/devices/{id}/revoke"), None);
    assert_eq!(status, 200);
    let ended = (0..200).find_map(|_| {
        if agent.is_finished() {
            return Some(());
        }
        std::thread::sleep(Duration::from_millis(50));
        None
    });
    stop.store(true, Ordering::Relaxed);
    assert!(ended.is_some(), "il nodo revocato deve fermarsi");
    assert!(matches!(agent.join().unwrap(), Err(AgentError::Revoked)));
}
