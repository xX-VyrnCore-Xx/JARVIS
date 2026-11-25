//! Automazioni guidate da eventi (ADR-0158): un workflow parte quando cambia un file o una cartella della workspace.
//!
//! Le regole le gestisce solo il proprietario (`/api/watches`). Il controllo avviene a ogni giro dello scheduler: si
//! confronta una firma (esistenza, dimensione e data di modifica; per le cartelle numero di voci e data più recente).
//! La prima lettura fissa il punto di partenza e non avvia nulla. Il workflow collegato esegue con i suoi permessi
//! già approvati: la regola non li amplia.

use std::path::Path;

use serde_json::{json, Value};

use crate::tools::resolve;

const FILE: &str = "watches.json";
pub const MAX_WATCHES: usize = 16;

fn load(dir: &Path) -> Vec<Value> {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["watches"].as_array().cloned())
        .unwrap_or_default()
}

fn save(dir: &Path, watches: &[Value]) -> Result<(), String> {
    std::fs::write(dir.join(FILE), json!({"watches": watches}).to_string())
        .map_err(|e| e.to_string())
}

pub fn list(dir: &Path) -> Value {
    json!({"items": load(dir), "max": MAX_WATCHES})
}

/// Firma di un percorso: cambia quando cambia il contenuto osservabile.
pub fn signature(path: &Path) -> String {
    let stamp = |m: &std::fs::Metadata| {
        m.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis())
    };
    match std::fs::metadata(path) {
        Err(_) => "assente".into(),
        Ok(meta) if meta.is_dir() => {
            let (mut count, mut newest) = (0u64, 0u128);
            for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                count += 1;
                if let Ok(m) = entry.metadata() {
                    newest = newest.max(stamp(&m));
                }
            }
            format!("cartella:{count}:{newest}")
        }
        Ok(meta) => format!("file:{}:{}", meta.len(), stamp(&meta)),
    }
}

/// Aggiunge una regola: percorso nella workspace e workflow da avviare (nome, id o prefisso).
pub fn add(dir: &Path, workspace: &Path, path: &str, workflow: &str) -> Result<Value, String> {
    if workflow.trim().is_empty() || workflow.len() > 80 {
        return Err("Indica il workflow da avviare".into());
    }
    let target = resolve(workspace, path, false).map_err(|e| e.message)?;
    let mut watches = load(dir);
    if watches.len() >= MAX_WATCHES {
        return Err(format!("Al massimo {MAX_WATCHES} regole"));
    }
    let rule = json!({
        "id": uuid::Uuid::new_v4().simple().to_string()[..8],
        "path": path, "workflow": workflow.trim(),
        "baseline": signature(&target), "created_at": crate::now(),
    });
    watches.push(rule.clone());
    save(dir, &watches)?;
    Ok(rule)
}

pub fn remove(dir: &Path, id: &str) -> Result<bool, String> {
    let mut watches = load(dir);
    let before = watches.len();
    watches.retain(|w| w["id"] != id);
    save(dir, &watches)?;
    Ok(watches.len() < before)
}

/// Regole il cui percorso è cambiato dall'ultimo controllo; la firma nuova diventa il punto di partenza.
pub fn changed(dir: &Path, workspace: &Path) -> Vec<Value> {
    let mut watches = load(dir);
    let mut fired = Vec::new();
    let mut dirty = false;
    for rule in &mut watches {
        let Ok(target) = resolve(workspace, rule["path"].as_str().unwrap_or_default(), false)
        else {
            continue;
        };
        let now = signature(&target);
        if rule["baseline"] != now.as_str() {
            fired.push(rule.clone());
            rule["baseline"] = json!(now);
            rule["last_fired_at"] = json!(crate::now());
            dirty = true;
        }
    }
    if dirty {
        let _ = save(dir, &watches);
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_detected_once_and_the_first_read_is_a_baseline() {
        let root =
            std::env::temp_dir().join(format!("jarvis-watch-{}", uuid::Uuid::new_v4().simple()));
        let workspace = root.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("in.txt"), "uno").unwrap();
        let rule = add(&root, &workspace, "in.txt", "Elabora").unwrap();
        assert!(
            changed(&root, &workspace).is_empty(),
            "nessun cambiamento dalla creazione"
        );
        std::fs::write(workspace.join("in.txt"), "uno e due").unwrap();
        let fired = changed(&root, &workspace);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0]["id"], rule["id"]);
        assert!(
            changed(&root, &workspace).is_empty(),
            "una sola volta per cambiamento"
        );
        // Cartella: nuova voce, e file che compare dove prima non c'era.
        std::fs::create_dir_all(workspace.join("in")).unwrap();
        add(&root, &workspace, "in", "Elabora").unwrap();
        std::fs::write(workspace.join("in/nuovo.txt"), "x").unwrap();
        assert_eq!(changed(&root, &workspace).len(), 1);
        add(&root, &workspace, "futuro.txt", "Elabora").unwrap();
        std::fs::write(workspace.join("futuro.txt"), "x").unwrap();
        assert_eq!(changed(&root, &workspace).len(), 1);
        // Percorsi fuori dalla workspace e regole in eccesso sono rifiutati.
        assert!(add(&root, &workspace, "../fuori", "x").is_err());
        assert!(add(&root, &workspace, "in.txt", "").is_err());
        assert!(remove(&root, rule["id"].as_str().unwrap()).unwrap());
        assert!(!remove(&root, "inesistente").unwrap());
        for _ in 0..MAX_WATCHES {
            let _ = add(&root, &workspace, "in.txt", "x");
        }
        assert!(add(&root, &workspace, "in.txt", "x").is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
