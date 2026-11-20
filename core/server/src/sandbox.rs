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
    isolated_python(source.as_bytes().to_vec(), &["-"])
}

fn docker() -> Command {
    let mut cmd = Command::new("docker");
    cmd.args(["--context", "default"]);
    crate::hide_console(&mut cmd);
    cmd
}

fn limited_output(mut stream: impl Read) -> (String, bool) {
    let mut kept = Vec::new();
    let _ = stream
        .by_ref()
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut kept);
    let truncated = kept.len() as u64 > OUTPUT_LIMIT;
    kept.truncate(OUTPUT_LIMIT as usize);
    let _ = std::io::copy(&mut stream, &mut std::io::sink());
    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}

fn isolated_python(source: Vec<u8>, args: &[&str]) -> Result<Value, ToolError> {
    let name = format!("jarvis-python-{}", uuid::Uuid::new_v4().simple());
    let mut child = docker()
        .args([
            "run",
            "--rm",
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
    let _ = write.join();
    let (stdout, out_cut) = out.join().unwrap_or_default();
    let (stderr, err_cut) = err.join().unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_output_bounded_while_draining() {
        let input = vec![b'x'; (OUTPUT_LIMIT * 2) as usize];
        let (stdout, truncated) = limited_output(input.as_slice());
        assert!(truncated);
        assert_eq!(stdout.len() as u64, OUTPUT_LIMIT);
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
}
