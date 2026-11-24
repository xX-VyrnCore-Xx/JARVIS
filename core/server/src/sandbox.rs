use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

const IMAGE: &str = "python:3.13-alpine";
/// Etichetta di tutti i container del sandbox: permette di ritrovare quelli rimasti dopo un arresto brusco.
const LABEL: &str = "jarvis.sandbox=1";
/// Dockerfile dell'immagine completa, incorporato per costruirla anche senza il repository (ADR-0114).
const DOCKERFILE: &str = include_str!("../../../infra/sandbox/Dockerfile");
const SOURCE_LIMIT: u64 = 65_536;
const OUTPUT_LIMIT: u64 = 8_192;
const DEADLINE: Duration = Duration::from_secs(12);

fn error(code: &'static str, message: &str) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

pub fn check_python(workspace: &Path, path: &str) -> Result<Value, ToolError> {
    let file = resolve(workspace, path, true)?;
    if !file.is_file() || file.extension().and_then(|ext| ext.to_str()) != Some("py") {
        return Err(error(
            "code.invalid_source",
            "È richiesto un file .py nella workspace",
        ));
    }
    let mut source = Vec::new();
    std::fs::File::open(file)
        .and_then(|f| f.take(SOURCE_LIMIT + 1).read_to_end(&mut source))
        .map_err(|_| error("code.invalid_source", "File Python non leggibile"))?;
    if source.len() as u64 > SOURCE_LIMIT {
        return Err(error("code.invalid_source", "Il file Python supera 64 KiB"));
    }
    isolated_python(source, &["-c", "import ast,sys; ast.parse(sys.stdin.buffer.read(), filename='workspace.py'); print('OK')"])
}

pub fn run_python(source: &str) -> Result<Value, ToolError> {
    if source.len() as u64 > SOURCE_LIMIT {
        return Err(error(
            "code.invalid_source",
            "Il codice Python supera 64 KiB",
        ));
    }
    isolated_python(source.as_bytes().to_vec(), &["-"])
}

fn docker() -> Command {
    crate::container_runtime::docker_command()
}

fn limited_output(mut stream: impl Read) -> std::io::Result<(String, bool)> {
    let mut kept = Vec::new();
    stream
        .by_ref()
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut kept)?;
    let truncated = kept.len() as u64 > OUTPUT_LIMIT;
    kept.truncate(OUTPUT_LIMIT as usize);
    std::io::copy(&mut stream, &mut std::io::sink())?;
    Ok((String::from_utf8_lossy(&kept).into_owned(), truncated))
}

fn isolated_python(source: Vec<u8>, args: &[&str]) -> Result<Value, ToolError> {
    let name = format!("jarvis-python-{}", uuid::Uuid::new_v4().simple());
    let mut child = docker()
        .args([
            "run",
            "--rm",
            "--init",
            "--pull",
            "never",
            "--name",
            &name,
            "--label",
            LABEL,
            "--network",
            "none",
            "--read-only",
            "--user",
            "65534:65534",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--pids-limit",
            "24",
            "--memory",
            "128m",
            "--memory-swap",
            "128m",
            "--cpus",
            "0.5",
            "--ulimit",
            "cpu=5:5",
            "-i",
            IMAGE,
            "python",
            "-I",
            "-S",
            "-B",
        ])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| error("code.unavailable", "Docker locale non disponibile"))?;
    let mut input = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let write = std::thread::spawn(move || input.write_all(&source));
    let out = std::thread::spawn(move || limited_output(stdout));
    let err = std::thread::spawn(move || limited_output(stderr));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < DEADLINE => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // Il nome è univoco per questa chiamata. Il limite CPU resta attivo anche se il client si stacca.
                if let Ok(mut cleanup) = docker()
                    .args(["rm", "-f", &name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    std::thread::spawn(move || {
                        let _ = cleanup.wait();
                    });
                }
                break None;
            }
        }
    };
    write
        .join()
        .map_err(|_| {
            error(
                "code.input_failed",
                "Invio del codice al container non riuscito",
            )
        })?
        .map_err(|_| {
            error(
                "code.input_failed",
                "Invio del codice al container non riuscito",
            )
        })?;
    let (stdout, out_cut) = out
        .join()
        .map_err(|_| {
            error(
                "code.output_failed",
                "Lettura dell'output del container non riuscita",
            )
        })?
        .map_err(|_| {
            error(
                "code.output_failed",
                "Lettura dell'output del container non riuscita",
            )
        })?;
    let (stderr, err_cut) = err
        .join()
        .map_err(|_| {
            error(
                "code.output_failed",
                "Lettura dell'output del container non riuscita",
            )
        })?
        .map_err(|_| {
            error(
                "code.output_failed",
                "Lettura dell'output del container non riuscita",
            )
        })?;
    let status =
        status.ok_or_else(|| error("code.timeout", "Sandbox interrotta per limite di tempo"))?;
    if matches!(status.code(), Some(125..=127)) {
        return Err(error(
            "code.unavailable",
            "Verifica Docker locale e installa l’immagine python:3.13-alpine",
        ));
    }
    Ok(
        json!({"succeeded": status.success(), "exit_code": status.code(), "stdout": stdout,
              "stderr": stderr, "truncated": out_cut || err_cut, "sandbox": "docker"}),
    )
}

