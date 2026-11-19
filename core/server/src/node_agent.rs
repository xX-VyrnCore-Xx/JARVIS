//! Agente del nodo esecutore (RF-11, ADR-0017): `jarvis node <url-del-core> --code <codice>`.
//!
//! Gira sul secondo computer con lo stesso binario del Core. Si associa una volta con il codice monouso
//! generato dal proprietario, conserva il token in `<cartella>/node.json` (permessi 0600 su Unix), poi
//! chiede lavoro al Core con richieste in uscita e restituisce gli esiti. Esegue solo gli strumenti
//! dichiarati all'associazione, dentro la propria workspace, e si ferma quando viene revocato.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::nodes::NODE_TOOLS;
use crate::store::Store;
use crate::tools;

pub struct AgentConfig {
    /// Indirizzo del Core, es. `http://127.0.0.1:8765` o `https://jarvis.esempio.ts.net`.
    pub core_url: String,
    /// Cartella del nodo: token, workspace e database locale.
    pub dir: PathBuf,
    pub name: String,
    pub code: Option<String>,
    pub capabilities: Vec<String>,
    /// Attesa lunga di ogni richiesta di lavoro, in secondi (al massimo 25).
    pub poll_wait_s: u64,
}

#[derive(Debug)]
pub enum AgentError {
    Revoked,
    Pairing(String),
    Io(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Revoked => write!(f, "il nodo è stato revocato dal proprietario"),
            AgentError::Pairing(m) => write!(f, "associazione non riuscita: {m}"),
            AgentError::Io(m) => write!(f, "{m}"),
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(35))
        .user_agent(concat!("JARVIS-node/", env!("CARGO_PKG_VERSION")))
        .build()
}

// Involucro sottile: restituisce l'errore di ureq così com'è, come le chiamate dirette alla libreria.
#[allow(clippy::result_large_err)]
fn post_json(req: ureq::Request, body: &Value) -> Result<ureq::Response, ureq::Error> {
    req.set("Content-Type", "application/json")
        .send_string(&body.to_string())
}

/// Ritenta solo la consegna dell'esito già calcolato, anche per gli strumenti di scrittura.
fn send_result(
    http: &ureq::Agent,
    url: &str,
    token: &str,
    reply: &Value,
    stop: &AtomicBool,
) -> Result<(), AgentError> {
    for attempt in 0..3 {
        if attempt > 0 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200 * attempt));
        }
        let sent = post_json(
            http.post(url)
                .timeout(Duration::from_secs(3))
                .set("X-Jarvis-Device", token),
            reply,
        );
        match sent {
            Err(ureq::Error::Status(401, _)) => return Err(AgentError::Revoked),
            Err(ureq::Error::Transport(_) | ureq::Error::Status(408 | 429 | 500..=599, _)) => {}
            // Un 404 indica un esito già ricevuto, un'assegnazione scaduta o un Core riavviato.
            _ => return Ok(()),
        }
    }
    Ok(())
}

fn read_json(r: ureq::Response) -> Value {
    r.into_string()
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join("node.json")
}

