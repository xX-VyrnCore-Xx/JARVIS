//! Strumenti di sviluppo e dati (ADR-0155): Git, analisi dei log, dipendenze e tabelle CSV.
//!
//! Git gira senza shell, con argomenti fissi e le funzioni pericolose della configurazione del repository
//! disattivate (hook, monitor del file system, programmi di confronto esterni). Nessuno strumento qui legge fuori
//! dalla workspace né usa la rete.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_LIMIT: usize = 24 * 1024;
const FILE_LIMIT: u64 = 2 * 1024 * 1024;

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

// ------------------------------------------------------------------ Git

/// Esegue `git` nella workspace con argomenti fissi. Hook, fsmonitor e programmi di confronto esterni sono spenti.
fn git(workspace: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Result<String, ToolError> {
    let root = workspace
        .canonicalize()
        .map_err(|e| err("git.unavailable", e.to_string()))?;
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(&root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            "-c",
            "color.ui=false",
            "-c",
            "protocol.ext.allow=never",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        command.env(key, value);
    }
    crate::hide_console(&mut command);
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            err("git.unavailable", "Git non è installato su questo computer")
        } else {
            err("git.unavailable", e.to_string())
        }
    })?;
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = (&mut stdout)
            .take(OUTPUT_LIMIT as u64 + 1)
            .read_to_end(&mut buffer);
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        buffer
    });
    let error = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = (&mut stderr).take(4096).read_to_end(&mut buffer);
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        buffer
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < GIT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err("git.timeout", "Git non ha risposto entro 10 secondi"));
            }
        }
    };
    let out = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    let error = String::from_utf8_lossy(&error.join().unwrap_or_default()).into_owned();
    if !status.success() {
        let text = error.trim();
        return Err(if text.contains("not a git repository") {
            err("git.not_repository", "La workspace non è un repository Git")
        } else {
            err("git.failed", text.chars().take(300).collect::<String>())
        });
    }
    Ok(out)
}

fn clip(text: String) -> (String, bool) {
    if text.len() > OUTPUT_LIMIT {
        let mut end = OUTPUT_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        (text[..end].to_string(), true)
    } else {
        (text, false)
    }
}

/// Percorso opzionale dentro la workspace, restituito relativo.
fn inside(workspace: &Path, args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match args[key].as_str().filter(|p| !p.is_empty()) {
        None => Ok(None),
        Some(path) => {
            resolve(workspace, path, false)?;
            Ok(Some(path.to_string()))
        }
    }
}

/// Stato: ramo, avanti/indietro rispetto al remoto e file modificati.
pub fn status(workspace: &Path) -> Result<Value, ToolError> {
    let raw = git(
        workspace,
        &["status", "--porcelain=v1", "-b", "--no-renames"],
        &[],
    )?;
    let mut branch = Value::Null;
    let mut files = Vec::new();
    for line in raw.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            branch = json!(head);
        } else if line.len() > 3 {
            files.push(json!({"status": line[..2].trim(), "path": line[3..]}));
        }
    }
    Ok(
        json!({"branch": branch, "clean": files.is_empty(), "changed": files.len(), "files": files.into_iter().take(200).collect::<Vec<_>>()}),
    )
}

/// Ultimi commit (fino a 50), eventualmente di un solo percorso.
pub fn log(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"]
        .as_u64()
        .unwrap_or(10)
        .clamp(1, 50)
        .to_string();
    let path = inside(workspace, args, "path")?;
    let count = format!("-n{limit}");
    let mut command = vec![
        "log",
        &count,
        "--no-color",
        "--date=iso-strict",
        "--pretty=format:%H%x1f%an%x1f%ad%x1f%s",
    ];
    if let Some(path) = &path {
        command.push("--");
        command.push(path);
    }
    let raw = git(workspace, &command, &[])?;
    let commits: Vec<Value> = raw
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            Some(json!({
                "hash": parts.next()?, "short": parts.clone().next().map(|_| ()).and(None::<&str>),
                "author": parts.next()?, "date": parts.next()?, "subject": parts.next()?,
            }))
        })
        .map(|mut c| {
            let short: String = c["hash"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(8)
                .collect();
            c["short"] = json!(short);
            c
        })
        .collect();
    Ok(json!({"commits": commits}))
}