// ---------------------------------------------------------------- terminale sandbox (ADR-0105)

const TERMINAL_IMAGE: &str = "jarvis-sandbox:latest";
const FALLBACK_IMAGE: &str = "python:3.13-alpine";
const TERMINAL_OUTPUT: u64 = 16_384;
const COMMAND_LIMIT: usize = 4_000;
const SESSION_SOFT_LIMIT: u64 = 256 * 1024 * 1024;
const SESSION_HARD_LIMIT: u64 = 1024 * 1024 * 1024;
const SETTINGS_FILE: &str = "sandbox.json";

/// Il terminale sandbox è attivo salvo interruttore del proprietario o `JARVIS_SANDBOX=off`.
pub fn enabled(data_dir: &Path) -> bool {
    if std::env::var("JARVIS_SANDBOX").is_ok_and(|v| matches!(v.as_str(), "off" | "0" | "false")) {
        return false;
    }
    std::fs::read(data_dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["enabled"].as_bool())
        .unwrap_or(true)
}

pub fn settings(data_dir: &Path) -> Value {
    json!({"enabled": enabled(data_dir),
           "forced_off": std::env::var("JARVIS_SANDBOX").is_ok_and(|v| matches!(v.as_str(), "off" | "0" | "false"))})
}

pub fn update(data_dir: &Path, patch: &Value) -> Result<Value, String> {
    let on = patch["enabled"]
        .as_bool()
        .ok_or("campo «enabled» booleano richiesto")?;
    let body = serde_json::to_vec(&json!({"enabled": on})).map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join(SETTINGS_FILE), body).map_err(|e| e.to_string())?;
    Ok(settings(data_dir))
}

fn sandbox_root(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("sandbox")
}

