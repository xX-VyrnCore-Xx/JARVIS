//! Cronologia dei permessi (ADR-0210): ogni modifica a permessi, autonomia e arresto di emergenza lascia
//! una voce con **cosa** è cambiato (campo, valore precedente e nuovo). Solo configurazione: nessun dato
//! personale, nessun contenuto. Il file è limitato a `MAX_ENTRIES` voci, le più vecchie escono per prime.
//! Il registro di audit firmato resta la fonte di verità sul *quando*; qui c'è il *cosa*.

use std::path::Path;

use serde_json::{json, Value};

const FILE: &str = "permission-history.json";
const MAX_ENTRIES: usize = 200;
const MAX_CHANGES: usize = 40;

/// Differenze foglia per foglia fra due documenti JSON: `(percorso, prima, dopo)`.
pub fn diff(before: &Value, after: &Value) -> Vec<(String, Value, Value)> {
    let mut out = Vec::new();
    walk("", before, after, &mut out, 0);
    out.truncate(MAX_CHANGES);
    out
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<(String, Value, Value)>, depth: usize) {
    if a == b {
        return;
    }
    if let (Some(x), Some(y), true) = (a.as_object(), b.as_object(), depth < 4) {
        let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            let child = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            walk(
                &child,
                x.get(k).unwrap_or(&Value::Null),
                y.get(k).unwrap_or(&Value::Null),
                out,
                depth + 1,
            );
        }
        return;
    }
    out.push((path.to_string(), a.clone(), b.clone()));
}

fn read_all(dir: &Path) -> Vec<Value> {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
        .unwrap_or_default()
}

/// Registra una modifica; senza differenze non scrive nulla. `kind`: `permissions`, `autonomy`, `emergency`.
pub fn record(dir: &Path, kind: &str, before: &Value, after: &Value) -> Result<usize, String> {
    let changes = diff(before, after);
    if changes.is_empty() {
        return Ok(0);
    }
    let count = changes.len();
    let mut all = read_all(dir);
    all.push(json!({
        "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "kind": kind,
        "changes": changes.into_iter().map(|(field, from, to)| json!({"field": field, "from": from, "to": to})).collect::<Vec<_>>(),
    }));
    if all.len() > MAX_ENTRIES {
        all.drain(..all.len() - MAX_ENTRIES);
    }
    std::fs::write(
        dir.join(FILE),
        serde_json::to_vec(&all).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(count)
}

/// Le voci più recenti per prime; `kind` opzionale per filtrare.
pub fn recent(dir: &Path, limit: usize, kind: Option<&str>) -> Value {
    let all = read_all(dir);
    let total = all.len();
    let items: Vec<Value> = all
        .into_iter()
        .rev()
        .filter(|e| kind.is_none_or(|k| e["kind"] == k))
        .take(limit.clamp(1, MAX_ENTRIES))
        .collect();
    json!({"total": total, "items": items, "max_entries": MAX_ENTRIES})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("jarvis-permhist-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_real_changes_are_recorded_with_before_and_after() {
        let d = dir();
        let before = json!({"profile": "standard", "rules": {"web.*": "ask"}});
        assert_eq!(record(&d, "permissions", &before, &before).unwrap(), 0);
        assert_eq!(recent(&d, 10, None)["total"], 0);
        let after = json!({"profile": "prudent", "rules": {"web.*": "ask", "app.*": "deny"}});
        assert_eq!(record(&d, "permissions", &before, &after).unwrap(), 2);
        let h = recent(&d, 10, None);
        let fields: Vec<&str> = h["items"][0]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["field"].as_str().unwrap())
            .collect();
        assert_eq!(fields, vec!["profile", "rules.app.*"]);
        assert_eq!(h["items"][0]["changes"][0]["from"], "standard");
        assert_eq!(h["items"][0]["changes"][1]["from"], Value::Null);
        assert_eq!(
            recent(&d, 10, Some("autonomy"))["items"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_history_is_bounded() {
        let d = dir();
        for i in 0..(MAX_ENTRIES + 15) {
            record(
                &d,
                "autonomy",
                &json!({"default": 1}),
                &json!({"default": i + 2}),
            )
            .unwrap();
        }
        let h = recent(&d, 500, None);
        assert_eq!(h["total"], MAX_ENTRIES);
        assert_eq!(h["items"][0]["changes"][0]["to"], MAX_ENTRIES + 16);
        let _ = std::fs::remove_dir_all(d);
    }
}
