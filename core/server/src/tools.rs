//! Esecutori degli strumenti. Ogni percorso è confinato nella workspace (SECURITY_AND_PRIVACY.md).

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use crate::store::Store;

const MAX_READ: u64 = 64 * 1024;
const MAX_LIST: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Controllo lessicale: percorso relativo, senza `..`, radice o prefissi di unità.
pub fn lexically_inside(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.contains('\0')
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Risolve un percorso relativo nella workspace, rifiutando uscite anche tramite link simbolici.
pub fn resolve(workspace: &Path, rel: &str, must_exist: bool) -> Result<PathBuf, ToolError> {
    if !lexically_inside(rel) {
        return Err(ToolError::new(
            "fs.outside_workspace",
            format!("percorso non ammesso: {rel}"),
        ));
    }
    let root = workspace
        .canonicalize()
        .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?;
    let joined = root.join(rel);
    let check = |p: &Path| -> Result<(), ToolError> {
        if p.starts_with(&root) {
            Ok(())
        } else {
            Err(ToolError::new(
                "fs.outside_workspace",
                format!("percorso fuori dalla workspace: {rel}"),
            ))
        }
    };
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?;
        check(&canonical)?;
        return Ok(canonical);
    }
    if must_exist {
        return Err(ToolError::new(
            "fs.not_found",
            format!("non trovato: {rel}"),
        ));
    }
    // Il file non esiste: la cartella più vicina esistente deve stare nella workspace.
    let mut ancestor = joined.parent();
    while let Some(dir) = ancestor {
        if dir.exists() {
            check(
                &dir.canonicalize()
                    .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?,
            )?;
            break;
        }
        ancestor = dir.parent();
    }
    Ok(joined)
}

pub struct Context<'a> {
    pub workspace: &'a Path,
    pub store: &'a Store,
    pub web: &'a crate::web::WebConfig,
}

