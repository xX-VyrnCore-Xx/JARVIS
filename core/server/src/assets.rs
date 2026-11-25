//! Gestione delle risorse digitali (ADR-0185): uso dello spazio, file grandi, duplicati e file temporanei
//! nella workspace. Solo lettura: nessun file viene spostato o eliminato; le eliminazioni restano a
//! `fs.delete`, che richiede la conferma del proprietario. Niente collegamenti simbolici, scansione e
//! lettura con limiti espliciti, risultato dichiarato come parziale quando i limiti scattano.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::tools::{resolve, ToolError};

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

const MAX_SCAN: usize = 20_000;
const MAX_HASH_FILE: u64 = 64 * 1024 * 1024;
const MAX_HASH_TOTAL: u64 = 512 * 1024 * 1024;

fn is_temporary(name: &str) -> bool {
    let n = name.to_lowercase();
    n == ".ds_store"
        || n == "thumbs.db"
        || n.ends_with(".tmp")
        || n.ends_with(".temp")
        || n.ends_with(".bak")
        || n.ends_with(".swp")
        || n.ends_with('~')
        || n.ends_with(".crdownload")
        || n.ends_with(".part")
}

fn digest(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

struct Entry {
    rel: String,
    bytes: u64,
}

/// Analizza la workspace (o una sua cartella): `path`, `top` (1–50, predefinito 10) e `min_bytes`
/// (dimensione minima dei file considerati duplicati, predefinita 1).
pub fn usage(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let rel = args["path"].as_str().unwrap_or(".");
    let top = args["top"].as_u64().unwrap_or(10).clamp(1, 50) as usize;
    let min_bytes = args["min_bytes"].as_u64().unwrap_or(1).max(1);
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

    let mut files: Vec<Entry> = Vec::new();
    let mut stack = vec![start];
    let mut truncated = false;
    'scan: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(e.path());
            } else if meta.is_file() {
                if files.len() >= MAX_SCAN {
                    truncated = true;
                    break 'scan;
                }
                let path = e.path();
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(Entry {
                    rel,
                    bytes: meta.len(),
                });
            }
        }
    }

    let total: u64 = files.iter().map(|f| f.bytes).sum();
    let mut by_ext: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut by_top: BTreeMap<String, u64> = BTreeMap::new();
    let (mut temp_files, mut temp_bytes) = (Vec::new(), 0u64);
    for f in &files {
        let name = f.rel.rsplit('/').next().unwrap_or(&f.rel);
        let ext = name
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map_or("(nessuna)".to_string(), |(_, e)| e.to_lowercase());
        let slot = by_ext.entry(ext).or_default();
        slot.0 += 1;
        slot.1 += f.bytes;
        let first = f.rel.split_once('/').map_or("(radice)", |(a, _)| a);
        *by_top.entry(first.to_string()).or_default() += f.bytes;
        if is_temporary(name) {
            temp_bytes += f.bytes;
            if temp_files.len() < top {
                temp_files.push(json!({"path": f.rel, "bytes": f.bytes}));
            }
        }
    }
    let ranked = |mut v: Vec<(String, u64)>| -> Vec<Value> {
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter()
            .take(top)
            .map(|(name, bytes)| json!({"name": name, "bytes": bytes}))
            .collect()
    };
    let mut ext_rows: Vec<(String, u64, u64)> =
        by_ext.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    ext_rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let extensions: Vec<Value> = ext_rows
        .into_iter()
        .take(top)
        .map(|(ext, files, bytes)| json!({"extension": ext, "files": files, "bytes": bytes}))
        .collect();

    let mut largest: Vec<&Entry> = files.iter().collect();
    largest.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.rel.cmp(&b.rel)));
    let largest: Vec<Value> = largest
        .into_iter()
        .take(top)
        .map(|f| json!({"path": f.rel, "bytes": f.bytes}))
        .collect();

    // Duplicati: prima per dimensione, poi per impronta SHA-256, con un tetto ai byte letti.
    let mut by_size: BTreeMap<u64, Vec<&Entry>> = BTreeMap::new();
    for f in files.iter().filter(|f| f.bytes >= min_bytes) {
        by_size.entry(f.bytes).or_default().push(f);
    }
    let mut groups: Vec<(u64, Vec<String>)> = Vec::new();
    let mut hashed = 0u64;
    let mut skipped = 0usize;
    for (size, same) in by_size.iter().filter(|(_, v)| v.len() > 1) {
        if *size > MAX_HASH_FILE || hashed + size * same.len() as u64 > MAX_HASH_TOTAL {
            skipped += same.len();
            continue;
        }
        let mut by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in same {
            if let Some(h) = digest(&root.join(&f.rel)) {
                hashed += size;
                by_hash.entry(h).or_default().push(f.rel.clone());
            }
        }
        for (_, mut paths) in by_hash {
            if paths.len() > 1 {
                paths.sort();
                groups.push((*size, paths));
            }
        }
    }
    groups.sort_by(|a, b| {
        let waste = |g: &(u64, Vec<String>)| g.0 * (g.1.len() as u64 - 1);
        waste(b).cmp(&waste(a)).then(a.1.cmp(&b.1))
    });
    let wasted: u64 = groups.iter().map(|g| g.0 * (g.1.len() as u64 - 1)).sum();
    let duplicate_groups: Vec<Value> = groups
        .iter()
        .take(top)
        .map(|(size, paths)| json!({"bytes_each": size, "copies": paths.len(), "paths": paths}))
        .collect();

    Ok(json!({
        "path": rel,
        "files": files.len(),
        "total_bytes": total,
        "folders": ranked(by_top.into_iter().collect()),
        "extensions": extensions,
        "largest": largest,
        "duplicates": {"groups": duplicate_groups, "group_count": groups.len(), "wasted_bytes": wasted,
            "not_checked": skipped},
        "temporary": {"files": temp_files, "bytes": temp_bytes},
        "truncated": truncated,
        "read_only": true,
        "note": "Analisi in sola lettura. Per eliminare un file usa fs.delete: richiede la conferma del proprietario."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> std::path::PathBuf {
        let ws = std::env::current_dir().unwrap().join(format!(
            "target/jarvis-assets-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(ws.join("docs")).unwrap();
        ws
    }

    #[test]
    fn usage_finds_big_files_duplicates_and_temporaries_without_touching_anything() {
        let ws = workspace();
        std::fs::write(ws.join("docs/a.txt"), "stesso contenuto").unwrap();
        std::fs::write(ws.join("copia.txt"), "stesso contenuto").unwrap();
        std::fs::write(ws.join("altro.txt"), "stesso contenuto!").unwrap();
        std::fs::write(ws.join("grande.bin"), vec![7u8; 5000]).unwrap();
        std::fs::write(ws.join("scarto.tmp"), "x").unwrap();
        let r = usage(&ws, &json!({"top": 3})).unwrap();
        assert_eq!(r["files"], 5);
        assert_eq!(r["largest"][0]["path"], "grande.bin");
        assert_eq!(r["duplicates"]["group_count"], 1);
        assert_eq!(r["duplicates"]["groups"][0]["copies"], 2);
        assert_eq!(r["duplicates"]["wasted_bytes"], 16);
        assert_eq!(r["temporary"]["files"][0]["path"], "scarto.tmp");
        assert_eq!(r["read_only"], true);
        assert!(ws.join("copia.txt").exists() && ws.join("docs/a.txt").exists());
        let sub = usage(&ws, &json!({"path": "docs"})).unwrap();
        assert_eq!(sub["files"], 1);
        assert_eq!(sub["duplicates"]["group_count"], 0);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn paths_outside_the_workspace_and_links_are_not_followed() {
        let ws = workspace();
        assert_eq!(
            usage(&ws, &json!({"path": "../fuori"})).unwrap_err().code,
            "fs.outside_workspace"
        );
        #[cfg(unix)]
        {
            std::fs::write(ws.join("vero.txt"), "dato").unwrap();
            std::os::unix::fs::symlink(ws.join("vero.txt"), ws.join("link.txt")).unwrap();
            assert_eq!(usage(&ws, &json!({})).unwrap()["files"], 1);
        }
        let _ = std::fs::remove_dir_all(&ws);
    }
}
