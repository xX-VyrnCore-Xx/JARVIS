//! Test di RF-11, nodi esecutori (ADR-0017): Core HTTP reale e agente del nodo in un altro thread, con
//! workspace separate. Coprono associazione, esecuzione con policy e conferma, confini della workspace del
//! nodo, nodo non in linea, revoca immediata con perdita di accesso e assegnazioni.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::node_agent::{run, AgentConfig, AgentError};
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

struct Env {
    url: String,
    token: String,
    data: PathBuf,
    _rt: tokio::runtime::Runtime,
}

fn temp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "jarvis-node-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ))
}

/// Core HTTP su una porta libera, con il token di sessione «t».
fn core() -> Env {
    core_with_router(|app| app)
}

fn core_with_router(configure: impl FnOnce(axum::Router) -> axum::Router) -> Env {
    std::env::set_var("JARVIS_NODE_TIMEOUT_MS", "4000");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let data = temp("core");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let app = configure(api::router(api::state(
        Jarvis::new(cfg).unwrap(),
        "t".into(),
        port,
    )));
    rt.spawn(async move { axum::serve(listener, app).await.unwrap() });
    Env {
        url: format!("http://127.0.0.1:{port}"),
        token: "t".into(),
        data,
        _rt: rt,
    }
}

impl Env {
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let req = ureq::request(method, &format!("{}{path}", self.url))
            .set("X-Jarvis-Token", &self.token);
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

    fn pairing_code(&self) -> String {
        let (_, out) = self.call(
            "POST",
            "/api/devices/pairing",
            Some(json!({"scopes": ["node"]})),
        );
        assert_eq!(
            out["scopes"],
            json!(["node"]),
            "un codice per nodi concede solo il permesso node"
        );
        out["code"].as_str().unwrap().to_string()
    }

    fn chat(&self, message: &str) -> Value {
        self.call("POST", "/api/chat", Some(json!({"message": message})))
            .1
    }

    fn wait_online(&self, slug: &str) {
        for _ in 0..100 {
            let (_, nodes) = self.call("GET", "/api/nodes", None);
            if nodes["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["slug"] == slug && n["online"] == true)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("il nodo {slug} non è andato in linea");
    }
}

fn step<'a>(out: &'a Value, kind: &str) -> Option<&'a Value> {
    out["steps"]
        .as_array()
        .and_then(|s| s.iter().find(|x| x["kind"] == kind))
}

fn spawn_agent(
    env: &Env,
    dir: &Path,
    code: &str,
    tools: &[&str],
) -> (
    Arc<AtomicBool>,
    std::thread::JoinHandle<Result<(), AgentError>>,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let cfg = AgentConfig {
        core_url: env.url.clone(),
        dir: dir.to_path_buf(),
        name: "Studio".into(),
        code: Some(code.into()),
        capabilities: tools.iter().map(|t| t.to_string()).collect(),
        poll_wait_s: 1,
    };
    let s = stop.clone();
    (stop, std::thread::spawn(move || run(&cfg, s)))
}

#[test]
fn node_executes_assigned_tools_inside_its_own_workspace() {
    let env = core();
    let node_dir = temp("agent");
    let (stop, agent) = spawn_agent(
        &env,
        &node_dir,
        &env.pairing_code(),
        &["system.info", "fs.list", "fs.read", "fs.write"],
    );
    env.wait_online("studio");

    // Strumenti del nodo nel catalogo, con ambito «node».
    let (_, tools) = env.call("GET", "/api/tools", None);
    let names: Vec<&str> = tools["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names.contains(&"node.studio.fs.write") && !names.contains(&"node.studio.time.now"),
        "{names:?}"
    );

    // Lettura senza conferma: eseguita dal nodo.
    let out = env.chat("nodo studio system.info");
    assert_eq!(
        step(&out, "tool_result").unwrap()["status"],
        "succeeded",
        "{out}"
    );
    assert!(
        out["reply"].as_str().unwrap().starts_with("[nodo studio]"),
        "{out}"
    );

    // Scrittura: conferma richiesta, poi il file finisce nella workspace del nodo, non in quella del Core.
    let out = env.chat("nodo studio fs.write path=nota.txt content=\"dal Core al nodo\"");
    let confirmation = out["confirmation"].clone();
    assert!(
        confirmation["expected_effect"]
            .as_str()
            .unwrap()
            .contains("Sul nodo «studio»"),
        "{out}"
    );
    assert!(
        !node_dir.join("workspace/nota.txt").exists(),
        "nessuna scrittura prima della conferma"
    );
    let id = confirmation["confirmation_id"].as_str().unwrap();
    let (_, done) = env.call(
        "POST",
        &format!("/api/confirmations/{id}"),
        Some(json!({"approve": true})),
    );
    assert_eq!(
        step(&done, "tool_result").unwrap()["status"],
        "succeeded",
        "{done}"
    );
    assert_eq!(
        std::fs::read_to_string(node_dir.join("workspace/nota.txt")).unwrap(),
        "dal Core al nodo"
    );
    assert!(!env.data.join("workspace/nota.txt").exists());

    // Uscire dalla workspace del nodo è negato dalla policy del Core prima di arrivare al nodo.
    let out = env.chat("nodo studio fs.read path=../node.json");
    assert_eq!(step(&out, "policy").unwrap()["decision"], "deny", "{out}");

    // Audit: l'esecuzione sul nodo è registrata con il nome dello strumento.
    let (_, audit) = env.call("GET", "/api/audit?limit=200", None);
    assert!(audit.to_string().contains("node.studio.fs.write"));

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    agent.join().unwrap().unwrap();
}

