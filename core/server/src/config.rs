//! Configurazione da variabili d'ambiente. Il Core ascolta solo su loopback.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub python: String,
    pub ai_dir: PathBuf,
    /// Override di `JARVIS_AI_BACKEND` per il worker (`auto`, `ollama`, `offline`).
    pub ai_backend: Option<String>,
    pub delegation_timeout_ms: u64,
    /// Variabili aggiuntive per il worker (es. iniezione di guasti nei test, `JARVIS_FAULT_INJECTION`).
    pub worker_env: Vec<(String, String)>,
    /// Override di `JARVIS_VOICE_ENGINE` per il worker (`auto`, `local`, `fake`, `off`).
    pub voice_engine: Option<String>,
    /// Intervallo del pianificatore in secondi.
    pub scheduler_tick_s: u64,
    pub token: String,
    pub web: crate::web::WebConfig,
}

fn repo_root() -> PathBuf {
    std::env::var_os("JARVIS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// Un percorso relativo (es. `.venv/bin/python`) va risolto rispetto alla cartella di avvio:
/// il worker parte con `current_dir` in `ai/`. Un nome semplice (`python3`) resta da cercare nel PATH.
fn absolutize(p: String) -> String {
    let path = PathBuf::from(&p);
    if path.is_relative() && path.components().count() > 1 {
        if let Ok(cwd) = std::env::current_dir() {
            return cwd.join(path).to_string_lossy().into_owned();
        }
    }
    p
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let host: IpAddr = var("JARVIS_HOST")
            .map(|h| {
                h.parse()
                    .map_err(|_| format!("JARVIS_HOST non valido: {h}"))
            })
            .transpose()?
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        if !host.is_loopback() {
            return Err("JARVIS_HOST deve essere un indirizzo di loopback: il prototipo non espone il Core in rete".into());
        }
        let port = var("JARVIS_PORT")
            .map(|p| {
                p.parse()
                    .map_err(|_| format!("JARVIS_PORT non valido: {p}"))
            })
            .transpose()?
            .unwrap_or(8765);
        let root = repo_root();
        Ok(Self {
            host,
            port,
            data_dir: var("JARVIS_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("data")),
            python: var("JARVIS_PYTHON")
                .map(absolutize)
                .unwrap_or_else(|| "python3".into()),
            ai_dir: var("JARVIS_AI_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("ai")),
            ai_backend: var("JARVIS_AI_BACKEND"),
            delegation_timeout_ms: var("JARVIS_DELEGATION_TIMEOUT_MS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(180_000),
            voice_engine: var("JARVIS_VOICE_ENGINE"),
            worker_env: Vec::new(),
            scheduler_tick_s: var("JARVIS_SCHEDULER_TICK_S")
                .and_then(|v| v.parse().ok())
                .filter(|v| *v >= 1)
                .unwrap_or(20),
            token: uuid::Uuid::new_v4().simple().to_string(),
            web: crate::web::WebConfig::from_env(),
        })
    }

    pub fn workspace(&self) -> PathBuf {
        self.data_dir.join("workspace")
    }
}
