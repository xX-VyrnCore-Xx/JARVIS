//! Rollback di release (RNF-08, ADR-0015).
//!
//! La cartella dati registra in `VERSION` la versione del Core che l'ha usata per ultima. Quando parte una
//! versione diversa, prima di aprire il database (e quindi prima di qualunque migrazione) il Core copia
//! database, audit, chiave dei segreti e workspace in `<dati>-rollback/<versione>-<istante>/`. Restano le
//! ultime 3 copie. `jarvis rollback` ripristina l'ultima copia spostando i dati attuali da parte, così
//! anche il rollback è reversibile. Le copie hanno la stessa protezione della cartella dati.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const KEEP: usize = 3;
const MAX_WORKSPACE_BYTES: u64 = 1024 * 1024 * 1024;

pub fn rollback_dir(data_dir: &Path) -> PathBuf {
    let name = data_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".into());
    data_dir.with_file_name(format!("{name}-rollback"))
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            total += copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            total += fs::copy(entry.path(), target)?;
        }
    }
    Ok(total)
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .map(|it| {
            it.flatten()
                .map(|e| match e.file_type() {
                    Ok(k) if k.is_dir() => dir_size(&e.path()),
                    Ok(k) if k.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Da chiamare con il blocco della cartella dati acquisito e prima di aprire il database.
/// Restituisce la descrizione della copia creata, se la versione è cambiata.
pub fn prepare(data_dir: &Path) -> Result<Option<Value>, String> {
    let marker = data_dir.join("VERSION");
    let db = data_dir.join("jarvis.sqlite3");
    let previous = fs::read_to_string(&marker)
        .ok()
        .map(|v| v.trim().to_string());
    let previous = match previous {
        Some(v) if v == CURRENT => return Ok(None),
        Some(v) => v,
        // Dati creati prima del registro delle versioni: la versione precedente non è nota.
        None if db.exists() => "precedente".to_string(),
        None => return Ok(None),
    };
    let safe: String = previous
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let root = rollback_dir(data_dir);
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let target = root.join(format!("{safe}-{stamp}"));
    let staging = root.join(format!(".in-corso-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(&staging)?;
        for name in [
            "jarvis.sqlite3",
            "jarvis.sqlite3-wal",
            "jarvis.sqlite3-shm",
            ".secrets.key",
            "VERSION",
        ] {
            let src = data_dir.join(name);
            if src.is_file() {
                fs::copy(&src, staging.join(name))?;
            }
        }
        if data_dir.join("audit").is_dir() {
            copy_tree(&data_dir.join("audit"), &staging.join("audit"))?;
        }
        let workspace = data_dir.join("workspace");
        if workspace.is_dir() && dir_size(&workspace) <= MAX_WORKSPACE_BYTES {
            copy_tree(&workspace, &staging.join("workspace"))?;
        }
        fs::rename(&staging, &target)
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("copia per il rollback non riuscita: {e}"));
    }
    prune(data_dir);
    Ok(Some(
        json!({"from": previous, "to": CURRENT, "dir": target.to_string_lossy()}),
    ))
}

/// Registra la versione corrente: da chiamare dopo un'apertura riuscita.
pub fn mark(data_dir: &Path) -> Result<(), String> {
    fs::write(data_dir.join("VERSION"), CURRENT).map_err(|e| format!("VERSION: {e}"))
}

pub fn list(data_dir: &Path) -> Vec<PathBuf> {
    let mut items: Vec<PathBuf> = fs::read_dir(rollback_dir(data_dir))
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && !p
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                })
                .collect()
        })
        .unwrap_or_default();
    // Il nome termina con l'istante: si ordina su quello.
    items.sort_by_key(|p| {
        p.file_name().map(|n| {
            n.to_string_lossy()
                .rsplit('-')
                .next()
                .unwrap_or("")
                .to_string()
        })
    });
    items
}

fn prune(data_dir: &Path) {
    let items = list(data_dir);
    if items.len() > KEEP {
        for old in &items[..items.len() - KEEP] {
            let _ = fs::remove_dir_all(old);
        }
    }
}

/// Ripristina una copia (l'ultima se `which` è None). I dati attuali vengono spostati in
/// `<dati>.before-rollback-<istante>`. Va eseguito con JARVIS chiuso.
pub fn restore(data_dir: &Path, which: Option<&Path>) -> Result<Value, String> {
    let source = match which {
        Some(p) => p.to_path_buf(),
        None => list(data_dir)
            .pop()
            .ok_or("nessuna copia per il rollback")?,
    };
    if !source.join("jarvis.sqlite3").is_file() {
        return Err(format!(
            "{} non contiene un database JARVIS",
            source.display()
        ));
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let name = data_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".into());
    let aside = data_dir.with_file_name(format!("{name}.before-rollback-{stamp}"));
    let staging = data_dir.with_file_name(format!(
        ".{name}-rollback-{}",
        uuid::Uuid::new_v4().simple()
    ));
    copy_tree(&source, &staging).map_err(|e| format!("copia non riuscita: {e}"))?;
    fs::create_dir_all(staging.join("workspace")).map_err(|e| e.to_string())?;
    if data_dir.exists() {
        fs::rename(data_dir, &aside).map_err(|e| {
            let _ = fs::remove_dir_all(&staging);
            format!("impossibile spostare i dati attuali: {e}")
        })?;
    }
    fs::rename(&staging, data_dir).map_err(|e| format!("ripristino non completato: {e}"))?;
    let version = fs::read_to_string(data_dir.join("VERSION")).unwrap_or_else(|_| "?".into());
    Ok(
        json!({"restored_from": source.to_string_lossy(), "version": version.trim(), "previous_data": aside.to_string_lossy()}),
    )
}
