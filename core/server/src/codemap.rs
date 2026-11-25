//! Mappa del codice della workspace (ADR-0189): linguaggi, simboli, dipendenze fra moduli e «chi dipende da chi».
//! Solo lettura, nessuna esecuzione, nessun collegamento simbolico, limiti espliciti. L'analisi è lessicale
//! (righe che iniziano con `fn`, `use crate::`, `import`…): non è un compilatore, quindi macro, import dinamici e
//! reimportazioni possono sfuggire. Le dipendenze si risolvono solo verso file che esistono nella workspace.
//! Non c'è indice persistente: ogni richiesta rilegge i file entro i limiti; l'indicizzazione incrementale resta da fare.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

const MAX_FILES: usize = 4000;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const SKIP_DIRS: [&str; 9] = [
    ".git",
    "target",
    "node_modules",
    "build",
    "dist",
    ".build",
    "__pycache__",
    ".venv",
    "venv",
];

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn language(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "py" => "python",
        "swift" => "swift",
        "js" | "cjs" | "mjs" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "kt" | "java" => "jvm",
        "go" => "go",
        _ => return None,
    })
}

fn word_after<'a>(line: &'a str, keywords: &[&str]) -> Option<&'a str> {
    let t = line.trim_start();
    let t = t.strip_prefix("pub ").unwrap_or(t);
    let t = t.strip_prefix("pub(crate) ").unwrap_or(t);
    let t = t.strip_prefix("async ").unwrap_or(t);
    let t = t.strip_prefix("export ").unwrap_or(t);
    for k in keywords {
        if let Some(rest) = t.strip_prefix(k).and_then(|r| r.strip_prefix(' ')) {
            let name: &str = rest
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Simboli e importazioni di un file (lessicale).
fn scan(lang: &str, text: &str) -> (Vec<(String, &'static str)>, BTreeSet<String>) {
    let mut symbols = Vec::new();
    let mut imports = BTreeSet::new();
    for line in text.lines() {
        let t = line.trim_start();
        match lang {
            "rust" => {
                if let Some(n) = word_after(t, &["fn"]) {
                    symbols.push((n.to_string(), "fn"));
                } else if let Some(n) = word_after(t, &["struct"]) {
                    symbols.push((n.to_string(), "struct"));
                } else if let Some(n) = word_after(t, &["enum"]) {
                    symbols.push((n.to_string(), "enum"));
                } else if let Some(n) = word_after(t, &["trait"]) {
                    symbols.push((n.to_string(), "trait"));
                }
                let t2 = t.strip_prefix("pub ").unwrap_or(t);
                if let Some(rest) = t2.strip_prefix("use crate::") {
                    imports.insert(first_ident(rest));
                } else if let Some(rest) = t2.strip_prefix("use super::") {
                    imports.insert(first_ident(rest));
                } else if let Some(rest) = t2.strip_prefix("mod ") {
                    if rest.trim_end().ends_with(';') {
                        imports.insert(first_ident(rest));
                    }
                }
            }
            "python" => {
                if let Some(n) = word_after(t, &["def"]) {
                    symbols.push((n.to_string(), "def"));
                } else if let Some(n) = word_after(t, &["class"]) {
                    symbols.push((n.to_string(), "class"));
                }
                if let Some(rest) = t.strip_prefix("from ") {
                    if let Some((module, _)) = rest.split_once(" import") {
                        imports.insert(
                            module
                                .trim()
                                .trim_start_matches('.')
                                .split('.')
                                .next_back()
                                .unwrap_or("")
                                .to_string(),
                        );
                    }
                } else if let Some(rest) = t.strip_prefix("import ") {
                    for part in rest.split(',') {
                        imports.insert(
                            part.trim()
                                .split(" as ")
                                .next()
                                .unwrap_or("")
                                .split('.')
                                .next_back()
                                .unwrap_or("")
                                .to_string(),
                        );
                    }
                }
            }
            "swift" => {
                for (k, kind) in [
                    ("func", "func"),
                    ("class", "class"),
                    ("struct", "struct"),
                    ("enum", "enum"),
                ] {
                    if let Some(n) = word_after(t, &[k]) {
                        symbols.push((n.to_string(), kind));
                        break;
                    }
                }
            }
            "javascript" | "typescript" => {
                if let Some(n) = word_after(t, &["function", "class"]) {
                    symbols.push((n.to_string(), "symbol"));
                }
                if t.starts_with("import ") || t.contains("require(") {
                    if let Some(start) = t.find(['\'', '"']) {
                        let quote = t.as_bytes()[start] as char;
                        if let Some(end) = t[start + 1..].find(quote) {
                            let spec = &t[start + 1..start + 1 + end];
                            if spec.starts_with('.') {
                                let stem = spec.rsplit('/').next().unwrap_or("");
                                imports.insert(stem.split('.').next().unwrap_or("").to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    imports.retain(|i| !i.is_empty());
    (symbols, imports)
}

fn first_ident(s: &str) -> String {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("")
        .to_string()
}

struct FileInfo {
    path: String,
    lang: &'static str,
    lines: usize,
    symbols: Vec<(String, &'static str)>,
    imports: BTreeSet<String>,
}

/// Argomenti: `path` (cartella, predefinita la workspace), `dependents_of` (modulo: elenca chi lo usa),
/// `symbol` (cerca una definizione per nome), `top` (1–50).
pub fn map(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let rel = args["path"].as_str().unwrap_or(".");
    let top = args["top"].as_u64().unwrap_or(15).clamp(1, 50) as usize;
    let root = workspace
        .canonicalize()
        .map_err(|e| err("fs.unavailable", e.to_string()))?;
    let start = if rel == "." || rel.is_empty() {
        root.clone()
    } else {
        resolve(workspace, rel, true)?
    };
    if !start.is_dir() {
        return Err(err("fs.unavailable", "il percorso non è una cartella"));
    }
    let mut files: Vec<FileInfo> = Vec::new();
    let mut stack = vec![start];
    let (mut truncated, mut skipped_large) = (false, 0usize);
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if meta.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            let Some(lang) = path.extension().and_then(|x| x.to_str()).and_then(language) else {
                continue;
            };
            if meta.len() > MAX_FILE_BYTES {
                skipped_large += 1;
                continue;
            }
            if files.len() >= MAX_FILES {
                truncated = true;
                break 'walk;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (symbols, imports) = scan(lang, &text);
            files.push(FileInfo {
                path: path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                lang,
                lines: text.lines().count(),
                symbols,
                imports,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let stem = |p: &str| -> String {
        p.rsplit('/')
            .next()
            .unwrap_or(p)
            .rsplit_once('.')
            .map_or(p.to_string(), |(s, _)| s.to_string())
    };
    // Modulo = nome del file senza estensione; un'importazione conta solo se corrisponde a un file esistente.
    let known: BTreeSet<String> = files.iter().map(|f| stem(&f.path)).collect();
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for f in &files {
        let me = stem(&f.path);
        for i in &f.imports {
            if i != &me && known.contains(i) {
                edges.entry(me.clone()).or_default().insert(i.clone());
            }
        }
    }
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (from, tos) in &edges {
        for to in tos {
            dependents
                .entry(to.clone())
                .or_default()
                .insert(from.clone());
        }
    }

    let mut langs: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for f in &files {
        let s = langs.entry(f.lang).or_default();
        s.0 += 1;
        s.1 += f.lines;
    }
    let mut hubs: Vec<(&String, usize)> = dependents.iter().map(|(k, v)| (k, v.len())).collect();
    hubs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut biggest: Vec<&FileInfo> = files.iter().collect();
    biggest.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.path.cmp(&b.path)));

    let mut out = json!({
        "path": rel,
        "files": files.len(),
        "languages": langs.iter().map(|(l, (n, lines))| json!({"language": l, "files": n, "lines": lines})).collect::<Vec<_>>(),
        "most_depended_on": hubs.iter().take(top).map(|(m, n)| json!({"module": m, "dependents": n})).collect::<Vec<_>>(),
        "largest_files": biggest.iter().take(top).map(|f| json!({"path": f.path, "lines": f.lines, "symbols": f.symbols.len()})).collect::<Vec<_>>(),
        "truncated": truncated, "skipped_large_files": skipped_large, "read_only": true,
        "note": "Analisi lessicale: macro, importazioni dinamiche e reimportazioni possono sfuggire. Nessun indice persistente.",
    });
    if let Some(target) = args["dependents_of"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let direct: Vec<&String> = dependents.get(target).into_iter().flatten().collect();
        // Impatto transitivo: tutto ciò che raggiunge il modulo risalendo le dipendenze.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = vec![target.to_string()];
        while let Some(m) = queue.pop() {
            for d in dependents.get(&m).into_iter().flatten() {
                if seen.insert(d.clone()) {
                    queue.push(d.clone());
                }
            }
        }
        seen.remove(target);
        out["dependents_of"] = json!({"module": target, "known": known.contains(target),
            "direct": direct, "transitive": seen,
            "depends_on": edges.get(target).map(|s| s.iter().collect::<Vec<_>>()).unwrap_or_default()});
    }
    if let Some(name) = args["symbol"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let hits: Vec<Value> = files
            .iter()
            .flat_map(|f| {
                f.symbols
                    .iter()
                    .filter(|(n, _)| n == name)
                    .map(move |(n, k)| json!({"path": f.path, "name": n, "kind": k}))
            })
            .take(50)
            .collect();
        out["symbol"] = json!({"name": name, "definitions": hits});
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_symbols_and_impact_are_mapped_from_real_files() {
        let ws = std::env::current_dir().unwrap().join(format!(
            "target/jarvis-codemap-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(ws.join("target/debug")).unwrap();
        std::fs::write(
            ws.join("src/store.rs"),
            "pub struct Store;\npub fn open() {}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/memory.rs"),
            "use crate::store::Store;\npub fn remember() {}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/api.rs"),
            "use crate::memory;\nuse crate::store;\nasync fn handler() {}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/lone.py"),
            "import os\nfrom helper import x\ndef run():\n    pass\nclass Tool: pass\n",
        )
        .unwrap();
        std::fs::write(ws.join("src/helper.py"), "def x(): pass\n").unwrap();
        std::fs::write(ws.join("target/debug/ignored.rs"), "fn nascosto() {}\n").unwrap();
        let r = map(
            &ws,
            &json!({"dependents_of": "store", "symbol": "remember"}),
        )
        .unwrap();
        assert_eq!(r["files"], 5, "la cartella target è esclusa: {r}");
        assert_eq!(r["dependents_of"]["direct"], json!(["api", "memory"]));
        assert_eq!(r["dependents_of"]["transitive"], json!(["api", "memory"]));
        assert_eq!(r["most_depended_on"][0]["module"], "store");
        assert_eq!(r["symbol"]["definitions"][0]["path"], "src/memory.rs");
        let py = map(&ws, &json!({"dependents_of": "helper"})).unwrap();
        assert_eq!(py["dependents_of"]["direct"], json!(["lone"]));
        assert_eq!(r["read_only"], true);
        assert_eq!(
            map(&ws, &json!({"path": "../fuori"})).unwrap_err().code,
            "fs.outside_workspace"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn the_core_sources_are_mapped_with_plausible_dependencies() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let r = map(&src, &json!({"dependents_of": "store", "symbol": "map"})).unwrap();
        assert!(r["files"].as_u64().unwrap() > 50, "{r}");
        assert!(!r["dependents_of"]["direct"].as_array().unwrap().is_empty());
        assert!(r["symbol"]["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["path"] == "codemap.rs"));
    }
}