/// Differenze (sola lettura): riepilogo e patch ridotta.
pub fn diff(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let path = inside(workspace, args, "path")?;
    let staged = args["staged"].as_bool().unwrap_or(false);
    let mut stat = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--stat",
    ];
    let mut patch = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--unified=3",
    ];
    if staged {
        stat.push("--cached");
        patch.push("--cached");
    }
    if let Some(path) = &path {
        stat.push("--");
        stat.push(path);
        patch.push("--");
        patch.push(path);
    }
    let summary = git(workspace, &stat, &[])?;
    let (text, truncated) = clip(git(workspace, &patch, &[])?);
    Ok(json!({"summary": summary.trim(), "patch": text, "truncated": truncated, "staged": staged}))
}

/// Rami locali con quello corrente.
pub fn branches(workspace: &Path) -> Result<Value, ToolError> {
    let raw = git(
        workspace,
        &[
            "branch",
            "--list",
            "--no-color",
            "--format=%(refname:short)%1f%(HEAD)",
        ],
        &[],
    )?;
    let items: Vec<Value> = raw
        .lines()
        .filter_map(|line| {
            let (name, head) = line.split_once('\u{1f}')?;
            Some(json!({"name": name, "current": head.trim() == "*"}))
        })
        .collect();
    let current = items
        .iter()
        .find(|b| b["current"] == true)
        .map(|b| b["name"].clone());
    Ok(json!({"branches": items, "current": current}))
}