pub(crate) fn valid_session(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn session_name(args: &Value) -> Result<String, ToolError> {
    let name = args["session"].as_str().unwrap_or("main");
    if valid_session(name) {
        Ok(name.to_string())
    } else {
        Err(error(
            "sandbox.invalid_session",
            "Sessione non valida: usa 1-32 caratteri tra a-z, 0-9 e trattino",
        ))
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(m) if m.is_dir() => stack.push(entry.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
        if total > SESSION_HARD_LIMIT * 2 {
            break;
        }
    }
    total
}

fn ensure_enabled(data_dir: &Path) -> Result<(), ToolError> {
    if enabled(data_dir) {
        Ok(())
    } else {
        Err(error(
            "sandbox.disabled",
            "Il terminale sandbox è disattivato dal proprietario (Impostazioni → Sandbox)",
        ))
    }
}

fn image_present(name: &str) -> bool {
    docker()
        .args(["image", "inspect", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Immagine da usare: `JARVIS_SANDBOX_IMAGE`, poi `jarvis-sandbox:latest`, poi l'immagine Python minimale.
fn pick_image() -> Result<String, ToolError> {
    if let Ok(custom) = std::env::var("JARVIS_SANDBOX_IMAGE") {
        let ok = !custom.is_empty()
            && custom
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-:/@".contains(&b));
        if ok && image_present(&custom) {
            return Ok(custom);
        }
    }
    for candidate in [TERMINAL_IMAGE, FALLBACK_IMAGE] {
        if image_present(candidate) {
            return Ok(candidate.to_string());
        }
    }
    let (health, _) = crate::container_runtime::diagnose();
    let message = if health == crate::container_runtime::Health::Ready {
        "Docker è pronto ma manca l'immagine del sandbox: chiedi «prepara la sandbox» (serve la rete una volta sola)".to_string()
    } else {
        health.hint().to_string()
    };
    Err(ToolError {
        code: "sandbox.unavailable",
        message,
    })
}

#[cfg(unix)]
fn container_user(dir: &Path) -> String {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(dir)
        .map(|m| format!("{}:{}", m.uid(), m.gid()))
        .unwrap_or_else(|_| "65534:65534".into())
}

#[cfg(not(unix))]
fn container_user(_dir: &Path) -> String {
    "65534:65534".into()
}

fn session_dir(data_dir: &Path, session: &str) -> Result<std::path::PathBuf, ToolError> {
    let dir = sandbox_root(data_dir).join(session);
    std::fs::create_dir_all(&dir).map_err(|_| {
        error(
            "sandbox.unavailable",
            "Cartella della sessione non creabile",
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            sandbox_root(data_dir),
            std::fs::Permissions::from_mode(0o700),
        );
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

/// Profilo di risorse di un'esecuzione (ADR-0147): stesso isolamento per tutti (nessuna rete, radice in sola
/// lettura, nessuna capability), cambiano solo i limiti di memoria, CPU, processi e tempo.
pub struct Profile {
    pub name: &'static str,
    pub summary: &'static str,
    pub memory_mb: u32,
    pub cpus: &'static str,
    pub pids: u32,
    pub tmp_mb: u32,
    pub max_timeout_s: u64,
}

pub const PROFILES: [Profile; 4] = [
    Profile {
        name: "light",
        summary: "Controlli rapidi e piccoli script",
        memory_mb: 256,
        cpus: "0.5",
        pids: 64,
        tmp_mb: 32,
        max_timeout_s: 10,
    },
    Profile {
        name: "standard",
        summary: "Uso generale (predefinito)",
        memory_mb: 512,
        cpus: "1",
        pids: 128,
        tmp_mb: 64,
        max_timeout_s: 60,
    },
    Profile {
        name: "build",
        summary: "Compilazioni e test più pesanti",
        memory_mb: 1024,
        cpus: "2",
        pids: 256,
        tmp_mb: 256,
        max_timeout_s: 60,
    },
    Profile {
        name: "data",
        summary: "Elaborazione di dati in memoria",
        memory_mb: 1024,
        cpus: "1",
        pids: 128,
        tmp_mb: 256,
        max_timeout_s: 60,
    },
];

pub fn profile(name: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.name == name)
}

fn standard() -> &'static Profile {
    &PROFILES[1]
}

/// Argomenti di `docker run` del terminale sandbox. Isolamento: nessuna rete, radice di sola lettura,
/// nessuna capability, niente mount del computer oltre la cartella della sessione.
#[cfg(test)]
pub(crate) fn terminal_args(
    name: &str,
    image: &str,
    dir: &Path,
    user: &str,
    command: &str,
) -> Vec<String> {
    terminal_args_with(standard(), name, image, dir, user, command)
}

pub(crate) fn terminal_args_with(
    profile: &Profile,
    name: &str,
    image: &str,
    dir: &Path,
    user: &str,
    command: &str,
) -> Vec<String> {
    let mount = format!("{}:/work:rw", dir.display());
    let tmpfs = format!("/tmp:rw,size={}m", profile.tmp_mb);
    let memory = format!("{}m", profile.memory_mb);
    let pids = profile.pids.to_string();
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--pull",
        "never",
        "--name",
        name,
        "--label",
        LABEL,
        "--ipc",
        "none",
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        &tmpfs,
        "--user",
        user,
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        &pids,
        "--memory",
        &memory,
        "--memory-swap",
        &memory,
        "--cpus",
        profile.cpus,
        "--ulimit",
        "nofile=256:256",
        "--ulimit",
        "fsize=67108864:67108864",
        "-v",
        &mount,
        "-w",
        "/work",
        "-e",
        "HOME=/work",
        "-e",
        "TMPDIR=/tmp",
        image,
        "sh",
        "-c",
        command,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.shrink_to_fit();
    args
}

fn spawn_cleanup(name: &str) {
    if let Ok(mut cleanup) = docker()
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = cleanup.wait();
        });
    }
}

fn bounded(stream: impl Read) -> std::io::Result<(String, bool)> {
    let mut kept = Vec::new();
    let mut stream = stream;
    stream
        .by_ref()
        .take(TERMINAL_OUTPUT + 1)
        .read_to_end(&mut kept)?;
    let truncated = kept.len() as u64 > TERMINAL_OUTPUT;
    kept.truncate(TERMINAL_OUTPUT as usize);
    std::io::copy(&mut stream, &mut std::io::sink())?;
    Ok((String::from_utf8_lossy(&kept).into_owned(), truncated))
}

pub fn terminal_run(data_dir: &Path, args: &Value) -> Result<Value, ToolError> {
    ensure_enabled(data_dir)?;
    let command = args["command"].as_str().unwrap_or_default();
    if command.trim().is_empty() || command.chars().count() > COMMAND_LIMIT {
        return Err(error(
            "sandbox.invalid_command",
            "Il comando deve avere da 1 a 4000 caratteri",
        ));
    }
    let session = session_name(args)?;
    let profile = match args["profile"].as_str() {
        None => standard(),
        Some(name) => profile(name).ok_or_else(|| {
            error(
                "sandbox.invalid_profile",
                "Profilo sconosciuto: usa light, standard, build o data",
            )
        })?,
    };
    let timeout = args["timeout_s"]
        .as_u64()
        .unwrap_or(20)
        .clamp(1, profile.max_timeout_s);
    let dir = session_dir(data_dir, &session)?;
    let used = dir_size(&dir);
    if used > SESSION_HARD_LIMIT {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = session_dir(data_dir, &session)?;
        return Err(error(
            "sandbox.session_full",
            "La sessione superava 1 GiB ed è stata azzerata: riprova",
        ));
    }
    if used > SESSION_SOFT_LIMIT {
        return Err(error(
            "sandbox.session_full",
            "La sessione supera 256 MiB: elimina file o usa sandbox.reset",
        ));
    }
    let image = pick_image()?;
    let name = format!("jarvis-sandbox-{}", uuid::Uuid::new_v4().simple());
    let user = container_user(&dir);
    let started = Instant::now();
    let mut child = docker()
        .args(terminal_args_with(
            profile, &name, &image, &dir, &user, command,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| error("sandbox.unavailable", "Docker locale non disponibile"))?;
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let out = std::thread::spawn(move || bounded(stdout));
    let err = std::thread::spawn(move || bounded(stderr));
    let limit = Duration::from_secs(timeout);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                spawn_cleanup(&name);
                break None;
            }
        }
    };
    let read = |h: std::thread::JoinHandle<std::io::Result<(String, bool)>>| {
        h.join()
            .ok()
            .and_then(Result::ok)
            .ok_or_else(|| error("sandbox.output_failed", "Lettura dell'output non riuscita"))
    };
    let (stdout, out_cut) = read(out)?;
    let (stderr, err_cut) = read(err)?;
    let Some(status) = status else {
        return Err(error(
            "sandbox.timeout",
            "Comando interrotto: tempo massimo superato",
        ));
    };
    if matches!(status.code(), Some(125..=127)) && stdout.is_empty() && stderr.contains("docker") {
        return Err(error(
            "sandbox.unavailable",
            "Docker non ha avviato il container: verifica Docker e l'immagine",
        ));
    }
    Ok(json!({
        "succeeded": status.success(), "exit_code": status.code(), "stdout": stdout,
        "stderr": stderr, "truncated": out_cut || err_cut, "session": session, "profile": profile.name,
        "image": image, "duration_ms": started.elapsed().as_millis() as u64, "sandbox": "docker",
    }))
}

const IMPORT_LIMIT: u64 = 8 * 1024 * 1024;
const EXPORT_LIMIT: u64 = 1024 * 1024;

/// Unisce `rel` a `base` rifiutando percorsi fuori da `base` e qualunque collegamento simbolico già presente:
/// il codice nel container può creare link verso percorsi che sul computer ospite hanno un altro significato.
fn join_without_links(base: &Path, rel: &str) -> Result<std::path::PathBuf, ToolError> {
    if rel.is_empty() || !crate::tools::lexically_inside(rel) {
        return Err(error(
            "sandbox.invalid_path",
            "Percorso non ammesso: usa un percorso relativo senza «..»",
        ));
    }
    let mut path = base.to_path_buf();
    for part in Path::new(rel).components() {
        if let std::path::Component::Normal(name) = part {
            path.push(name);
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(error(
                    "sandbox.invalid_path",
                    "Collegamento simbolico non ammesso nella sessione",
                ));
            }
        }
    }
    Ok(path)
}

