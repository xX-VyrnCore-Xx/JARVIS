//! Stato del mondo (ADR-0175): rappresentazione strutturata dell'ambiente operativo (sistema, dischi, processi,
//! repository, piani, missioni, nodi, automazioni, strumenti, regole di sicurezza).
//!
//! Ogni elemento dichiara la **fonte**: `observed` (letto dal Core con uno strumento o dal proprio stato) o
//! `inferred` (dedotto, ad esempio dal modello, con fiducia e motivo). Un'osservazione recente **non** viene mai
//! sovrascritta da una deduzione; un'osservazione nuova sostituisce la deduzione e, se i valori divergono, lo
//! registra come contraddizione. Ogni elemento ha una versione (cresce a ogni cambiamento reale) e una durata di
//! validità: oltre quella è `stale` e non va presentato come attuale. Gli elementi spariti dall'ambiente
//! osservato sono rimossi e il fatto resta nella cronologia degli eventi. Nessun segreto, contenuto di file o
//! riga di comando entra nello stato: solo nomi, stati e conteggi.

use rusqlite::params;
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS world_entities (key TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
    source TEXT NOT NULL, value TEXT NOT NULL, version INTEGER NOT NULL, observed_at TEXT NOT NULL,
    ttl_s INTEGER NOT NULL, confidence REAL, note TEXT);
CREATE INDEX IF NOT EXISTS world_by_kind ON world_entities (kind);
CREATE TABLE IF NOT EXISTS world_edges (from_key TEXT NOT NULL, to_key TEXT NOT NULL, relation TEXT NOT NULL,
    PRIMARY KEY (from_key, to_key, relation));
CREATE TABLE IF NOT EXISTS world_events (seq INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL, key TEXT NOT NULL,
    change TEXT NOT NULL, detail TEXT NOT NULL);
";

/// Elementi e archi sono limitati: lo stato è una vista, non un archivio.
const MAX_ENTITIES: i64 = 2000;
const MAX_EVENTS: i64 = 2000;
const MAX_VALUE_BYTES: usize = 2048;

pub const KINDS: [&str; 12] = [
    "system",
    "disk",
    "process",
    "repository",
    "plan",
    "mission",
    "node",
    "automation",
    "tool",
    "policy",
    "device",
    "fact",
];

/// Esito di una registrazione.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Created,
    Changed,
    Refreshed,
    /// Una deduzione è stata sostituita da un'osservazione; `contradicted` se i valori divergevano.
    Reconciled {
        contradicted: bool,
    },
}

fn now_s() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn key_of(kind: &str, name: &str) -> String {
    format!("{kind}:{name}")
}

fn clip(value: &Value) -> Value {
    let text = value.to_string();
    if text.len() <= MAX_VALUE_BYTES {
        value.clone()
    } else {
        json!({"truncated": true, "preview": text.chars().take(MAX_VALUE_BYTES / 2).collect::<String>()})
    }
}

fn valid_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

fn event(store: &Store, at: &str, key: &str, change: &str, detail: &Value) -> StoreResult<()> {
    store.conn.execute(
        "INSERT INTO world_events (at, key, change, detail) VALUES (?1, ?2, ?3, ?4)",
        params![at, key, change, detail.to_string()],
    )?;
    store.conn.execute(
        "DELETE FROM world_events WHERE seq <= (SELECT MAX(seq) FROM world_events) - ?1",
        params![MAX_EVENTS],
    )?;
    Ok(())
}

/// Riga salvata: fonte, valore, versione, istante dell'ultima lettura e durata di validità.
type Stored = (String, Value, i64, String, i64);

fn existing(store: &Store, key: &str) -> StoreResult<Option<Stored>> {
    let mut stmt = store.conn.prepare(
        "SELECT source, value, version, observed_at, ttl_s FROM world_entities WHERE key = ?1",
    )?;
    let mut rows = stmt.query(params![key])?;
    Ok(match rows.next()? {
        Some(r) => Some((
            r.get(0)?,
            serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or(Value::Null),
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
        )),
        None => None,
    })
}

fn age_s(observed_at: &str) -> i64 {
    crate::automation::parse_ts(observed_at)
        .map(|t| (chrono::Utc::now() - t).num_seconds().max(0))
        .unwrap_or(i64::MAX)
}

fn count(store: &Store) -> StoreResult<i64> {
    store
        .conn
        .query_row("SELECT COUNT(*) FROM world_entities", [], |r| r.get(0))
}