/// Registra le modifiche in un commit (richiede la conferma del proprietario). Gli hook sono disattivati.
pub fn commit(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let message = args["message"].as_str().unwrap_or_default().trim();
    if message.is_empty() || message.chars().count() > 200 || message.contains('\0') {
        return Err(err(
            "git.invalid",
            "Il messaggio deve avere da 1 a 200 caratteri",
        ));
    }
    let paths: Vec<String> = args["paths"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for path in &paths {
        resolve(workspace, path, false)?;
    }
    let mut add = vec!["add", "--"];
    if paths.is_empty() {
        add = vec!["add", "-A"];
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    add.extend(refs);
    git(workspace, &add, &[])?;
    let identity = [
        ("GIT_AUTHOR_NAME", "JARVIS"),
        ("GIT_AUTHOR_EMAIL", "jarvis@localhost"),
        ("GIT_COMMITTER_NAME", "JARVIS"),
        ("GIT_COMMITTER_EMAIL", "jarvis@localhost"),
    ];
    // L'identità del proprietario ha la precedenza: i valori di ripiego servono solo se manca la configurazione.
    let configured =
        git(workspace, &["config", "user.email"], &[]).is_ok_and(|v| !v.trim().is_empty());
    let env: &[(&str, &str)] = if configured { &[] } else { &identity };
    git(workspace, &["commit", "--no-verify", "-m", message], env)?;
    let head = git(workspace, &["rev-parse", "--short=8", "HEAD"], &[])?;
    Ok(json!({"committed": true, "commit": head.trim(), "message": message}))
}

// ------------------------------------------------------------------ Log

/// Sostituisce numeri, esadecimali e identificativi con `#`: messaggi che differiscono solo per i dati si raggruppano.
fn normalise(line: &str) -> String {
    let mut out = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if !token.is_empty() {
            let digits = token.chars().filter(char::is_ascii_digit).count();
            let hex = token.len() >= 8 && token.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
            if digits > 0
                && (hex || digits * 2 >= token.len() || token.chars().all(|c| c.is_ascii_digit()))
            {
                out.push('#');
            } else {
                out.push_str(token);
            }
            token.clear();
        }
    };
    for c in line.chars() {
        if c.is_alphanumeric() || c == '-' || c == '.' {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out.trim().to_string()
}

fn level_of(line: &str) -> Option<&'static str> {
    let upper = line.to_ascii_uppercase();
    let has = |word: &str| {
        upper.match_indices(word).any(|(i, _)| {
            let before = upper[..i].chars().next_back();
            let after = upper[i + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    };
    [
        "FATAL", "CRITICAL", "ERROR", "ERR", "WARNING", "WARN", "INFO", "DEBUG", "TRACE",
    ]
    .into_iter()
    .find(|w| has(w))
    .map(|w| match w {
        "FATAL" | "CRITICAL" => "fatal",
        "ERROR" | "ERR" => "error",
        "WARNING" | "WARN" => "warn",
        "INFO" => "info",
        _ => "debug",
    })
}

/// Riepilogo di un log: livelli, messaggi ricorrenti, intervallo di tempo e ultimi errori.
pub fn analyze_text(text: &str) -> Value {
    let mut levels: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    let mut clusters: std::collections::HashMap<String, (u64, String, &'static str)> =
        std::collections::HashMap::new();
    let (mut first, mut last) = (None::<String>, None::<String>);
    let mut recent_errors: Vec<String> = Vec::new();
    let mut lines = 0u64;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        lines += 1;
        let stamp: String = line.chars().take(19).collect();
        if stamp.len() == 19 && stamp.as_bytes()[4] == b'-' && stamp.as_bytes()[7] == b'-' {
            first.get_or_insert_with(|| stamp.clone());
            last = Some(stamp);
        }
        let Some(level) = level_of(line) else {
            continue;
        };
        *levels.entry(level).or_default() += 1;
        if matches!(level, "error" | "fatal" | "warn") {
            let key = normalise(line);
            let entry = clusters
                .entry(key)
                .or_insert((0, line.chars().take(200).collect(), level));
            entry.0 += 1;
            if level != "warn" {
                recent_errors.push(line.chars().take(200).collect());
                if recent_errors.len() > 5 {
                    recent_errors.remove(0);
                }
            }
        }
    }
    let mut top: Vec<_> = clusters.into_values().collect();
    top.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    json!({
        "lines": lines,
        "levels": levels,
        "from": first, "to": last,
        "top_problems": top.into_iter().take(5).map(|(count, example, level)| json!({"count": count, "level": level, "example": example})).collect::<Vec<_>>(),
        "recent_errors": recent_errors,
    })
}

/// Analizza un file di log della workspace.
pub fn analyze(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let file = resolve(workspace, args["path"].as_str().unwrap_or_default(), true)?;
    let text = read_limited(&file)?;
    Ok(analyze_text(&text))
}

fn read_limited(file: &Path) -> Result<String, ToolError> {
    if !file.is_file() {
        return Err(err("fs.not_file", "Il percorso non è un file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .and_then(|f| f.take(FILE_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|e| err("fs.unavailable", e.to_string()))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(err("fs.too_large", "Il file supera 2 MiB"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ------------------------------------------------------------------ Dipendenze

fn dep(name: &str, spec: &str, kind: &str) -> Value {
    let spec = spec.trim();
    let source = if spec.contains("git") || spec.contains("http") {
        "git"
    } else if spec.contains("path") || spec.starts_with("file:") || spec.starts_with('.') {
        "path"
    } else {
        "registry"
    };
    let pinned = source == "registry"
        && !spec.is_empty()
        && spec != "*"
        && !spec.eq_ignore_ascii_case("latest")
        && !spec.starts_with('>')
        && !spec.contains("||");
    json!({"name": name, "spec": spec, "kind": kind, "source": source, "pinned": pinned})
}

fn cargo_deps(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut kind = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            kind = match section {
                "dependencies" => "runtime",
                "dev-dependencies" => "dev",
                "build-dependencies" => "build",
                s if s.ends_with(".dependencies") => "runtime",
                _ => "",
            }
            .into();
            continue;
        }
        if kind.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let version = value
            .split("version")
            .nth(1)
            .and_then(|r| r.split('"').nth(1))
            .map(str::to_string);
        let spec = match (value.starts_with('"'), version) {
            (true, _) => value.trim_matches('"').to_string(),
            (false, Some(v)) => v,
            (false, None) => value.to_string(),
        };
        out.push(dep(name.trim(), &spec, &kind));
    }
    out
}

fn npm_deps(text: &str) -> Vec<Value> {
    let doc: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let mut out = Vec::new();
    for (section, kind) in [
        ("dependencies", "runtime"),
        ("devDependencies", "dev"),
        ("peerDependencies", "peer"),
    ] {
        for (name, spec) in doc[section].as_object().into_iter().flatten() {
            let spec = spec.as_str().unwrap_or_default();
            let mut item = dep(name, spec, kind);
            // «^1.2.3» e «~1.2.3» sono intervalli compatibili, non versioni esatte.
            if spec.starts_with('^') || spec.starts_with('~') {
                item["pinned"] = json!(false);
            }
            out.push(item);
        }
    }
    out
}

fn pip_deps(text: &str) -> Vec<Value> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('-'))
        .map(|l| {
            let end = l
                .find(['=', '<', '>', '~', '!', ';', '[', ' '])
                .unwrap_or(l.len());
            let (name, spec) = l.split_at(end);
            let mut item = dep(name.trim(), spec.trim(), "runtime");
            item["pinned"] = json!(spec.trim_start().starts_with("=="));
            item
        })
        .collect()
}

fn go_deps(text: &str) -> Vec<Value> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("require (") {
            inside = true;
        } else if line == ")" {
            inside = false;
        } else if inside || line.starts_with("require ") {
            let body = line.trim_start_matches("require").trim();
            let mut parts = body.split_whitespace();
            if let (Some(name), Some(version)) = (parts.next(), parts.next()) {
                out.push(dep(name, version, "runtime"));
            }
        }
    }
    out
}

/// Dipendenze dichiarate dai manifest presenti in una cartella della workspace (non ricorsivo).
pub fn dependencies(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let dir = resolve(
        workspace,
        args["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or("."),
        true,
    )?;
    if !dir.is_dir() {
        return Err(err("fs.not_directory", "Il percorso non è una cartella"));
    }
    let mut manifests = Vec::new();
    for (file, ecosystem) in [
        ("Cargo.toml", "rust"),
        ("package.json", "node"),
        ("requirements.txt", "python"),
        ("go.mod", "go"),
    ] {
        let path = dir.join(file);
        if !path.is_file() {
            continue;
        }
        let text = read_limited(&path)?;
        let deps = match ecosystem {
            "rust" => cargo_deps(&text),
            "node" => npm_deps(&text),
            "python" => pip_deps(&text),
            _ => go_deps(&text),
        };
        let unpinned = deps
            .iter()
            .filter(|d| d["pinned"] == false && d["source"] == "registry")
            .count();
        manifests.push(
            json!({"file": file, "ecosystem": ecosystem, "count": deps.len(), "unpinned": unpinned,
            "dependencies": deps.into_iter().take(300).collect::<Vec<_>>()}),
        );
    }
    let total: u64 = manifests.iter().filter_map(|m| m["count"].as_u64()).sum();
    let unpinned: u64 = manifests
        .iter()
        .filter_map(|m| m["unpinned"].as_u64())
        .sum();
    Ok(json!({"manifests": manifests, "total": total, "unpinned": unpinned}))
}

// ------------------------------------------------------------------ Tabelle

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let delimiter = {
        let head: String = text.lines().next().unwrap_or_default().to_string();
        [';', '\t', ',']
            .into_iter()
            .max_by_key(|d| head.matches(*d).count())
            .unwrap_or(',')
    };
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, c) if c == delimiter => row.push(std::mem::take(&mut field)),
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, '\r') => {}
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.into_iter()
        .filter(|r| r.iter().any(|f| !f.trim().is_empty()))
        .collect()
}

fn number_of(text: &str) -> Option<f64> {
    let t = text.trim().replace('\u{a0}', "");
    let t = if t.contains(',') && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Riepilogo di una tabella CSV: colonne, tipi, statistiche numeriche, valori più frequenti e anteprima.
pub fn summarize_csv(text: &str) -> Result<Value, ToolError> {
    let rows = parse_csv(text);
    let Some((header, body)) = rows.split_first() else {
        return Err(err("table.empty", "La tabella è vuota"));
    };
    if body.len() > 50_000 {
        return Err(err("table.too_large", "Al massimo 50000 righe"));
    }
    let mut columns = Vec::new();
    for (index, name) in header.iter().enumerate() {
        let cells: Vec<&str> = body
            .iter()
            .map(|r| r.get(index).map_or("", String::as_str))
            .collect();
        let filled: Vec<&str> = cells
            .iter()
            .copied()
            .filter(|c| !c.trim().is_empty())
            .collect();
        let numbers: Vec<f64> = filled.iter().filter_map(|c| number_of(c)).collect();
        let kind = if filled.is_empty() {
            "vuota"
        } else if numbers.len() == filled.len()
            && filled.iter().all(|c| c.trim().parse::<i64>().is_ok())
        {
            "intero"
        } else if numbers.len() == filled.len() {
            "numero"
        } else if filled.iter().all(|c| {
            c.len() >= 8 && c.as_bytes().get(4) == Some(&b'-') && c.as_bytes().get(7) == Some(&b'-')
        }) {
            "data"
        } else {
            "testo"
        };
        let mut info = json!({"name": name, "type": kind, "filled": filled.len(), "empty": cells.len() - filled.len()});
        if matches!(kind, "intero" | "numero") {
            let mut sorted = numbers.clone();
            sorted.sort_by(f64::total_cmp);
            let sum: f64 = sorted.iter().sum();
            let median = if sorted.len() % 2 == 1 {
                sorted[sorted.len() / 2]
            } else {
                (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
            };
            let round = |v: f64| (v * 1e4).round() / 1e4;
            info["min"] = json!(sorted[0]);
            info["max"] = json!(sorted[sorted.len() - 1]);
            info["sum"] = json!(round(sum));
            info["mean"] = json!(round(sum / sorted.len() as f64));
            info["median"] = json!(round(median));
        } else if kind != "vuota" {
            let mut counts: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
            for cell in &filled {
                *counts.entry(cell.trim()).or_default() += 1;
            }
            info["unique"] = json!(counts.len());
            let mut top: Vec<_> = counts.into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            info["top_values"] = json!(top
                .into_iter()
                .take(3)
                .map(|(v, c)| json!({"value": v, "count": c}))
                .collect::<Vec<_>>());
        }
        columns.push(info);
    }
    Ok(json!({
        "rows": body.len(), "columns": columns,
        "preview": body.iter().take(5).collect::<Vec<_>>(),
    }))
}

/// Riepilogo di un CSV della workspace (`path`) o di un testo (`text`).
pub fn summarize(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    match (
        args["path"].as_str().filter(|p| !p.is_empty()),
        args["text"].as_str(),
    ) {
        (Some(path), _) => summarize_csv(&read_limited(&resolve(workspace, path, true)?)?),
        (None, Some(text)) => summarize_csv(text),
        _ => Err(err(
            "table.invalid",
            "Indica «path» di un file CSV o «text»",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-dev-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.org"])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "{args:?}");
    }

    #[test]
    fn git_reports_status_history_diff_and_commits_safely() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = temp("git");
        assert_eq!(status(&dir).unwrap_err().code, "git.not_repository");
        run(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "uno\n").unwrap();
        let first = status(&dir).unwrap();
        assert_eq!(first["clean"], false);
        assert_eq!(first["files"][0]["path"], "a.txt");
        let made = commit(&dir, &json!({"message": "Prima versione"})).unwrap();
        assert_eq!(made["committed"], true);
        assert_eq!(status(&dir).unwrap()["clean"], true);
        std::fs::write(dir.join("a.txt"), "uno\ndue\n").unwrap();
        let changes = diff(&dir, &json!({})).unwrap();
        assert!(changes["summary"].as_str().unwrap().contains("a.txt"));
        assert!(changes["patch"].as_str().unwrap().contains("+due"));
        commit(&dir, &json!({"message": "Seconda", "paths": ["a.txt"]})).unwrap();
        let history = log(&dir, &json!({"limit": 5})).unwrap();
        assert_eq!(history["commits"].as_array().unwrap().len(), 2);
        assert_eq!(history["commits"][0]["subject"], "Seconda");
        assert_eq!(history["commits"][0]["short"].as_str().unwrap().len(), 8);
        assert_eq!(branches(&dir).unwrap()["current"], "main");
        // Percorsi fuori dalla workspace e messaggi non validi sono rifiutati; gli hook del repository non girano.
        assert!(diff(&dir, &json!({"path": "../fuori"})).is_err());
        assert!(commit(&dir, &json!({"message": ""})).is_err());
        let hook = dir.join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        assert!(commit(&dir, &json!({"message": "Con hook"})).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn logs_are_summarised_by_level_and_recurring_problem() {
        let log = "2026-11-25T08:00:01 INFO avvio\n\
                   2026-11-25T08:00:02 ERROR connessione al database 10.0.0.5 rifiutata (tentativo 1)\n\
                   2026-11-25T08:00:03 WARN lentezza 350 ms\n\
                   2026-11-25T08:00:04 ERROR connessione al database 10.0.0.9 rifiutata (tentativo 2)\n\
                   2026-11-25T08:00:05 error: disco pieno\n\
                   riga senza livello\n";
        let out = analyze_text(log);
        assert_eq!(out["lines"], 6);
        assert_eq!(out["levels"]["error"], 3);
        assert_eq!(out["levels"]["warn"], 1);
        assert_eq!(out["levels"]["info"], 1);
        assert_eq!(out["from"], "2026-11-25T08:00:01");
        assert_eq!(out["to"], "2026-11-25T08:00:05");
        // I due errori di connessione differiscono solo per i numeri: un unico problema ricorrente.
        assert_eq!(out["top_problems"][0]["count"], 2);
        assert!(out["top_problems"][0]["example"]
            .as_str()
            .unwrap()
            .contains("connessione"));
        assert_eq!(out["recent_errors"].as_array().unwrap().len(), 3);
        assert_eq!(level_of("informazioni utili"), None);
    }

    #[test]
    fn manifests_are_parsed_and_unpinned_dependencies_flagged() {
        let dir = temp("deps");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\nureq = { version = \"2.9\", features = [\"json\"] }\nlocale = { path = \"../locale\" }\nwild = \"*\"\n\n[dev-dependencies]\ntempfile = \"3\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies": {"react": "^18.2.0", "left-pad": "1.3.0"}, "devDependencies": {"vite": "latest"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("requirements.txt"),
            "requests==2.31.0\nflask>=2\n# commento\nnumpy\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("go.mod"),
            "module x\n\nrequire (\n\tgithub.com/a/b v1.2.3\n)\n",
        )
        .unwrap();
        let out = dependencies(&dir, &json!({})).unwrap();
        assert_eq!(out["manifests"].as_array().unwrap().len(), 4);
        let cargo = &out["manifests"][0];
        assert_eq!(cargo["count"], 5);
        let by = |m: &Value, name: &str| {
            m["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["name"] == name)
                .cloned()
                .unwrap()
        };
        assert_eq!(by(cargo, "ureq")["spec"], "2.9");
        assert_eq!(by(cargo, "locale")["source"], "path");
        assert_eq!(by(cargo, "wild")["pinned"], false);
        assert_eq!(by(cargo, "tempfile")["kind"], "dev");
        let node = &out["manifests"][1];
        assert_eq!(by(node, "react")["pinned"], false);
        assert_eq!(by(node, "left-pad")["pinned"], true);
        let python = &out["manifests"][2];
        assert_eq!(by(python, "requests")["pinned"], true);
        assert_eq!(by(python, "numpy")["pinned"], false);
        assert_eq!(
            out["manifests"][3]["dependencies"][0]["name"],
            "github.com/a/b"
        );
        assert!(out["unpinned"].as_u64().unwrap() >= 5);
        assert!(dependencies(&dir, &json!({"path": "../x"})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tables_are_summarised_with_types_and_statistics() {
        let csv = "nome;citta;fatturato;data\nAda;Roma;1200,5;2026-01-02\nBruno;Milano;800;2026-02-03\nCarla;Roma;;2026-03-04\nDino;\"Reggio, Emilia\";950;2026-03-05\n";
        let out = summarize_csv(csv).unwrap();
        assert_eq!(out["rows"], 4);
        let columns = out["columns"].as_array().unwrap();
        assert_eq!(columns[0]["type"], "testo");
        assert_eq!(columns[1]["top_values"][0]["value"], "Roma");
        assert_eq!(columns[1]["top_values"][0]["count"], 2);
        assert_eq!(columns[2]["type"], "numero");
        assert_eq!(columns[2]["filled"], 3);
        assert_eq!(columns[2]["empty"], 1);
        assert_eq!(columns[2]["max"], 1200.5);
        assert_eq!(columns[2]["median"], 950.0);
        assert_eq!(columns[3]["type"], "data");
        assert!(out["preview"].as_array().unwrap().len() == 4);
        assert_eq!(out["preview"][3][1], "Reggio, Emilia");
        assert_eq!(
            summarize_csv("a,b\n1,2\n3,4\n").unwrap()["columns"][0]["type"],
            "intero"
        );
        assert!(summarize_csv("").is_err());
        let dir = temp("table");
        std::fs::write(dir.join("t.csv"), "x\n1\n2\n").unwrap();
        assert_eq!(
            summarize(&dir, &json!({"path": "t.csv"})).unwrap()["columns"][0]["sum"],
            3.0
        );
        assert!(summarize(&dir, &json!({})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
