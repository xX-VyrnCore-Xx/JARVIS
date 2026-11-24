use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

const IMAGE: &str = "python:3.13-alpine";
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
    Err(error(
        "sandbox.unavailable",
        "Docker non disponibile o immagine assente: avvia Docker e costruisci l'immagine con infra/sandbox/build.sh",
    ))
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

/// Argomenti di `docker run` del terminale sandbox. Isolamento: nessuna rete, radice di sola lettura,
/// nessuna capability, niente mount del computer oltre la cartella della sessione.
pub(crate) fn terminal_args(
    name: &str,
    image: &str,
    dir: &Path,
    user: &str,
    command: &str,
) -> Vec<String> {
    let mount = format!("{}:/work:rw", dir.display());
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--pull",
        "never",
        "--name",
        name,
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,size=64m",
        "--user",
        user,
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "128",
        "--memory",
        "512m",
        "--memory-swap",
        "512m",
        "--cpus",
        "1",
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
    let timeout = args["timeout_s"].as_u64().unwrap_or(20).clamp(1, 60);
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
        .args(terminal_args(&name, &image, &dir, &user, command))
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
        "stderr": stderr, "truncated": out_cut || err_cut, "session": session,
        "image": image, "duration_ms": started.elapsed().as_millis() as u64, "sandbox": "docker",
    }))
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
    let image = pick_image().ok();
    json!({
        "enabled": enabled(data_dir), "available": image.is_some(), "image": image,
        "limits": {"timeout_s_max": 60, "memory_mb": 512, "cpus": 1, "pids": 128,
                   "output_bytes": TERMINAL_OUTPUT, "session_mb": SESSION_SOFT_LIMIT / 1024 / 1024,
                   "network": "none"},
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
}