pub fn execute(name: &str, args: &Value, ctx: &Context) -> Result<Value, ToolError> {
    let s = |k: &str| args[k].as_str().unwrap_or_default();
    match name {
        "time.now" => {
            let now = chrono::Local::now();
            Ok(json!({
                "utc": now.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "local": now.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
            }))
        }
        "process.list" => process_list(args),
        "web.browse" => crate::browser::browse(ctx.web, args),
        "web.form" => crate::browser::submit_form(ctx.web, args),
        "system.info" => Ok(json!({
            "sistema operativo": std::env::consts::OS,
            "famiglia": std::env::consts::FAMILY,
            "architettura": std::env::consts::ARCH,
            "cpu logiche": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            "versione JARVIS": env!("CARGO_PKG_VERSION"),
        })),
        "fs.list" => {
            let rel = if s("path").is_empty() { "." } else { s("path") };
            let dir = resolve(ctx.workspace, rel, true)?;
            if !dir.is_dir() {
                return Err(ToolError::new(
                    "fs.not_a_directory",
                    format!("non è una cartella: {rel}"),
                ));
            }
            let mut entries: Vec<Value> = fs::read_dir(&dir)
                .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?
                .filter_map(Result::ok)
                .map(|e| {
                    let meta = e.metadata().ok();
                    json!({
                        "name": e.file_name().to_string_lossy(),
                        "kind": if meta.as_ref().is_some_and(|m| m.is_dir()) { "dir" } else { "file" },
                        "size": meta.map(|m| m.len()).unwrap_or(0),
                    })
                })
                .collect();
            entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            let truncated = entries.len() > MAX_LIST;
            entries.truncate(MAX_LIST);
            Ok(json!({"path": rel, "entries": entries, "truncated": truncated}))
        }
        "fs.read" => {
            let file = resolve(ctx.workspace, s("path"), true)?;
            let meta =
                fs::metadata(&file).map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?;
            if !meta.is_file() {
                return Err(ToolError::new("fs.not_a_file", "non è un file"));
            }
            if meta.len() > MAX_READ {
                return Err(ToolError::new(
                    "fs.too_large",
                    format!("file oltre {MAX_READ} byte"),
                ));
            }
            let content = fs::read_to_string(&file)
                .map_err(|_| ToolError::new("fs.not_text", "il file non è testo UTF-8"))?;
            Ok(json!({"path": s("path"), "content": content, "bytes": meta.len()}))
        }
        "fs.write" => {
            let file = resolve(ctx.workspace, s("path"), false)?;
            if file.is_dir() {
                return Err(ToolError::new(
                    "fs.is_directory",
                    "il percorso è una cartella",
                ));
            }
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?;
            }
            let content = s("content");
            fs::write(&file, content)
                .map_err(|e| ToolError::new("fs.unavailable", e.to_string()))?;
            Ok(json!({"path": s("path"), "bytes_written": content.len()}))
        }
        "memory.remember" => {
            // Stessa informazione già ricordata: nessun duplicato, si restituisce l'elemento esistente.
            if let Some((id, kind)) = ctx
                .store
                .memory_find_same(s("text"))
                .map_err(|e| ToolError::new("memory.unavailable", e.to_string()))?
            {
                return Ok(
                    json!({"id": id, "text": s("text"), "kind": kind, "already_known": true}),
                );
            }
            let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
            let kind = args["kind"]
                .as_str()
                .unwrap_or_else(|| crate::memory::infer_kind(s("text")));
            ctx.store
                .memory_insert(
                    &crate::memory::NewItem {
                        id: &id,
                        text: s("text"),
                        kind,
                        source: json!({"kind": "agent"}),
                        expires_at: None,
                    },
                    &crate::now(),
                )
                .map_err(|e| ToolError::new("memory.unavailable", e.to_string()))?;
            Ok(json!({"id": id, "text": s("text"), "kind": kind}))
        }
        "memory.list" => {
            let items = ctx
                .store
                .memory_list_kind(args["kind"].as_str())
                .map_err(|e| ToolError::new("memory.unavailable", e.to_string()))?;
            Ok(json!({"items": items}))
        }
        "memory.search" => {
            let limit = args["limit"].as_u64().unwrap_or(5) as usize;
            let items = ctx
                .store
                .memory_search(s("query"), args["kind"].as_str(), limit)
                .map_err(|e| ToolError::new("memory.unavailable", e.to_string()))?;
            Ok(json!({"query": s("query"), "items": items}))
        }
        "memory.forget" => {
            let deleted = ctx
                .store
                .memory_delete(s("id"))
                .map_err(|e| ToolError::new("memory.unavailable", e.to_string()))?;
            if deleted {
                Ok(json!({"id": s("id")}))
            } else {
                Err(ToolError::new(
                    "memory.not_found",
                    format!("nessun elemento con id {}", s("id")),
                ))
            }
        }
        "web.search" => {
            if !ctx.web.enabled {
                return Err(ToolError::new("web.disabled", "accesso al Web disattivato"));
            }
            crate::web::search(
                ctx.web,
                s("query"),
                args["limit"].as_u64().unwrap_or(5) as usize,
            )
        }
        "web.fetch" => {
            if !ctx.web.enabled {
                return Err(ToolError::new("web.disabled", "accesso al Web disattivato"));
            }
            crate::web::fetch(ctx.web, s("url"))
        }
        other => Err(ToolError::new(
            "tool.unknown",
            format!("strumento sconosciuto: {other}"),
        )),
    }
}

// ---------------------------------------------------------------------- processi e terminale (RF-05, ADR-0038)

