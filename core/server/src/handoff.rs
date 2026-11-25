//! Passaggio di un'attività fra dispositivi (ADR-0180): il proprietario prepara dal Core un «pacchetto di ripresa»
//! (obiettivo, stato del piano, nota) e un dispositivo associato lo riprende. Il passaggio **non trasferisce
//! permessi**: chi riprende lavora con i propri permessi e livelli, e ogni conferma resta sua. Un pacchetto
//! scade (30 minuti) e si riprende **una sola volta**: il secondo dispositivo che prova a riprenderlo riceve un
//! rifiuto con il nome di chi l'ha già fatto, così due dispositivi non portano avanti lo stesso lavoro senza saperlo.
//! Il testo viene ripulito dalle credenziali e accorciato.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS handoffs (id TEXT PRIMARY KEY, to_device TEXT NOT NULL, plan_id TEXT, goal TEXT NOT NULL,
    note TEXT NOT NULL, created_at TEXT NOT NULL, expires_at TEXT NOT NULL, state TEXT NOT NULL,
    accepted_by TEXT, accepted_at TEXT);
";

pub const TTL_MINUTES: i64 = 30;
const MAX_OPEN: i64 = 20;

fn ts(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn clean(text: &str, max: usize) -> Result<String, String> {
    if crate::cloud::contains_credentials(text) {
        return Err(
            "il testo sembra contenere una credenziale: non lo passo a un altro dispositivo".into(),
        );
    }
    Ok(text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect())
}

/// Riga salvata di un passaggio: destinazione, piano, obiettivo, nota, scadenza, stato e chi l'ha ripreso.
type Row = (
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Option<String>,
);

/// Crea un passaggio verso `to` (`device_id` oppure `any`). `plan` è il riepilogo del piano, se c'è.
pub fn create(
    store: &Store,
    to: &str,
    goal: &str,
    note: &str,
    plan: Option<&Value>,
) -> Result<Value, String> {
    let db = |e: rusqlite::Error| e.to_string();
    if to.is_empty() || to.chars().count() > 64 {
        return Err("destinazione non valida".into());
    }
    let goal = clean(goal, 300)?;
    if goal.is_empty() {
        return Err("serve un obiettivo da passare".into());
    }
    let note = clean(note, 300)?;
    let now = chrono::Utc::now();
    store
        .conn
        .execute(
            "UPDATE handoffs SET state = 'expired' WHERE state = 'pending' AND expires_at < ?1",
            [ts(now)],
        )
        .map_err(db)?;
    let open: i64 = store
        .conn
        .query_row(
            "SELECT COUNT(*) FROM handoffs WHERE state = 'pending'",
            [],
            |r| r.get(0),
        )
        .map_err(db)?;
    if open >= MAX_OPEN {
        return Err(format!("al massimo {MAX_OPEN} passaggi aperti"));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    store
        .conn
        .execute(
            "INSERT INTO handoffs (id, to_device, plan_id, goal, note, created_at, expires_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending')",
            params![
                id,
                to,
                plan.and_then(|p| p["id"].as_str()),
                goal,
                note,
                ts(now),
                ts(now + chrono::Duration::minutes(TTL_MINUTES))
            ],
        )
        .map_err(db)?;
    Ok(
        json!({"id": id, "to": to, "goal": goal, "note": note, "expires_at": ts(now + chrono::Duration::minutes(TTL_MINUTES)),
              "plan": plan.cloned().unwrap_or(Value::Null), "state": "pending"}),
    )
}

/// Passaggi ancora validi per un dispositivo (o per qualsiasi dispositivo se `to` è `any`).
pub fn pending_for(store: &Store, device: &str) -> StoreResult<Vec<Value>> {
    let now = ts(chrono::Utc::now());
    let mut stmt = store.conn.prepare(
        "SELECT id, to_device, plan_id, goal, note, created_at, expires_at FROM handoffs
         WHERE state = 'pending' AND expires_at >= ?1 AND (to_device = ?2 OR to_device = 'any')
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![now, device], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?, "to": r.get::<_, String>(1)?, "plan_id": r.get::<_, Option<String>>(2)?,
            "goal": r.get::<_, String>(3)?, "note": r.get::<_, String>(4)?,
            "created_at": r.get::<_, String>(5)?, "expires_at": r.get::<_, String>(6)?,
        }))
    })?;
    rows.collect()
}

