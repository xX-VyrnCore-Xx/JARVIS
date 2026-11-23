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
    let id = uuid::Uuid::new_v4().simple();
    let target = root.join(format!("{safe}-{stamp}"));
    let target = if target.exists() {
        root.join(format!("{safe}-{id}-{stamp}"))
    } else {
        target
    };
    let staging = root.join(format!(".in-corso-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(&root)?;
        crate::backup::private_dir(&staging)?;
        for name in [
            "jarvis.sqlite3",
            "jarvis.sqlite3-wal",
            "jarvis.sqlite3-shm",
            ".secrets.key",
            "VERSION",
        ]
        .into_iter()
        .chain(crate::backup::DATA_SETTINGS.iter().copied())
        {
            let src = data_dir.join(name);
            if src.is_file() {
                let dest = staging.join(name);
                fs::create_dir_all(dest.parent().expect("cartella della copia"))?;
                fs::copy(&src, dest)?;
            }
        }
        if data_dir.join("audit").is_dir() {
            copy_tree(&data_dir.join("audit"), &staging.join("audit"))?;
        }
        let workspace = data_dir.join("workspace");
        if workspace.is_dir() {
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
    crate::backup::atomic_write(&data_dir.join("VERSION"), CURRENT.as_bytes())
        .map_err(|e| format!("VERSION: {e}"))
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
    let id = uuid::Uuid::new_v4().simple();
    let aside = data_dir.with_file_name(format!("{name}.before-rollback-{stamp}-{id}"));
    let staging = data_dir.with_file_name(format!(".{name}-rollback-{id}"));
    crate::backup::private_dir(&staging).map_err(|e| format!("cartella temporanea: {e}"))?;
    let result = (|| {
        copy_tree(&source, &staging).map_err(|e| format!("copia non riuscita: {e}"))?;
        fs::create_dir_all(staging.join("workspace")).map_err(|e| e.to_string())?;
        crate::backup::validate_database(&staging.join("jarvis.sqlite3"))?;
        let version = fs::read_to_string(staging.join("VERSION")).unwrap_or_else(|_| "?".into());
        let previous = replace_data(data_dir, &staging, &aside)?;
        Ok(json!({
            "restored_from": source.to_string_lossy(), "version": version.trim(),
            "previous_data": previous.then(|| aside.to_string_lossy()),
        }))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

/// Installa i dati già verificati. Se lo spostamento finale fallisce, rimette la cartella precedente.
pub(crate) fn replace_data(data: &Path, staging: &Path, aside: &Path) -> Result<bool, String> {
    let has_previous = match fs::symlink_metadata(data) {
        Ok(info) if info.is_dir() => {
            let marker = ".macos-owner-session";
            if data.join(marker).is_file() && !staging.join(marker).is_file() {
                crate::backup::private_file(&staging.join(marker))
                    .map_err(|e| format!("conservazione della sessione macOS: {e}"))?;
            }
            fs::rename(data, aside).map_err(|e| format!("spostamento dei dati attuali: {e}"))?;
            true
        }
        Ok(_) => return Err("la destinazione deve essere una cartella dati reale".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("lettura della cartella dati: {e}")),
    };
    if let Err(e) = fs::rename(staging, data) {
        if has_previous {
            fs::rename(aside, data).map_err(|rollback| format!(
                "installazione non riuscita ({e}) e recupero automatico non riuscito ({rollback}). I dati precedenti sono in {}",
                aside.display()
            ))?;
        }
        return Err(format!("installazione dei dati non riuscita: {e}"));
    }
    Ok(has_previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_install_puts_original_data_back() {
        let root =
            std::env::temp_dir().join(format!("jarvis-swap-{}", uuid::Uuid::new_v4().simple()));
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("sentinel"), "originale").unwrap();
        let aside = root.join("precedenti");
        assert!(replace_data(&data, &root.join("staging-assente"), &aside).is_err());
        assert_eq!(
            fs::read_to_string(data.join("sentinel")).unwrap(),
            "originale"
        );
        assert!(!aside.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