/// Registra un'**osservazione**. Un valore invariato aggiorna solo l'istante (la versione non cresce).
pub fn observe(
    store: &Store,
    kind: &str,
    name: &str,
    value: &Value,
    ttl_s: i64,
) -> Result<Change, String> {
    if !valid_kind(kind) || name.is_empty() || name.chars().count() > 120 {
        return Err("tipo o nome non validi".into());
    }
    let key = key_of(kind, name);
    let value = clip(value);
    let at = now_s();
    let db = |e: rusqlite::Error| e.to_string();
    let change = match existing(store, &key).map_err(db)? {
        None => {
            if count(store).map_err(db)? >= MAX_ENTITIES {
                return Err("stato del mondo pieno".into());
            }
            Change::Created
        }
        Some((source, old, _, _, _)) if source == "inferred" => Change::Reconciled {
            contradicted: old != value,
        },
        Some((_, old, _, _, _)) if old == value => Change::Refreshed,
        Some(_) => Change::Changed,
    };
    let version_bump = i64::from(!matches!(change, Change::Refreshed));
    store
        .conn
        .execute(
            "INSERT INTO world_entities (key, kind, name, source, value, version, observed_at, ttl_s, confidence, note)
             VALUES (?1, ?2, ?3, 'observed', ?4, 1, ?5, ?6, NULL, NULL)
             ON CONFLICT(key) DO UPDATE SET source = 'observed', value = ?4, version = version + ?7,
                 observed_at = ?5, ttl_s = ?6, confidence = NULL, note = NULL",
            params![key, kind, name, value.to_string(), at, ttl_s.clamp(10, 7 * 86_400), version_bump],
        )
        .map_err(db)?;
    match &change {
        Change::Created => event(store, &at, &key, "appeared", &value).map_err(db)?,
        Change::Changed => event(store, &at, &key, "changed", &value).map_err(db)?,
        Change::Reconciled { contradicted } => event(
            store,
            &at,
            &key,
            if *contradicted {
                "contradicted"
            } else {
                "confirmed"
            },
            &value,
        )
        .map_err(db)?,
        Change::Refreshed => {}
    }
    Ok(change)
}

/// Registra una **deduzione**. Rifiutata se esiste un'osservazione ancora valida: ciò che si è visto prevale.
pub fn infer(
    store: &Store,
    kind: &str,
    name: &str,
    value: &Value,
    confidence: f64,
    note: &str,
    ttl_s: i64,
) -> Result<Change, String> {
    if !valid_kind(kind) || name.is_empty() || name.chars().count() > 120 {
        return Err("tipo o nome non validi".into());
    }
    if !(0.0..=1.0).contains(&confidence) {
        return Err("la fiducia va da 0 a 1".into());
    }
    if note.chars().count() > 300 || crate::cloud::contains_credentials(note) {
        return Err("motivo troppo lungo o con credenziali".into());
    }
    let key = key_of(kind, name);
    let value = clip(value);
    let at = now_s();
    let db = |e: rusqlite::Error| e.to_string();
    let prior = existing(store, &key).map_err(db)?;
    if let Some((source, _, _, observed_at, ttl)) = &prior {
        if source == "observed" && age_s(observed_at) <= *ttl {
            return Err(
                "esiste un'osservazione ancora valida: una deduzione non la sostituisce".into(),
            );
        }
    }
    if prior.is_none() && count(store).map_err(db)? >= MAX_ENTITIES {
        return Err("stato del mondo pieno".into());
    }
    let change = match &prior {
        None => Change::Created,
        Some((_, old, _, _, _)) if *old == value => Change::Refreshed,
        Some(_) => Change::Changed,
    };
    store
        .conn
        .execute(
            "INSERT INTO world_entities (key, kind, name, source, value, version, observed_at, ttl_s, confidence, note)
             VALUES (?1, ?2, ?3, 'inferred', ?4, 1, ?5, ?6, ?7, ?8)
             ON CONFLICT(key) DO UPDATE SET source = 'inferred', value = ?4,
                 version = version + ?9, observed_at = ?5, ttl_s = ?6, confidence = ?7, note = ?8",
            params![
                key,
                kind,
                name,
                value.to_string(),
                at,
                ttl_s.clamp(10, 7 * 86_400),
                confidence,
                note,
                i64::from(!matches!(change, Change::Refreshed))
            ],
        )
        .map_err(db)?;
    if change != Change::Refreshed {
        event(
            store,
            &at,
            &key,
            "inferred",
            &json!({"value": value, "confidence": confidence}),
        )
        .map_err(db)?;
    }
    Ok(change)
}