/// Riprende un passaggio: una sola volta, solo se destinato a `device` (o a `any`) e non scaduto.
pub fn accept(store: &Store, id: &str, device: &str) -> Result<Value, String> {
    let db = |e: rusqlite::Error| e.to_string();
    let row: Option<Row> = store
        .conn
        .query_row(
            "SELECT to_device, plan_id, goal, note, expires_at, state, accepted_by FROM handoffs WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(db)?;
    let Some((to, plan_id, goal, note, expires_at, state, by)) = row else {
        return Err("passaggio inesistente".into());
    };
    if to != "any" && to != device {
        return Err("il passaggio è destinato a un altro dispositivo".into());
    }
    if state == "accepted" {
        return Err(format!(
            "già ripreso da {}",
            by.unwrap_or_else(|| "un altro dispositivo".into())
        ));
    }
    if state != "pending" || expires_at < ts(chrono::Utc::now()) {
        return Err("il passaggio è scaduto".into());
    }
    let changed = store
        .conn
        .execute(
            "UPDATE handoffs SET state = 'accepted', accepted_by = ?2, accepted_at = ?3
             WHERE id = ?1 AND state = 'pending'",
            params![id, device, ts(chrono::Utc::now())],
        )
        .map_err(db)?;
    if changed == 0 {
        return Err("già ripreso da un altro dispositivo".into());
    }
    Ok(
        json!({"id": id, "goal": goal, "note": note, "plan_id": plan_id, "accepted_by": device,
              "permissions": "nessun permesso è trasferito: valgono quelli del dispositivo che riprende"}),
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
    fn a_handoff_is_resumed_once_by_the_right_device_and_never_twice() {
        let s = store();
        let h = create(
            &s,
            "tablet",
            "Finire il report",
            "parti dalla sezione 2",
            None,
        )
        .unwrap();
        let id = h["id"].as_str().unwrap();
        assert_eq!(pending_for(&s, "tablet").unwrap().len(), 1);
        assert_eq!(pending_for(&s, "telefono").unwrap().len(), 0);
        assert!(accept(&s, id, "telefono")
            .unwrap_err()
            .contains("altro dispositivo"));
        let taken = accept(&s, id, "tablet").unwrap();
        assert_eq!(taken["goal"], "Finire il report");
        assert!(taken["permissions"]
            .as_str()
            .unwrap()
            .contains("nessun permesso"));
        assert!(accept(&s, id, "tablet")
            .unwrap_err()
            .contains("già ripreso da tablet"));
        assert!(pending_for(&s, "tablet").unwrap().is_empty());
        assert!(accept(&s, "inesistente", "tablet").is_err());
    }

    #[test]
    fn any_device_can_take_an_open_handoff_but_only_one() {
        let s = store();
        let h = create(&s, "any", "Controllare il server", "", None).unwrap();
        let id = h["id"].as_str().unwrap();
        assert!(accept(&s, id, "telefono").is_ok());
        assert!(accept(&s, id, "tablet")
            .unwrap_err()
            .contains("già ripreso da telefono"));
    }

    #[test]
    fn expired_handoffs_cannot_be_taken_and_secrets_never_travel() {
        let s = store();
        let h = create(&s, "tablet", "Compito", "", None).unwrap();
        let id = h["id"].as_str().unwrap();
        s.conn
            .execute(
                "UPDATE handoffs SET expires_at = '2020-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        assert!(pending_for(&s, "tablet").unwrap().is_empty());
        assert!(accept(&s, id, "tablet").unwrap_err().contains("scaduto"));
        assert!(create(
            &s,
            "tablet",
            "usa la chiave sk-abcdefghijklmnopqrstuvwxyz0123456789",
            "",
            None
        )
        .is_err());
        assert!(create(&s, "tablet", "", "", None).is_err());
        assert!(create(&s, "", "x y", "", None).is_err());
    }
}
