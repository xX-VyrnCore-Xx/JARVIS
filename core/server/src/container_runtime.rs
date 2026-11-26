use std::path::PathBuf;
use std::process::Command;

fn docker_binary() -> PathBuf {
    #[cfg(target_os = "macos")]
    for path in [
        "/usr/local/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
    ] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("ProgramFiles") {
        let path = PathBuf::from(root).join("Docker/Docker/resources/bin/docker.exe");
        if path.is_file() {
            return path;
        }
    }
    "docker".into()
}

fn local_docker_host() -> String {
    #[cfg(unix)]
    {
        let mut sockets = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(home) = std::env::var_os("HOME") {
            sockets.push(PathBuf::from(home).join(".docker/run/docker.sock"));
        }
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            sockets.push(PathBuf::from(dir).join("docker.sock"));
        }
        sockets.push(PathBuf::from("/var/run/docker.sock"));
        if let Some(socket) = sockets
            .iter()
            .find(|socket| socket.is_absolute() && socket.exists())
        {
            return format!("unix://{}", socket.display());
        }
        "unix:///var/run/docker.sock".into()
    }
    #[cfg(windows)]
    {
        "npipe:////./pipe/docker_engine".into()
    }
}

/// `PATH` ampliato con le cartelle note di Docker: un'app avviata dal Finder o da un servizio ha un `PATH`
/// minimo e non trova gli helper (`docker-credential-desktop`, plugin) installati con Docker Desktop.
fn docker_path() -> Option<std::ffi::OsString> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let mut extra: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    extra.extend([
        "/usr/local/bin".into(),
        "/opt/homebrew/bin".into(),
        "/Applications/Docker.app/Contents/Resources/bin".into(),
    ]);
    #[cfg(all(unix, not(target_os = "macos")))]
    extra.extend(["/usr/local/bin".into(), "/usr/bin".into()]);
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("ProgramFiles") {
        extra.push(PathBuf::from(root).join("Docker/Docker/resources/bin"));
    }
    for dir in extra {
        if dir.is_dir() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).ok()
}

pub(crate) fn docker_command() -> Command {
    let mut command = Command::new(docker_binary());
    if let Some(path) = docker_path() {
        command.env("PATH", path);
    }
    command
        .env_remove("DOCKER_HOST")
        .env_remove("DOCKER_CONTEXT")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .arg("--host")
        .arg(local_docker_host());
    crate::hide_console(&mut command);
    command
}

/// Cartella di configurazione Docker privata e scrivibile, con un `config.json` vuoto: le immagini pubbliche non
/// richiedono credenziali, quindi si evitano l'helper delle credenziali e `~/.docker` non scrivibile.
pub fn private_config_dir(base: &std::path::Path) -> Result<PathBuf, String> {
    let dir = base.join("docker-config");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cartella di configurazione Docker non creabile: {e}"))?;
    let file = dir.join("config.json");
    if !file.exists() {
        std::fs::write(&file, "{}\n")
            .map_err(|e| format!("configurazione Docker non scrivibile: {e}"))?;
    }
    Ok(dir)
}

/// Spiega un errore di `docker pull` con la causa probabile e il rimedio (in italiano).
pub fn explain_pull_error(detail: &str) -> String {
    let t = detail.to_ascii_lowercase();
    let remedy = if t.contains("docker.sock") && t.contains("permission denied")
        || t.contains("access is denied")
    {
        "l'utente non può usare il motore Docker: su Linux aggiungilo al gruppo «docker» e riaccedi; su Windows avvia Docker Desktop con lo stesso utente"
    } else if t.contains("credential") || t.contains("keychain") {
        "l'helper delle credenziali di Docker non risponde: sblocca il portachiavi o apri Docker Desktop una volta"
    } else if t.contains("permission denied") {
        "Docker non può scrivere nella sua cartella di lavoro: controlla i permessi di ~/.docker e dello spazio su disco"
    } else if t.contains("toomanyrequests") || t.contains("rate limit") {
        "il registro ha raggiunto il limite di richieste: riprova più tardi"
    } else if t.contains("unauthorized") || t.contains("denied: requested access") {
        "il registro rifiuta l'accesso all'immagine: controlla il nome e la rete aziendale"
    } else if t.contains("no such host")
        || t.contains("timeout")
        || t.contains("tls")
        || t.contains("eof")
    {
        "il registro non è raggiungibile: controlla la connessione, la VPN e il proxy"
    } else if t.contains("no space left") {
        "spazio su disco esaurito: libera spazio e riprova"
    } else {
        return detail.to_string();
    };
    format!("{detail} — {remedy}")
}

/// Stato di Docker visto dal Core (ADR-0113).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    NotInstalled,
    DaemonDown,
    PermissionDenied,
    Unknown,
    Ready,
}

