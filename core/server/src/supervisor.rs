//! Supervisione degli agenti (ADR-0164): il proprietario può mettere in pausa una specializzazione o darle un
//! limite di lavori all'ora. Un agente in pausa o oltre quota **non lavora**: la richiesta passa al generalista,
//! che resta sotto le stesse policy, conferme, budget e audit. La supervisione restringe e non allarga mai: non
//! concede strumenti, permessi né autonomia in più.

use serde_json::{json, Value};
use std::path::Path;

const FILE: &str = "agents-limits.json";
pub const MAX_PER_HOUR: u64 = 500;

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Paused,
    QuotaExceeded,
}

impl Decision {
    pub fn state(&self) -> &'static str {
        match self {
            Decision::Allowed => "attivo",
            Decision::Paused => "in pausa",
            Decision::QuotaExceeded => "quota esaurita",
        }
    }
}

fn read(dir: &Path) -> Value {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"agents": {}}))
}

/// `(attivo, limite orario)` di un agente; senza regole è attivo e senza limite.
pub fn limits_of(dir: &Path, agent: &str) -> (bool, Option<u64>) {
    let doc = read(dir);
    let entry = &doc["agents"][agent];
    (
        entry["enabled"].as_bool().unwrap_or(true),
        entry["max_runs_per_hour"].as_u64(),
    )
}

pub fn decide(enabled: bool, max_per_hour: Option<u64>, runs_last_hour: u64) -> Decision {
    if !enabled {
        Decision::Paused
    } else if max_per_hour.is_some_and(|max| runs_last_hour >= max) {
        Decision::QuotaExceeded
    } else {
        Decision::Allowed
    }
}

/// Aggiorna le regole di un agente. `enabled` e `max_runs_per_hour` sono facoltativi; `max_runs_per_hour: null`
/// toglie il limite. L'agente deve esistere nel registro.
pub fn set(dir: &Path, known: &[&str], body: &Value) -> Result<Value, String> {
    let agent = body["agent"].as_str().unwrap_or_default();
    if !known.contains(&agent) {
        return Err(format!("Agente sconosciuto: «{agent}»"));
    }
    let mut doc = read(dir);
    if !doc["agents"].is_object() {
        doc["agents"] = json!({});
    }
    let mut entry = doc["agents"][agent].clone();
    if !entry.is_object() {
        entry = json!({});
    }
    if let Some(enabled) = body.get("enabled") {
        entry["enabled"] = json!(enabled
            .as_bool()
            .ok_or("«enabled» deve essere vero o falso")?);
    }
    if let Some(max) = body.get("max_runs_per_hour") {
        if max.is_null() {
            entry.as_object_mut().map(|o| o.remove("max_runs_per_hour"));
        } else {
            let n = max
                .as_u64()
                .filter(|n| (1..=MAX_PER_HOUR).contains(n))
                .ok_or(format!(
                    "«max_runs_per_hour»: da 1 a {MAX_PER_HOUR}, oppure null"
                ))?;
            entry["max_runs_per_hour"] = json!(n);
        }
    }
    doc["agents"][agent] = entry.clone();
    std::fs::write(dir.join(FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jarvis-sup-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn decisions_follow_pause_and_quota() {
        assert_eq!(decide(true, None, 1_000), Decision::Allowed);
        assert_eq!(decide(false, None, 0), Decision::Paused);
        assert_eq!(decide(true, Some(3), 2), Decision::Allowed);
        assert_eq!(decide(true, Some(3), 3), Decision::QuotaExceeded);
        assert_eq!(
            decide(false, Some(3), 9),
            Decision::Paused,
            "la pausa vince"
        );
    }

    #[test]
    fn rules_are_validated_persisted_and_removable() {
        let dir = temp();
        let known = ["coding", "security"];
        assert_eq!(limits_of(&dir, "coding"), (true, None));
        set(
            &dir,
            &known,
            &json!({"agent": "coding", "max_runs_per_hour": 5}),
        )
        .unwrap();
        set(&dir, &known, &json!({"agent": "coding", "enabled": false})).unwrap();
        assert_eq!(limits_of(&dir, "coding"), (false, Some(5)));
        assert_eq!(limits_of(&dir, "security"), (true, None));
        set(
            &dir,
            &known,
            &json!({"agent": "coding", "enabled": true, "max_runs_per_hour": null}),
        )
        .unwrap();
        assert_eq!(limits_of(&dir, "coding"), (true, None));
        for bad in [
            json!({"agent": "inventato", "enabled": false}),
            json!({"agent": "coding", "enabled": "no"}),
            json!({"agent": "coding", "max_runs_per_hour": 0}),
            json!({"agent": "coding", "max_runs_per_hour": 501}),
            json!({"agent": "coding", "max_runs_per_hour": "tanti"}),
        ] {
            assert!(set(&dir, &known, &bad).is_err(), "{bad}");
        }
    }
}