/// Rimuove gli elementi di un tipo non più presenti fra le chiavi osservate nel giro di sincronizzazione.
pub fn retain_only(store: &Store, kind: &str, present: &[String]) -> StoreResult<usize> {
    let mut stmt = store
        .conn
        .prepare("SELECT key FROM world_entities WHERE kind = ?1 AND source = 'observed'")?;
    let keys: Vec<String> = stmt
        .query_map(params![kind], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let at = now_s();
    let mut gone = 0;
    for key in keys.into_iter().filter(|k| !present.contains(k)) {
        store
            .conn
            .execute("DELETE FROM world_entities WHERE key = ?1", params![key])?;
        store.conn.execute(
            "DELETE FROM world_edges WHERE from_key = ?1 OR to_key = ?1",
            params![key],
        )?;
        event(store, &at, &key, "disappeared", &Value::Null)?;
        gone += 1;
    }
    Ok(gone)
}

pub fn link(store: &Store, from: &str, to: &str, relation: &str) -> StoreResult<()> {
    store.conn.execute(
        "INSERT OR IGNORE INTO world_edges (from_key, to_key, relation) VALUES (?1, ?2, ?3)",
        params![from, to, relation],
    )?;
    Ok(())
}

/// Elenco con età e validità. `kind` filtra per tipo; `only_stale` mostra solo i dati non più attuali.
pub fn list(store: &Store, kind: Option<&str>, only_stale: bool) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT key, kind, name, source, value, version, observed_at, ttl_s, confidence, note
         FROM world_entities WHERE (?1 IS NULL OR kind = ?1) ORDER BY kind, name",
    )?;
    let rows = stmt.query_map(params![kind], |r| {
        let observed_at: String = r.get(6)?;
        let ttl: i64 = r.get(7)?;
        let age = age_s(&observed_at);
        Ok(json!({
            "key": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "name": r.get::<_, String>(2)?,
            "source": r.get::<_, String>(3)?,
            "value": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
            "version": r.get::<_, i64>(5)?, "observed_at": observed_at, "age_s": age,
            "ttl_s": ttl, "stale": age > ttl,
            "confidence": r.get::<_, Option<f64>>(8)?, "note": r.get::<_, Option<String>>(9)?,
        }))
    })?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|e| !only_stale || e["stale"] == true)
        .collect())
}

/// Cosa dipende da cosa: archi uscenti ed entranti di un elemento.
pub fn dependencies(store: &Store, key: &str) -> StoreResult<Value> {
    let mut stmt = store
        .conn
        .prepare("SELECT to_key, relation FROM world_edges WHERE from_key = ?1 ORDER BY to_key")?;
    let uses: Vec<Value> = stmt
        .query_map(params![key], |r| {
            Ok(json!({"key": r.get::<_, String>(0)?, "relation": r.get::<_, String>(1)?}))
        })?
        .collect::<Result<_, _>>()?;
    let mut stmt = store.conn.prepare(
        "SELECT from_key, relation FROM world_edges WHERE to_key = ?1 ORDER BY from_key",
    )?;
    let used_by: Vec<Value> = stmt
        .query_map(params![key], |r| {
            Ok(json!({"key": r.get::<_, String>(0)?, "relation": r.get::<_, String>(1)?}))
        })?
        .collect::<Result<_, _>>()?;
    Ok(json!({"key": key, "uses": uses, "used_by": used_by}))
}

pub fn events(store: &Store, limit: i64, key: Option<&str>) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT seq, at, key, change, detail FROM world_events WHERE (?1 IS NULL OR key = ?1)
         ORDER BY seq DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![key, limit.clamp(1, 200)], |r| {
        Ok(json!({
            "seq": r.get::<_, i64>(0)?, "at": r.get::<_, String>(1)?, "key": r.get::<_, String>(2)?,
            "change": r.get::<_, String>(3)?,
            "detail": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
        }))
    })?;
    rows.collect()
}