#[test]
fn agent_retries_delivery_of_the_same_result_after_a_transient_failure() {
    let deliveries = Arc::new(std::sync::Mutex::new(Vec::new()));
    let incoming = deliveries.clone();
    let env = core_with_router(|app| {
        app.layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let incoming = incoming.clone();
                async move {
                    use axum::response::IntoResponse;
                    if req.uri().path().starts_with("/api/nodes/result/") {
                        let mut deliveries = incoming.lock().unwrap();
                        deliveries.push(req.uri().path().to_owned());
                        if deliveries.len() == 1 {
                            return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
                        }
                    }
                    next.run(req).await
                }
            },
        ))
    });
    let node_dir = temp("retry");
    let (stop, agent) = spawn_agent(&env, &node_dir, &env.pairing_code(), &["time.now"]);
    env.wait_online("studio");

    let out = env.chat("nodo studio time.now");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    agent.join().unwrap().unwrap();
    assert_eq!(
        step(&out, "tool_result").unwrap()["status"],
        "succeeded",
        "{out}"
    );
    let deliveries = deliveries.lock().unwrap();
    assert_eq!(deliveries.len(), 2);
    assert_eq!(deliveries[0], deliveries[1]);
}

#[test]
fn revocation_removes_tools_fails_pending_work_and_stops_the_node() {
    let env = core();
    let node_dir = temp("agent");
    let (_stop, agent) = spawn_agent(&env, &node_dir, &env.pairing_code(), &["time.now"]);
    env.wait_online("studio");
    let state: Value =
        serde_json::from_slice(&std::fs::read(node_dir.join("node.json")).unwrap()).unwrap();
    let id = state["device_id"].as_str().unwrap().to_string();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(node_dir.join("node.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "token del nodo leggibile solo dall'utente");
    }
    let out = env.chat("nodo studio time.now");
    assert_eq!(
        step(&out, "tool_result").unwrap()["status"],
        "succeeded",
        "{out}"
    );

    let (status, _) = env.call("POST", &format!("/api/devices/{id}/revoke"), None);
    assert_eq!(status, 200);
    // Il nodo riceve 401 alla richiesta successiva e si ferma.
    assert!(matches!(agent.join().unwrap(), Err(AgentError::Revoked)));
    let (_, tools) = env.call("GET", "/api/tools", None);
    assert!(
        !tools.to_string().contains("node.studio."),
        "strumenti del nodo rimossi"
    );
    let out = env.chat("nodo studio time.now");
    assert!(step(&out, "tool_call").is_none(), "{out}");
    // Il token revocato non vale più nemmeno inviato a mano.
    let res = ureq::get(&format!("{}/api/nodes/next?wait=0", env.url))
        .set("X-Jarvis-Device", state["token"].as_str().unwrap())
        .call();
    assert!(matches!(res, Err(ureq::Error::Status(401, _))));
}

#[test]
fn api_revocation_interrupts_an_assignment_waiting_for_a_result() {
    let env = core();
    let (status, paired) = env.call(
        "POST",
        "/api/devices/pair",
        Some(json!({
            "code": env.pairing_code(), "name": "Studio", "platform": "desktop",
            "capabilities": ["fs.write"]
        })),
    );
    assert_eq!(status, 200, "{paired}");
    let token = paired["token"].as_str().unwrap();
    let id = paired["device"]["device_id"].as_str().unwrap();
    let poll = |wait| match ureq::get(&format!("{}/api/nodes/next?wait={wait}", env.url))
        .set("X-Jarvis-Device", token)
        .call()
    {
        Ok(response) => response.status(),
        Err(ureq::Error::Status(status, _)) => status,
        Err(err) => panic!("{err}"),
    };
    assert_eq!(poll(0), 204);

    let out = env.chat("nodo studio fs.write path=nota.txt content=prova");
    let confirmation = out["confirmation"]["confirmation_id"].as_str().unwrap();
    let revoke = format!("/api/devices/{id}/revoke");
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| {
            env.call(
                "POST",
                &format!("/api/confirmations/{confirmation}"),
                Some(json!({"approve": true})),
            )
        });
        assert_eq!(poll(3), 200, "assegnazione consegnata al nodo");
        let (status, revoked) = env.call("POST", &revoke, None);
        assert_eq!(status, 200, "{revoked}");
        assert!(revoked["revoked_at"].is_string());
        let (status, out) = pending.join().unwrap();
        assert_eq!(status, 200, "{out}");
        let result = step(&out, "tool_result").unwrap();
        assert_eq!(result["error"], "node.revoked", "{out}");
        assert_eq!(result["effect"], "unknown", "{out}");
        assert_eq!(env.call("POST", &revoke, None), (200, revoked));
    });
    assert_eq!(poll(0), 401);
    let stored = jarvis_core::store::Store::open(&env.data.join("jarvis.sqlite3")).unwrap();
    assert!(stored
        .device_by_token(&jarvis_core::nodes::token_hash(token))
        .unwrap()
        .is_none());
}