/// Copia un file della workspace nella sessione (ADR-0106). Sola lettura sulla workspace.
pub fn import_file(data_dir: &Path, workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    ensure_enabled(data_dir)?;
    let from = args["from"].as_str().unwrap_or_default();
    let source = resolve(workspace, from, true)?;
    let meta = std::fs::metadata(&source)
        .map_err(|_| error("sandbox.invalid_path", "File della workspace non leggibile"))?;
    if !meta.is_file() {
        return Err(error("sandbox.invalid_path", "È ammesso solo un file"));
    }
    if meta.len() > IMPORT_LIMIT {
        return Err(error("sandbox.too_large", "Il file supera 8 MiB"));
    }
    let session = session_name(args)?;
    let default_name = Path::new(from)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let to = args["to"].as_str().unwrap_or(default_name);
    let root = session_dir(data_dir, &session)?;
    if dir_size(&root) + meta.len() > SESSION_SOFT_LIMIT {
        return Err(error("sandbox.session_full", "La sessione è piena"));
    }
    let target = join_without_links(&root, to)?;
    if target.is_dir() {
        return Err(error(
            "sandbox.invalid_path",
            "La destinazione è una cartella",
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|_| {
            error(
                "sandbox.unavailable",
                "Cartella di destinazione non creabile",
            )
        })?;
    }
    std::fs::copy(&source, &target)
        .map_err(|_| error("sandbox.unavailable", "Copia nella sessione non riuscita"))?;
    Ok(json!({"session": session, "path": to, "bytes": meta.len()}))
}

/// Copia un file della sessione nella workspace (ADR-0106). Scrive sul computer: richiede conferma.
pub fn export_file(data_dir: &Path, workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    ensure_enabled(data_dir)?;
    let session = session_name(args)?;
    let from = args["from"].as_str().unwrap_or_default();
    let root = sandbox_root(data_dir).join(&session);
    let source = join_without_links(&root, from)?;
    let meta = std::fs::symlink_metadata(&source)
        .map_err(|_| error("sandbox.not_found", "File non presente nella sessione"))?;
    if !meta.is_file() {
        return Err(error("sandbox.invalid_path", "È ammesso solo un file"));
    }
    if meta.len() > EXPORT_LIMIT {
        return Err(error("sandbox.too_large", "Il file supera 1 MiB"));
    }
    let to = args["to"].as_str().unwrap_or_default();
    let target = resolve(workspace, to, false)?;
    if target.is_dir() {
        return Err(error(
            "sandbox.invalid_path",
            "La destinazione è una cartella",
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|_| {
            error(
                "sandbox.unavailable",
                "Cartella di destinazione non creabile",
            )
        })?;
    }
    std::fs::copy(&source, &target)
        .map_err(|_| error("sandbox.unavailable", "Copia nella workspace non riuscita"))?;
    Ok(json!({"session": session, "from": from, "path": to, "bytes": meta.len()}))
}

// ---------------------------------------------------------------- immagine e pulizia (ADR-0114, ADR-0115)

#[derive(Clone, Default)]
struct Build {
    status: &'static str, // idle | building | ready | failed
    detail: String,
}

fn build_state() -> &'static std::sync::Mutex<Build> {
    static S: std::sync::OnceLock<std::sync::Mutex<Build>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        std::sync::Mutex::new(Build {
            status: "idle",
            ..Build::default()
        })
    })
}