/// Riepilogo: quanti elementi, quanti osservati/dedotti/non attuali e le contraddizioni recenti.
pub fn summary(store: &Store) -> StoreResult<Value> {
    let all = list(store, None, false)?;
    let observed = all.iter().filter(|e| e["source"] == "observed").count();
    let inferred = all.iter().filter(|e| e["source"] == "inferred").count();
    let stale = all.iter().filter(|e| e["stale"] == true).count();
    let contradictions = events(store, 200, None)?
        .into_iter()
        .filter(|e| e["change"] == "contradicted")
        .count();
    Ok(
        json!({"entities": all.len(), "observed": observed, "inferred": inferred, "stale": stale,
              "recent_contradictions": contradictions}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn observations_are_versioned_only_when_something_changes() {
        let s = store();
        assert_eq!(
            observe(&s, "system", "local", &json!({"cpu": 10}), 60).unwrap(),
            Change::Created
        );
        assert_eq!(
            observe(&s, "system", "local", &json!({"cpu": 10}), 60).unwrap(),
            Change::Refreshed
        );
        assert_eq!(
            observe(&s, "system", "local", &json!({"cpu": 80}), 60).unwrap(),
            Change::Changed
        );
        let item = &list(&s, Some("system"), false).unwrap()[0];
        assert_eq!(item["version"], 2);
        assert_eq!(item["source"], "observed");
        assert_eq!(item["stale"], false);
        let kinds: Vec<String> = events(&s, 10, None)
            .unwrap()
            .iter()
            .map(|e| e["change"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds, ["changed", "appeared"]);
    }

    #[test]
    fn a_fresh_observation_is_never_overwritten_by_an_inference() {
        let s = store();
        observe(&s, "node", "mini-pc", &json!({"online": true}), 300).unwrap();
        let refused = infer(
            &s,
            "node",
            "mini-pc",
            &json!({"online": false}),
            0.7,
            "modello",
            300,
        );
        assert!(refused.is_err(), "{refused:?}");
        assert_eq!(list(&s, None, false).unwrap()[0]["value"]["online"], true);
    }

    #[test]
    fn an_observation_replaces_an_inference_and_flags_a_contradiction() {
        let s = store();
        infer(
            &s,
            "node",
            "server",
            &json!({"online": true}),
            0.6,
            "ping di ieri",
            300,
        )
        .unwrap();
        let item = &list(&s, None, false).unwrap()[0];
        assert_eq!(item["source"], "inferred");
        assert_eq!(item["confidence"], 0.6);
        let change = observe(&s, "node", "server", &json!({"online": false}), 300).unwrap();
        assert_eq!(change, Change::Reconciled { contradicted: true });
        let item = &list(&s, None, false).unwrap()[0];
        assert_eq!(item["source"], "observed");
        assert!(item["confidence"].is_null());
        assert_eq!(summary(&s).unwrap()["recent_contradictions"], 1);
        // Un'osservazione che conferma la deduzione non è una contraddizione.
        infer(&s, "fact", "a", &json!(1), 0.5, "", 300).unwrap();
        assert_eq!(
            observe(&s, "fact", "a", &json!(1), 300).unwrap(),
            Change::Reconciled {
                contradicted: false
            }
        );
    }

    #[test]
    fn old_data_is_flagged_stale_and_an_old_observation_can_be_replaced() {
        let s = store();
        observe(&s, "disk", "/", &json!({"used": 50}), 60).unwrap();
        s.conn
            .execute(
                "UPDATE world_entities SET observed_at = '2020-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        let stale = list(&s, None, true).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0]["stale"], true);
        // Un'osservazione ormai vecchia non impedisce una deduzione più recente.
        assert!(infer(&s, "disk", "/", &json!({"used": 70}), 0.5, "stima", 60).is_ok());
    }

    #[test]
    fn vanished_items_are_removed_and_leave_an_event_and_no_edges() {
        let s = store();
        observe(&s, "plan", "a", &json!({}), 60).unwrap();
        observe(&s, "plan", "b", &json!({}), 60).unwrap();
        link(&s, "plan:a", "tool:time.now", "uses").unwrap();
        assert_eq!(retain_only(&s, "plan", &["plan:b".into()]).unwrap(), 1);
        assert_eq!(list(&s, Some("plan"), false).unwrap().len(), 1);
        assert!(dependencies(&s, "plan:a").unwrap()["uses"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(events(&s, 1, None).unwrap()[0]["change"], "disappeared");
    }

    #[test]
    fn dependencies_are_navigable_in_both_directions() {
        let s = store();
        link(&s, "plan:a", "tool:time.now", "uses").unwrap();
        link(&s, "plan:b", "tool:time.now", "uses").unwrap();
        let d = dependencies(&s, "tool:time.now").unwrap();
        assert_eq!(d["used_by"].as_array().unwrap().len(), 2);
        assert_eq!(
            dependencies(&s, "plan:a").unwrap()["uses"][0]["key"],
            "tool:time.now"
        );
    }

    #[test]
    fn invalid_input_and_secrets_are_rejected_and_big_values_are_clipped() {
        let s = store();
        assert!(observe(&s, "sconosciuto", "x", &json!(1), 60).is_err());
        assert!(observe(&s, "fact", "", &json!(1), 60).is_err());
        assert!(infer(&s, "fact", "x", &json!(1), 1.5, "", 60).is_err());
        assert!(infer(
            &s,
            "fact",
            "x",
            &json!(1),
            0.5,
            "chiave sk-abcdefghijklmnopqrstuvwxyz0123456789",
            60
        )
        .is_err());
        observe(&s, "fact", "grande", &json!({"t": "x".repeat(5000)}), 60).unwrap();
        assert_eq!(
            list(&s, None, false).unwrap()[0]["value"]["truncated"],
            true
        );
    }
}
