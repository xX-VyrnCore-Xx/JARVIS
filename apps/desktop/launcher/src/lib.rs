//! Avvio supervisionato del Core JARVIS per l'app desktop (RF-10).
//!
//! Il client desktop non contiene logica di dominio: avvia il binario `jarvis` su una porta locale
//! libera, con la cartella dati dell'utente, attende che `/api/health` risponda e poi mostra
//! l'interfaccia servita dal Core. Alla chiusura termina il Core; il worker Python si chiude da solo
//! quando il suo stdin viene chiuso.

use std::fs::{File, OpenOptions};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum LaunchError {
    CoreNotFound(Vec<PathBuf>),
    Io(String),
    Exited { code: Option<i32>, log: PathBuf },
    Timeout { log: PathBuf },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::CoreNotFound(c) => write!(
                f,
                "binario del Core non trovato (cercato in: {}). Esegui `cargo build --release -p jarvis-core` nella cartella core/ oppure imposta JARVIS_CORE_BIN",
                c.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            LaunchError::Io(e) => write!(f, "errore di sistema: {e}"),
            LaunchError::Exited { code, log } => write!(f, "il Core si è chiuso all'avvio (codice {code:?}); dettagli in {}", log.display()),
            LaunchError::Timeout { log } => write!(f, "il Core non ha risposto in tempo; dettagli in {}", log.display()),
        }
    }
}

impl std::error::Error for LaunchError {}

#[derive(Clone, Debug)]
pub struct LaunchConfig {
    /// Radice del repository JARVIS (contiene `ai/` e `contracts/`).
    pub home: PathBuf,
    pub core_bin: PathBuf,
    pub data_dir: PathBuf,
    pub python: String,
    /// Variabili aggiuntive per il Core (es. `JARVIS_AI_BACKEND`).
    pub env: Vec<(String, String)>,
    pub startup_timeout: Duration,
}

impl LaunchConfig {
    /// Configurazione predefinita: binario e Python cercati a partire da `home`.
    pub fn discover(home: &Path, data_dir: &Path) -> Result<Self, LaunchError> {
        Ok(Self {
            home: home.to_path_buf(),
            core_bin: find_core_binary(home)?,
            data_dir: data_dir.to_path_buf(),
            python: find_python(home),
            env: Vec::new(),
            startup_timeout: Duration::from_secs(60),
        })
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Cerca il binario del Core: `JARVIS_CORE_BIN`, poi le build release e debug del repository.
pub fn find_core_binary(home: &Path) -> Result<PathBuf, LaunchError> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("JARVIS_CORE_BIN")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    candidates.push(home.join("core/target/release").join(exe("jarvis")));
    candidates.push(home.join("core/target/debug").join(exe("jarvis")));
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or(LaunchError::CoreNotFound(candidates))
}

/// Preferisce l'ambiente virtuale creato da `jarvis.sh`, poi l'interprete di sistema.
pub fn find_python(home: &Path) -> String {
    if let Ok(p) = std::env::var("JARVIS_PYTHON") {
        return p;
    }
    let venv = if cfg!(windows) {
        home.join(".venv/Scripts/python.exe")
    } else {
        home.join(".venv/bin/python")
    };
    if venv.is_file() {
        return venv.to_string_lossy().into_owned();
    }
    if cfg!(windows) {
        "python".into()
    } else {
        "python3".into()
    }
}

/// Porta TCP libera su loopback (assegnata dal sistema operativo).
pub fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

/// Core in esecuzione. Alla distruzione il processo viene terminato.
pub struct RunningCore {
    child: Child,
    /// Finché resta aperto, il Core continua a funzionare; si chiude anche se questo processo muore.
    parent_pipe: Option<ChildStdin>,
    pub port: u16,
    pub url: String,
    pub log: PathBuf,
    /// Token di sessione del Core, generato qui: serve all'app per leggere le notifiche (ADR-0044).
    pub token: String,
}

impl RunningCore {
    /// Solo per i test: simula la morte del processo padre chiudendo il canale.
    #[doc(hidden)]
    pub fn close_parent_pipe_for_tests(&mut self) {
        self.parent_pipe.take();
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self) {
        self.parent_pipe.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for RunningCore {
    fn drop(&mut self) {
        self.stop();
    }
}

fn healthy(port: u16) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(2))
        .build();
    // Host deve essere uno di quelli ammessi dal Core (difesa dal DNS rebinding).
    agent
        .get(&format!("http://127.0.0.1:{port}/api/health"))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}

/// Avvia il Core e attende che sia pronto.
/// Token casuale di 32 caratteri esadecimali (128 bit) dal generatore del sistema.
fn session_token() -> String {
    let mut raw = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut raw))
        .is_ok();
    if !ok {
        // Windows: niente /dev/urandom; RandomState usa chiavi casuali del sistema per ogni istanza.
        use std::hash::{BuildHasher, Hasher};
        for chunk in raw.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
        }
    }
    raw.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn launch(cfg: &LaunchConfig) -> Result<RunningCore, LaunchError> {
    std::fs::create_dir_all(&cfg.data_dir).map_err(|e| LaunchError::Io(e.to_string()))?;
    let log = cfg.data_dir.join("core.log");
    let out: File = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| LaunchError::Io(e.to_string()))?;
    let err = out
        .try_clone()
        .map_err(|e| LaunchError::Io(e.to_string()))?;
    let port = free_port().map_err(|e| LaunchError::Io(e.to_string()))?;
    let token = session_token();
    let mut cmd = Command::new(&cfg.core_bin);
    // L'app desktop è un programma grafico: su Windows il Core (programma da console) non deve aprire
    // una finestra di console.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.env("JARVIS_PORT", port.to_string())
        .env("JARVIS_HOST", "127.0.0.1")
        .env("JARVIS_DATA_DIR", &cfg.data_dir)
        .env("JARVIS_HOME", &cfg.home)
        .env("JARVIS_PYTHON", &cfg.python)
        .current_dir(&cfg.home)
        .env("JARVIS_EXIT_WITH_PARENT", "stdin")
        .env("JARVIS_TOKEN", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| LaunchError::Io(format!("avvio di {}: {e}", cfg.core_bin.display())))?;
    let parent_pipe = child.stdin.take();
    let mut core = RunningCore {
        child,
        parent_pipe,
        token,
        port,
        url: format!("http://localhost:{port}/"),
        log: log.clone(),
    };
    let deadline = Instant::now() + cfg.startup_timeout;
    while Instant::now() < deadline {
        if let Ok(Some(status)) = core.child.try_wait() {
            return Err(LaunchError::Exited {
                code: status.code(),
                log,
            });
        }
        if healthy(port) {
            return Ok(core);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    core.stop();
    Err(LaunchError::Timeout { log })
}
