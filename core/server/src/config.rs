//! Configurazione da variabili d'ambiente. Il Core ascolta solo su loopback.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub python: String,
    pub ai_dir: PathBuf,
    /// Backend del worker. L’app usa il motore a regole; i test possono sostituirlo.
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
            ai_backend: Some("offline".into()),
            delegation_timeout_ms: var("JARVIS_DELEGATION_TIMEOUT_MS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(180_000),
            voice_engine: var("JARVIS_VOICE_ENGINE"),
            worker_env: Vec::new(),
            scheduler_tick_s: var("JARVIS_SCHEDULER_TICK_S")
                .and_then(|v| v.parse().ok())
                .filter(|v| *v >= 1)
                .unwrap_or(20),
            // Token di sessione: dal processo che avvia il Core (l'app desktop, per le notifiche native,
            // ADR-0044) se robusto, altrimenti casuale a ogni avvio.
            token: var("JARVIS_TOKEN")
                .filter(|t| t.len() >= 32 && t.chars().all(|c| c.is_ascii_alphanumeric()))
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
            web: crate::web::WebConfig::from_env(),
        })
    }

    pub fn workspace(&self) -> PathBuf {
        self.data_dir.join("workspace")
    }

    /// Cartella dei backup, accanto ai dati (un ripristino sposta i dati, non i backup).
    pub fn backups_dir(&self) -> PathBuf {
        let name = self
            .data_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "data".into());
        self.data_dir.with_file_name(format!("{name}-backups"))
    }
}

/// Blocco stabile accanto ai dati, mantenuto anche quando la cartella viene sostituita.
pub fn data_lock(data_dir: &Path, wait: std::time::Duration) -> Result<std::fs::File, String> {
    let name = data_dir.file_name().ok_or("cartella dati non valida")?;
    let mut lock_name = std::ffi::OsString::from(".");
    lock_name.push(name);
    lock_name.push(".jarvis-lock");
    let path = data_dir.with_file_name(lock_name);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("cartella del blocco: {e}"))?;
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| format!("blocco della cartella dati: {e}"))?;
    wait_lock(&lock, data_dir, wait)?;
    Ok(lock)
}

pub(crate) fn wait_lock(
    file: &std::fs::File,
    data_dir: &Path,
    wait: std::time::Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                return Err(format!(
                    "un altro Core JARVIS sta già usando la cartella dati {} o il blocco non è disponibile ({e})",
                    data_dir.display()
                ))
            }
        }
    }
}