pub fn build_status() -> Value {
    let b = build_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    json!({"status": b.status, "detail": if b.detail.is_empty() { Value::Null } else { Value::String(b.detail) }})
}

fn set_build(status: &'static str, detail: impl Into<String>) {
    let mut b = build_state().lock().unwrap_or_else(|e| e.into_inner());
    b.status = status;
    b.detail = detail.into();
}

/// Costruisce `jarvis-sandbox:latest` in background dal Dockerfile incorporato (serve la rete, solo qui).
pub fn prepare_image(data_dir: &Path) -> Result<Value, ToolError> {
    ensure_enabled(data_dir)?;
    let (health, _) = crate::container_runtime::diagnose();
    if health != crate::container_runtime::Health::Ready {
        return Err(ToolError {
            code: "sandbox.unavailable",
            message: health.hint().to_string(),
        });
    }
    {
        let mut b = build_state().lock().unwrap_or_else(|e| e.into_inner());
        if b.status == "building" {
            return Err(error(
                "sandbox.busy",
                "La costruzione dell'immagine è già in corso",
            ));
        }
        b.status = "building";
        b.detail = String::new();
    }
    std::thread::spawn(|| match run_build() {
        Ok(()) => set_build("ready", ""),
        Err(message) => set_build("failed", message),
    });
    Ok(json!({"started": true, "image": TERMINAL_IMAGE}))
}