#[test]
fn offline_node_and_wrong_codes_are_reported() {
    let env = core();
    let node_dir = temp("agent");
    let (stop, agent) = spawn_agent(&env, &node_dir, &env.pairing_code(), &["time.now"]);
    env.wait_online("studio");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    agent.join().unwrap().unwrap();
    // Un codice per dispositivi (chat) non associa un nodo; un nodo non può chattare.
    let (_, code) = env.call(
        "POST",
        "/api/devices/pairing",
        Some(json!({"scopes": ["chat"]})),
    );
    let err = run(
        &AgentConfig {
            core_url: env.url.clone(),
            dir: temp("agent2"),
            name: "Altro".into(),
            code: Some(code["code"].as_str().unwrap().into()),
            capabilities: vec![],
            poll_wait_s: 1,
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert!(matches!(err, Err(AgentError::Pairing(_))), "{err:?}");
    let state: Value =
        serde_json::from_slice(&std::fs::read(node_dir.join("node.json")).unwrap()).unwrap();
    let res = ureq::get(&format!("{}/api/conversation", env.url))
        .set("X-Jarvis-Device", state["token"].as_str().unwrap())
        .call();
    assert!(
        matches!(res, Err(ureq::Error::Status(401, _))),
        "il token del nodo non apre l'API del proprietario"
    );
    // Nodo spento: dopo un minuto senza richieste risulta non in linea; qui si verifica il messaggio
    // forzando l'assenza con un nodo mai avviato.
    let (_, code) = env.call(
        "POST",
        "/api/devices/pairing",
        Some(json!({"scopes": ["node"]})),
    );
    let (status, paired) = env.call("POST", "/api/devices/pair", Some(json!({"code": code["code"], "name": "Spento", "platform": "desktop", "capabilities": ["time.now"]})));
    assert_eq!(status, 200, "{paired}");
    let out = env.chat("nodo spento time.now");
    let result = step(&out, "tool_result").unwrap();
    assert_eq!(
        (result["status"].as_str(), result["error"].as_str()),
        (Some("failed"), Some("node.offline")),
        "{out}"
    );
}

#[test]
fn automatic_node_choice_runs_on_an_online_node_and_follows_revocation() {
    let env = core();
    let mut agents = vec![];
    for name in ["Studio", "Garage"] {
        let stop = Arc::new(AtomicBool::new(false));
        let cfg = AgentConfig {
            core_url: env.url.clone(),
            dir: temp("auto"),
            name: name.into(),
            code: Some(env.pairing_code()),
            capabilities: vec!["time.now".into(), "system.info".into()],
            poll_wait_s: 1,
        };
        let s = stop.clone();
        agents.push((stop, std::thread::spawn(move || run(&cfg, s))));
    }
    env.wait_online("studio");
    env.wait_online("garage");
    let (_, tools) = env.call("GET", "/api/tools", None);
    assert!(
        tools.to_string().contains("node.auto.time.now"),
        "strumento automatico registrato"
    );

    let out = env.chat("nodo auto time.now");
    assert_eq!(
        step(&out, "tool_result").unwrap()["status"],
        "succeeded",
        "{out}"
    );

    // Revocati entrambi i nodi, lo strumento automatico sparisce.
    let (_, devices) = env.call("GET", "/api/devices", None);
    for d in devices["items"].as_array().unwrap() {
        let id = d["device_id"].as_str().unwrap();
        env.call("POST", &format!("/api/devices/{id}/revoke"), None);
    }
    for (_, a) in agents {
        let _ = a.join();
    }
    let (_, tools) = env.call("GET", "/api/tools", None);
    assert!(
        !tools.to_string().contains("node.auto."),
        "nessun nodo, nessuno strumento automatico"
    );
}