/// Caratteri conservati di stdout e stderr di un comando.
const SHELL_OUTPUT: usize = 8000;
/// Durata massima di un comando del terminale.
const SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Processi in esecuzione (sola lettura): `ps` su macOS e Linux, `tasklist` su Windows (senza CPU).
pub fn process_list(args: &Value) -> Result<Value, ToolError> {
    let by_memory = args["sort"] == "memory";
    let limit = args["limit"].as_u64().unwrap_or(15).clamp(1, 50) as usize;
    let unavailable = |e: std::io::Error| ToolError::new("process.unavailable", e.to_string());
    let mut items: Vec<Value> = if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/fo", "csv", "/nh"])
            .output()
            .map_err(unavailable)?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let f: Vec<String> = l.split("\",\"").map(|x| x.trim_matches('"').to_string()).collect();
                let kb: f64 = f.get(4)?.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok()?;
                Some(json!({"pid": f.get(1)?.parse::<u64>().ok()?, "name": f[0], "memory_mb": (kb / 1024.0 * 10.0).round() / 10.0}))
            })
            .collect()
    } else {
        let out = std::process::Command::new("ps")
            .args(["-Ao", "pid=,pcpu=,pmem=,rss=,comm="])
            .output()
            .map_err(unavailable)?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let pid: u64 = f.next()?.parse().ok()?;
                let cpu: f64 = f.next()?.replace(',', ".").parse().ok()?;
                let mem: f64 = f.next()?.replace(',', ".").parse().ok()?;
                let rss: f64 = f.next()?.parse().ok()?;
                let name = f.collect::<Vec<_>>().join(" ");
                let name = name.rsplit('/').next().unwrap_or_default().to_string();
                Some(
                    json!({"pid": pid, "name": name, "cpu_percent": cpu, "memory_percent": mem,
                            "memory_mb": (rss / 1024.0 * 10.0).round() / 10.0}),
                )
            })
            .collect()
    };
    let key = |v: &Value| {
        if by_memory || v.get("cpu_percent").is_none() {
            v["memory_mb"].as_f64().unwrap_or(0.0)
        } else {
            v["cpu_percent"].as_f64().unwrap_or(0.0)
        }
    };
    items.sort_by(|a, b| key(b).total_cmp(&key(a)));
    let total = items.len();
    items.truncate(limit);
    Ok(
        json!({"processes": items, "total": total, "sorted_by": if by_memory { "memory" } else { "cpu" }}),
    )
}

/// Comandi distruttivi noti, negati dalla policy anche se il proprietario li confermerebbe.
pub fn command_blocked(command: &str) -> bool {
    let c = command
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    const BLOCKED: [&str; 14] = [
        "rm -rf /",
        "rm -fr /",
        "rm -rf ~",
        "rm -rf *",
        "mkfs",
        "dd if=",
        ":(){",
        "shutdown",
        "reboot",
        "format c:",
        "diskpart",
        "del /s",
        "rd /s",
        "> /dev/sd",
    ];
    BLOCKED.iter().any(|b| c.contains(b)) || c.contains("sudo ")
}

/// Esegue un comando confermato nella workspace con la shell del sistema. Le variabili `JARVIS_*` (token,
/// percorsi interni) non passano al comando; output limitato e interruzione dopo 60 secondi.
pub fn shell_run(workspace: &Path, command: &str) -> Result<Value, ToolError> {
    use std::io::Read;
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("JARVIS_") {
            cmd.env_remove(k);
        }
    }
    let started = std::time::Instant::now();
    let mut child = cmd
        .current_dir(workspace)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ToolError::new("shell.unavailable", e.to_string()))?;
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            buf
        })
    };
    let out = drain(Box::new(child.stdout.take().expect("stdout")));
    let err = drain(Box::new(child.stderr.take().expect("stderr")));
    let status = loop {
        if let Some(s) = child
            .try_wait()
            .map_err(|e| ToolError::new("shell.unavailable", e.to_string()))?
        {
            break Some(s);
        }
        if started.elapsed() >= SHELL_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let text = |b: Vec<u8>| -> (String, bool) {
        let s = String::from_utf8_lossy(&b);
        (
            s.chars().take(SHELL_OUTPUT).collect(),
            s.chars().count() > SHELL_OUTPUT,
        )
    };
    let (stdout, cut_out) = text(out.join().unwrap_or_default());
    let (stderr, cut_err) = text(err.join().unwrap_or_default());
    let Some(status) = status else {
        return Err(ToolError::new(
            "shell.timeout",
            format!(
                "il comando non è terminato entro {} secondi ed è stato interrotto",
                SHELL_TIMEOUT.as_secs()
            ),
        ));
    };
    Ok(json!({
        "command": command, "exit_code": status.code(), "succeeded": status.success(),
        "stdout": stdout, "stderr": stderr, "truncated": cut_out || cut_err,
        "duration_ms": started.elapsed().as_millis() as u64, "cwd": "workspace",
    }))
}