fn run_build() -> Result<(), String> {
    let mut child = docker()
        .args([
            "build",
            "--pull",
            "-t",
            TERMINAL_IMAGE,
            "--label",
            LABEL,
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Docker non disponibile".to_string())?;
    let mut input = child.stdin.take().expect("stdin");
    let writer = std::thread::spawn(move || input.write_all(DOCKERFILE.as_bytes()));
    let mut stderr = child.stderr.take().expect("stderr");
    let log = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            kept.extend_from_slice(&buf[..n]);
            if kept.len() > 6000 {
                kept.drain(..kept.len() - 3000);
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(900) => {
                std::thread::sleep(Duration::from_millis(200))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let _ = writer.join();
    let tail = log.join().unwrap_or_default();
    match status {
        Some(s) if s.success() => Ok(()),
        Some(_) => Err(build_failure(&tail)),
        None => Err("Costruzione interrotta: oltre 15 minuti".into()),
    }
}

/// Riduce la coda del log di `docker build` a una causa leggibile.
pub(crate) fn build_failure(log: &str) -> String {
    let t = log.to_ascii_lowercase();
    if [
        "no such host",
        "temporary failure in name resolution",
        "dial tcp",
        "tls handshake",
        "i/o timeout",
    ]
    .iter()
    .any(|k| t.contains(k))
    {
        "Costruzione non riuscita: Docker non raggiunge Internet (serve solo per scaricare l'immagine di base e i pacchetti)".into()
    } else if t.contains("no space left") {
        "Costruzione non riuscita: spazio su disco esaurito per Docker".into()
    } else if t.contains("denied") || t.contains("unauthorized") {
        "Costruzione non riuscita: accesso negato al registro delle immagini".into()
    } else {
        let line = log
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("errore sconosciuto");
        format!(
            "Costruzione non riuscita: {}",
            line.chars().take(200).collect::<String>()
        )
    }
}

/// Elimina i container del sandbox rimasti dopo un arresto brusco del Core.
pub fn cleanup_orphans() -> usize {
    let Ok(out) = docker()
        .args(["ps", "-aq", "--filter", &format!("label={LABEL}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return 0;
    };
    let ids = parse_container_ids(&String::from_utf8_lossy(&out.stdout));
    if ids.is_empty() {
        return 0;
    }
    let _ = docker()
        .args(["rm", "-f"])
        .args(&ids)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    ids.len()
}

/// Identificativi validi (esadecimali) di `docker ps -q`: tutto il resto è ignorato.
pub(crate) fn parse_container_ids(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| (6..=64).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string)
        .collect()
}

/// Dimensione in byte di ogni sessione esistente.
pub fn session_sizes(data_dir: &Path) -> Vec<(String, u64)> {
    std::fs::read_dir(sandbox_root(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            valid_session(&name).then(|| (name, dir_size(&e.path())))
        })
        .collect()
}

pub fn terminal_status(data_dir: &Path) -> Value {
    let root = sandbox_root(data_dir);
    let mut sessions: Vec<Value> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            valid_session(&name).then(|| json!({"session": name, "bytes": dir_size(&e.path())}))
        })
        .collect();
    sessions.sort_by(|a, b| a["session"].as_str().cmp(&b["session"].as_str()));
    let (health, version) = crate::container_runtime::diagnose();
    let image = (health == crate::container_runtime::Health::Ready)
        .then(|| pick_image().ok())
        .flatten();
    json!({
        "enabled": enabled(data_dir), "available": image.is_some(), "image": image,
        "docker": {"state": health.code(), "version": version, "hint": health.hint()},
        "build": build_status(),
        "limits": {"timeout_s_max": 60, "memory_mb": 512, "cpus": 1, "pids": 128,
                   "output_bytes": TERMINAL_OUTPUT, "session_mb": SESSION_SOFT_LIMIT / 1024 / 1024,
                   "network": "none"},
        "profiles": PROFILES.iter().map(|p| json!({
            "name": p.name, "summary": p.summary, "memory_mb": p.memory_mb, "cpus": p.cpus,
            "pids": p.pids, "tmp_mb": p.tmp_mb, "timeout_s_max": p.max_timeout_s,
        })).collect::<Vec<_>>(),
        "sessions": sessions,
    })
}

pub fn terminal_reset(data_dir: &Path, args: &Value) -> Result<Value, ToolError> {
    ensure_enabled(data_dir)?;
    let session = session_name(args)?;
    let dir = sandbox_root(data_dir).join(&session);
    let existed = dir.is_dir();
    if existed {
        std::fs::remove_dir_all(&dir)
            .map_err(|_| error("sandbox.unavailable", "Sessione non azzerabile"))?;
    }
    Ok(json!({"session": session, "reset": existed}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_output_bounded_while_draining() {
        let input = vec![b'x'; (OUTPUT_LIMIT * 2) as usize];
        let (stdout, truncated) = limited_output(input.as_slice()).unwrap();
        assert!(truncated);
        assert_eq!(stdout.len() as u64, OUTPUT_LIMIT);
    }

    #[test]
    fn oversized_inline_code_is_refused_before_docker() {
        let source = "x".repeat(SOURCE_LIMIT as usize + 1);
        assert_eq!(run_python(&source).unwrap_err().code, "code.invalid_source");
    }

    #[test]
    #[ignore = "richiede Docker locale con python:3.13-alpine"]
    fn python_container_runs_without_network_or_host_workspace() {
        let result = run_python(
            "import os, socket
assert os.getuid() != 0
assert os.getenv('JARVIS_TOKEN') is None
assert not os.path.exists('/workspace')
assert len(socket.if_nameindex()) == 1
assert not os.access('/', os.W_OK)
print(2 + 2)",
        )
        .unwrap();
        assert_eq!(result["succeeded"], true, "{result}");
        assert_eq!(result["stdout"], "4\n");
    }

    #[test]
    fn session_names_are_restricted() {
        assert!(valid_session("main") && valid_session("a-1"));
        for bad in ["", "A", "../x", "a/b", "a b", &"x".repeat(33)] {
            assert!(!valid_session(bad), "{bad}");
        }
    }

    #[test]
    fn terminal_arguments_isolate_the_container() {
        let light = terminal_args_with(
            &PROFILES[0],
            "n",
            "img",
            Path::new("/d/main"),
            "1000:1000",
            "ls",
        )
        .join(" ");
        assert!(
            light.contains("--memory 256m")
                && light.contains("--cpus 0.5")
                && light.contains("--pids-limit 64")
        );
        assert!(
            light.contains("--network none")
                && light.contains("--cap-drop ALL")
                && light.contains("--read-only")
        );
        for p in &PROFILES {
            let joined =
                terminal_args_with(p, "n", "img", Path::new("/d/x"), "1:1", "ls").join(" ");
            assert!(
                joined.contains("--network none") && joined.contains("no-new-privileges"),
                "{}",
                p.name
            );
        }
        assert!(profile("build").is_some() && profile("root").is_none());
        let args = terminal_args("n", "img", Path::new("/d/main"), "1000:1000", "ls");
        let joined = args.join(" ");
        for needle in [
            "--network none",
            "--read-only",
            "--cap-drop ALL",
            "no-new-privileges",
            "--pids-limit 128",
            "--memory 512m",
            "-v /d/main:/work:rw",
            "--user 1000:1000",
        ] {
            assert!(joined.contains(needle), "manca {needle}");
        }
        assert_eq!(&args[args.len() - 3..], ["sh", "-c", "ls"]);
        assert!(!joined.contains("--privileged") && !joined.contains("docker.sock"));
    }

    #[test]
    fn terminal_validates_before_docker_and_honours_the_switch() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-sbx-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            terminal_run(&dir, &json!({"command": "  "}))
                .unwrap_err()
                .code,
            "sandbox.invalid_command"
        );
        assert_eq!(
            terminal_run(&dir, &json!({"command": "ls", "session": "../x"}))
                .unwrap_err()
                .code,
            "sandbox.invalid_session"
        );
        update(&dir, &json!({"enabled": false})).unwrap();
        assert_eq!(
            terminal_run(&dir, &json!({"command": "ls"}))
                .unwrap_err()
                .code,
            "sandbox.disabled"
        );
        update(&dir, &json!({"enabled": true})).unwrap();
        assert!(update(&dir, &json!({"enabled": "x"})).is_err());
        let status = terminal_status(&dir);
        assert_eq!(status["enabled"], true);
        std::fs::create_dir_all(dir.join("sandbox/main")).unwrap();
        std::fs::write(dir.join("sandbox/main/a.txt"), b"abc").unwrap();
        assert_eq!(terminal_status(&dir)["sessions"][0]["bytes"], 3);
        assert_eq!(
            terminal_reset(&dir, &json!({"session": "main"})).unwrap()["reset"],
            true
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "richiede Docker locale con un'immagine sandbox"]
    fn terminal_container_runs_commands_and_keeps_session_files() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-sbx-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = terminal_run(&dir, &json!({"command": "echo ciao > f.txt && id -u"})).unwrap();
        assert_eq!(first["succeeded"], true, "{first}");
        let second = terminal_run(
            &dir,
            &json!({"command": "cat f.txt; wget -T1 http://1.1.1.1 2>&1 | tail -1"}),
        )
        .unwrap();
        assert!(
            second["stdout"].as_str().unwrap().starts_with("ciao"),
            "{second}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_move_between_workspace_and_session_without_following_links() {
        let base =
            std::env::temp_dir().join(format!("jarvis-sbx-{}", uuid::Uuid::new_v4().simple()));
        let data = base.join("data");
        let work = base.join("workspace");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(work.join("a.txt"), b"ciao").unwrap();
        let put = import_file(&data, &work, &json!({"from": "a.txt", "to": "sub/b.txt"})).unwrap();
        assert_eq!(put["bytes"], 4);
        assert_eq!(
            std::fs::read(data.join("sandbox/main/sub/b.txt")).unwrap(),
            b"ciao"
        );
        let got = export_file(
            &data,
            &work,
            &json!({"from": "sub/b.txt", "to": "out/c.txt"}),
        )
        .unwrap();
        assert_eq!(got["bytes"], 4);
        assert_eq!(std::fs::read(work.join("out/c.txt")).unwrap(), b"ciao");
        // Percorsi fuori ambito e file mancanti.
        for bad in [
            json!({"from": "../x", "to": "y"}),
            json!({"from": "nope.txt"}),
        ] {
            assert!(import_file(&data, &work, &bad).is_err());
        }
        assert!(export_file(&data, &work, &json!({"from": "../x", "to": "y"})).is_err());
        assert!(export_file(
            &data,
            &work,
            &json!({"from": "sub/b.txt", "to": "../fuori"})
        )
        .is_err());
        // Un collegamento creato dal container non deve far leggere file del computer.
        #[cfg(unix)]
        {
            std::fs::write(base.join("segreto"), b"x").unwrap();
            std::os::unix::fs::symlink(base.join("segreto"), data.join("sandbox/main/link"))
                .unwrap();
            std::os::unix::fs::symlink(&base, data.join("sandbox/main/dir")).unwrap();
            for from in ["link", "dir/segreto"] {
                let err = export_file(&data, &work, &json!({"from": from, "to": "leak.txt"}))
                    .unwrap_err();
                assert_eq!(err.code, "sandbox.invalid_path");
            }
            assert!(!work.join("leak.txt").exists());
            let err = import_file(&data, &work, &json!({"from": "a.txt", "to": "dir/x.txt"}))
                .unwrap_err();
            assert_eq!(err.code, "sandbox.invalid_path");
            assert!(!base.join("x.txt").exists());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn container_ids_and_build_failures_are_interpreted() {
        assert_eq!(
            parse_container_ids("abc123def456\n\nnot-an-id\n0123456789ab\n"),
            vec!["abc123def456", "0123456789ab"]
        );
        assert!(
            build_failure("dial tcp: lookup registry-1.docker.io: no such host")
                .contains("Internet")
        );
        assert!(build_failure("write: no space left on device").contains("spazio"));
        assert!(build_failure("pull access denied").contains("accesso negato"));
        assert!(build_failure("boom\nerrore finale\n").contains("errore finale"));
        let args = terminal_args("n", "img", Path::new("/d/main"), "1000:1000", "ls").join(" ");
        assert!(args.contains("--label jarvis.sandbox=1") && args.contains("--ipc none"));
        assert!(DOCKERFILE.contains("FROM python"));
    }
}