fn save_state(dir: &Path, state: &Value) -> Result<(), AgentError> {
    let path = state_path(dir);
    std::fs::write(&path, serde_json::to_vec_pretty(state).unwrap_or_default())
        .map_err(|e| AgentError::Io(format!("{}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Token salvato oppure nuova associazione con il codice.
fn ensure_paired(cfg: &AgentConfig) -> Result<Value, AgentError> {
    std::fs::create_dir_all(&cfg.dir).map_err(|e| AgentError::Io(e.to_string()))?;
    if let Ok(raw) = std::fs::read(state_path(&cfg.dir)) {
        if let Ok(state) = serde_json::from_slice::<Value>(&raw) {
            if state["token"].is_string() && state["core_url"] == cfg.core_url.as_str() {
                return Ok(state);
            }
        }
    }
    let code = cfg.code.as_deref().ok_or_else(|| {
        AgentError::Pairing("serve il codice generato nella sezione Dispositivi (--code)".into())
    })?;
    let caps: Vec<&str> = if cfg.capabilities.is_empty() {
        NODE_TOOLS.to_vec()
    } else {
        cfg.capabilities.iter().map(String::as_str).collect()
    };
    // Identità TLS del nodo (ADR-0041): la chiave privata resta qui; al Core va solo il certificato.
    let identity = crate::tls::Identity::generate("jarvis-node").map_err(AgentError::Io)?;
    let res = post_json(
        agent().post(&format!("{}/api/devices/pair", cfg.core_url)),
        &json!({"code": code, "name": cfg.name, "platform": "desktop", "capabilities": caps, "cert_pem": identity.cert_pem}),
    );
    let body: Value = match res {
        Ok(r) => read_json(r),
        Err(ureq::Error::Status(_, r)) => {
            let v = read_json(r);
            return Err(AgentError::Pairing(
                v["error"]["message"]
                    .as_str()
                    .unwrap_or("codice rifiutato")
                    .to_string(),
            ));
        }
        Err(e) => return Err(AgentError::Pairing(e.to_string())),
    };
    if !body["device"]["scopes"]
        .as_array()
        .is_some_and(|s| s.iter().any(|x| x == "node"))
    {
        return Err(AgentError::Pairing(
            "il codice non è per un nodo esecutore".into(),
        ));
    }
    let mut state = json!({
        "core_url": cfg.core_url, "token": body["token"], "device_id": body["device"]["device_id"],
        "slug": body["device"]["slug"], "capabilities": body["device"]["capabilities"],
    });
    if let (Some(port), Some(fp)) = (
        body["mtls"]["port"].as_u64(),
        body["mtls"]["core_cert_sha256"].as_str(),
    ) {
        state["mtls"] = json!({
            "url": tls_url(&cfg.core_url, port as u16), "core_cert_sha256": fp,
            "cert_pem": identity.cert_pem, "key_pem": identity.key_pem,
        });
    }
    save_state(&cfg.dir, &state)?;
    Ok(state)
}

/// Indirizzo della porta TLS dei nodi: stesso host del Core, porta dichiarata all'associazione;
/// `JARVIS_NODE_TLS_URL` lo sostituisce (es. un nome della rete privata diverso).
fn tls_url(core_url: &str, port: u16) -> String {
    if let Ok(u) = std::env::var("JARVIS_NODE_TLS_URL") {
        return u.trim_end_matches('/').to_string();
    }
    let host = url::Url::parse(core_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "127.0.0.1".into());
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    format!("https://{host}:{port}")
}

/// Client HTTP e indirizzo base: con l'mTLS la porta TLS del Core con certificato del nodo e Core fissato.
fn transport(cfg: &AgentConfig, state: &Value) -> Result<(ureq::Agent, String), AgentError> {
    let mtls = &state["mtls"];
    if mtls.is_null() {
        return Ok((agent(), cfg.core_url.clone()));
    }
    let identity = crate::tls::Identity {
        cert_pem: mtls["cert_pem"].as_str().unwrap_or_default().to_string(),
        key_pem: mtls["key_pem"].as_str().unwrap_or_default().to_string(),
    };
    let config = crate::tls::client_config(
        &identity,
        mtls["core_cert_sha256"].as_str().unwrap_or_default(),
    )
    .map_err(AgentError::Io)?;
    let http = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(35))
        .user_agent(concat!("JARVIS-node/", env!("CARGO_PKG_VERSION")))
        .tls_config(Arc::new(config))
        .build();
    Ok((http, mtls["url"].as_str().unwrap_or_default().to_string()))
}

/// Ciclo del nodo: termina quando `stop` diventa vero (Ok) o quando il nodo viene revocato.
pub fn run(cfg: &AgentConfig, stop: Arc<AtomicBool>) -> Result<(), AgentError> {
    let state = ensure_paired(cfg)?;
    let token = state["token"].as_str().unwrap_or_default().to_string();
    let allowed: Vec<String> = state["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect();
    let workspace = cfg.dir.join("workspace");
    std::fs::create_dir_all(&workspace).map_err(|e| AgentError::Io(e.to_string()))?;
    let store =
        Store::open(&cfg.dir.join("node.sqlite3")).map_err(|e| AgentError::Io(e.to_string()))?;
    let web = crate::web::WebConfig {
        enabled: false,
        ..crate::web::WebConfig::default()
    };
    let (http, base) = transport(cfg, &state)?;
    let mut backoff = Duration::from_millis(500);
    println!(
        "Nodo «{}» associato a {} (strumenti: {}{})",
        state["slug"].as_str().unwrap_or("?"),
        base,
        allowed.join(", "),
        if state["mtls"].is_null() {
            ""
        } else {
            "; mTLS con il Core fissato"
        }
    );
    while !stop.load(Ordering::Relaxed) {
        let res = http
            .get(&format!(
                "{}/api/nodes/next?wait={}{}",
                base,
                cfg.poll_wait_s.min(25),
                resources_query()
            ))
            .set("X-Jarvis-Device", &token)
            .call();
        let assignment: Value = match res {
            Ok(r) if r.status() == 204 => {
                backoff = Duration::from_millis(500);
                continue;
            }
            Ok(r) => read_json(r),
            Err(ureq::Error::Status(401, _)) => return Err(AgentError::Revoked),
            Err(_) => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(15));
                continue;
            }
        };
        backoff = Duration::from_millis(500);
        let tool = assignment["tool"].as_str().unwrap_or_default();
        let id = assignment["assignment_id"].as_str().unwrap_or_default();
        // Il nodo applica anche il proprio elenco: uno strumento non dichiarato non viene eseguito.
        let reply = if !allowed.iter().any(|t| t == tool) {
            json!({"ok": false, "code": "node.not_allowed", "message": format!("strumento {tool} non abilitato su questo nodo")})
        } else {
            let ctx = tools::Context {
                workspace: &workspace,
                store: &store,
                web: &web,
            };
            match tools::execute(tool, &assignment["arguments"], &ctx) {
                Ok(output) => json!({"ok": true, "output": output}),
                Err(e) => json!({"ok": false, "code": e.code, "message": e.message}),
            }
        };
        send_result(
            &http,
            &format!("{}/api/nodes/result/{id}", base),
            &token,
            &reply,
            &stop,
        )?;
    }
    Ok(())
}

/// Risorse del computer per la scelta automatica del nodo (ADR-0031): CPU, e su Linux carico e memoria libera.
fn resources_query() -> String {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut q = format!("&cpus={cpus}");
    if let Ok(load) = std::fs::read_to_string("/proc/loadavg") {
        if let Some(l) = load.split_whitespace().next() {
            q.push_str(&format!("&load={l}"));
        }
    }
    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        if let Some(kb) = mem
            .lines()
            .find(|l| l.starts_with("MemAvailable:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
        {
            q.push_str(&format!("&mem_free_mb={}", kb / 1024));
        }
    }
    q
}