impl Health {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::DaemonDown => "daemon_down",
            Self::PermissionDenied => "permission_denied",
            Self::Unknown => "unknown",
            Self::Ready => "ready",
        }
    }

    /// Cosa fare, in italiano semplice.
    pub fn hint(self) -> &'static str {
        match self {
            Self::NotInstalled => "Docker non è installato: installa Docker Desktop (macOS, Windows) o il motore Docker (Linux), poi riprova",
            Self::DaemonDown => "Docker è installato ma non è in esecuzione: avvia Docker Desktop (o il servizio docker) e attendi che sia pronto",
            Self::PermissionDenied => "Docker rifiuta l'accesso a questo utente: su Linux aggiungi l'utente al gruppo «docker» e riaccedi",
            Self::Unknown => "Docker non risponde come previsto: controlla che sia avviato e aggiornato",
            Self::Ready => "Docker è pronto",
        }
    }
}

/// Classifica l'esito di `docker version` (funzione pura, provata nei test).
pub fn classify(spawn_error: Option<std::io::ErrorKind>, succeeded: bool, stderr: &str) -> Health {
    if let Some(kind) = spawn_error {
        return if kind == std::io::ErrorKind::NotFound {
            Health::NotInstalled
        } else if kind == std::io::ErrorKind::PermissionDenied {
            Health::PermissionDenied
        } else {
            Health::Unknown
        };
    }
    if succeeded {
        return Health::Ready;
    }
    let t = stderr.to_ascii_lowercase();
    if t.contains("permission denied") || t.contains("access is denied") {
        Health::PermissionDenied
    } else if t.contains("cannot connect")
        || t.contains("is the docker daemon running")
        || t.contains("error during connect")
        || t.contains("no such file or directory")
        || t.contains("the system cannot find the file")
        || t.contains("connection refused")
    {
        Health::DaemonDown
    } else {
        Health::Unknown
    }
}

/// Interroga il motore con un limite di 6 secondi: (stato, versione del server).
pub fn diagnose() -> (Health, Option<String>) {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut child = match docker_command()
        .args(["version", "--format", "{{.Server.Version}}"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return (classify(Some(error.kind()), false, ""), None),
    };
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(6) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let read = |stream: Option<&mut dyn Read>| {
        let mut text = String::new();
        if let Some(stream) = stream {
            let _ = stream.take(4096).read_to_string(&mut text);
        }
        text
    };
    let stdout = read(child.stdout.as_mut().map(|s| s as &mut dyn Read));
    let stderr = read(child.stderr.as_mut().map(|s| s as &mut dyn Read));
    match status {
        Some(status) => {
            let health = classify(None, status.success(), &stderr);
            let version = (health == Health::Ready)
                .then(|| stdout.trim().to_string())
                .filter(|v| !v.is_empty());
            (health, version)
        }
        None => (Health::DaemonDown, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_command_pins_an_explicit_local_endpoint() {
        let command = docker_command();
        let mut args = command.get_args();
        assert_eq!(args.next(), Some(std::ffi::OsStr::new("--host")));
        let endpoint = args.next().unwrap().to_string_lossy();
        assert!(endpoint.starts_with("unix://") || endpoint.starts_with("npipe://"));
    }

    #[test]
    fn docker_failures_are_classified_with_a_remedy() {
        use std::io::ErrorKind::*;
        assert_eq!(classify(Some(NotFound), false, ""), Health::NotInstalled);
        assert_eq!(
            classify(Some(PermissionDenied), false, ""),
            Health::PermissionDenied
        );
        assert_eq!(classify(None, true, ""), Health::Ready);
        assert_eq!(
            classify(None, false, "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"),
            Health::DaemonDown
        );
        assert_eq!(
            classify(
                None,
                false,
                "permission denied while trying to connect to the Docker daemon socket"
            ),
            Health::PermissionDenied
        );
        assert_eq!(
            classify(None, false, "error during connect: open //./pipe/docker_engine: The system cannot find the file specified."),
            Health::DaemonDown
        );
        assert_eq!(classify(None, false, "boh"), Health::Unknown);
        assert!(Health::DaemonDown.hint().contains("avvia"));
    }

    #[test]
    fn pull_errors_get_a_remedy_and_the_private_config_is_created() {
        let socket = explain_pull_error("permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock");
        assert!(socket.contains("gruppo «docker»"), "{socket}");
        let creds = explain_pull_error("error getting credentials - err: exec: \"docker-credential-desktop\": executable file not found");
        assert!(creds.contains("credenziali"), "{creds}");
        let config = explain_pull_error("mkdir /home/u/.docker: permission denied");
        assert!(config.contains("cartella di lavoro"), "{config}");
        assert_eq!(explain_pull_error("boh"), "boh");
        let base = std::env::temp_dir().join(format!(
            "jarvis-dockercfg-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let dir = private_config_dir(&base).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("config.json"))
                .unwrap()
                .trim(),
            "{}"
        );
        let _ = std::fs::remove_dir_all(base);
    }
}
