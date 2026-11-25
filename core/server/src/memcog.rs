//! Memoria cognitiva (ADR-0178): punteggio di **importanza**, livello di **affidabilità**, **memoria di progetto** e
//! **consolidamento** sopra la memoria esistente (ADR-0022, 0037, 0125).
//!
//! - Importanza (0–1): calcolata, deterministica e spiegabile da tipo, ricordo fissato, usi, età e affidabilità; è un
//!   criterio di ordinamento e di pulizia, mai una soglia che nasconde un ricordo richiesto per nome.
//! - Affidabilità: `verified` (confermato dal proprietario), `stated` (detto dal proprietario), `inferred` (dedotto
//!   dal Core o da un agente, come gli episodi), `uncertain` (da ricontrollare). Cambia solo per decisione del
//!   proprietario o per regola dichiarata; la provenienza resta in `source`.
//! - Progetto: un ricordo con `project` entra nel contesto solo se la richiesta cita quel progetto.
//! - Consolidamento: ricalcola le importanze e propone o applica la fusione dei quasi-doppioni, tenendo il più
//!   importante; la fusione è nel registro delle operazioni e non tocca i ricordi fissati.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

pub const RELIABILITY: [&str; 4] = ["verified", "stated", "inferred", "uncertain"];

/// Colonne aggiunte (idempotente): importanza, affidabilità, usi, ultimo uso e progetto.
pub fn migrate(store: &Store) -> StoreResult<()> {
    if store
        .conn
        .prepare("SELECT importance FROM memory LIMIT 0")
        .is_err()
    {
        store.conn.execute_batch(
            "ALTER TABLE memory ADD COLUMN importance REAL NOT NULL DEFAULT 0.5;
             ALTER TABLE memory ADD COLUMN reliability TEXT NOT NULL DEFAULT 'stated';
             ALTER TABLE memory ADD COLUMN uses INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE memory ADD COLUMN last_used_at TEXT;
             ALTER TABLE memory ADD COLUMN project TEXT;",
        )?;
    }
    store.conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS memory_by_project ON memory (project) WHERE project IS NOT NULL;
         CREATE TABLE IF NOT EXISTS memory_ops (seq INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL,
             op TEXT NOT NULL, detail TEXT NOT NULL);",
    )
}

/// Importanza di un ricordo (0–1). Fattori e pesi sono dichiarati qui e documentati in ADR-0178.
pub fn importance_of(kind: &str, pinned: bool, uses: i64, age_days: f64, reliability: &str) -> f64 {
    let base = match kind {
        "procedural" => 0.7,
        "semantic" => 0.5,
        _ => 0.3,
    };
    let mut score = base;
    if pinned {
        score += 0.25;
    }
    score += (uses.clamp(0, 10) as f64) * 0.02;
    // L'età pesa poco e solo per ciò che non è fissato né una preferenza.
    if !pinned && kind != "procedural" {
        score -= (age_days / 365.0).min(0.2);
    }
    score += match reliability {
        "verified" => 0.1,
        "inferred" => -0.1,
        "uncertain" => -0.2,
        _ => 0.0,
    };
    (score.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

fn age_days(stamp: &str) -> f64 {
    crate::automation::parse_ts(stamp)
        .map(|t| (chrono::Utc::now() - t).num_hours().max(0) as f64 / 24.0)
        .unwrap_or(0.0)
}

fn op(store: &Store, op: &str, detail: &Value) -> StoreResult<()> {
    store.conn.execute(
        "INSERT INTO memory_ops (at, op, detail) VALUES (?1, ?2, ?3)",
        params![
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            op,
            detail.to_string()
        ],
    )?;
    Ok(())
}

/// Ricalcola l'importanza di ogni ricordo; gli episodi ancora «detti» diventano «dedotti» (li ha scritti il Core).
pub fn rescore(store: &Store) -> StoreResult<usize> {
    store.conn.execute(
        "UPDATE memory SET reliability = 'inferred' WHERE kind = 'episodic' AND reliability = 'stated'",
        [],
    )?;
    let rows: Vec<(String, String, Option<String>, i64, String, String)> = {
        let mut stmt = store.conn.prepare(
            "SELECT id, kind, source, uses, reliability, COALESCE(updated_at, created_at) FROM memory",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        rows.collect::<StoreResult<_>>()?
    };
    for (id, kind, source, uses, reliability, stamp) in &rows {
        let pinned = source
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .is_some_and(|s| s["pinned"] == true);
        let score = importance_of(kind, pinned, *uses, age_days(stamp), reliability);
        store.conn.execute(
            "UPDATE memory SET importance = ?2 WHERE id = ?1",
            params![id, score],
        )?;
    }
    Ok(rows.len())
}

/// Registra l'uso di ricordi entrati nel contesto: più sono usati, più pesano.
pub fn touch(store: &Store, ids: &[String]) -> StoreResult<()> {
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for id in ids {
        store.conn.execute(
            "UPDATE memory SET uses = uses + 1, last_used_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
    }
    Ok(())
}

/// Progetto di un ricordo, se ne ha uno.
pub fn project_of(store: &Store, id: &str) -> StoreResult<Option<String>> {
    Ok(store
        .conn
        .query_row("SELECT project FROM memory WHERE id = ?1", [id], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten())
}

/// Nome di progetto valido: lettere, cifre, trattini e spazi, fino a 40 caratteri.
pub fn valid_project(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.chars().count() <= 40
        && n.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
}

/// Imposta affidabilità e/o progetto (`""` toglie il progetto). Solo decisione del proprietario.
pub fn set_meta(
    store: &Store,
    id: &str,
    reliability: Option<&str>,
    project: Option<&str>,
) -> Result<bool, String> {
    let db = |e: rusqlite::Error| e.to_string();
    if let Some(r) = reliability {
        if !RELIABILITY.contains(&r) {
            return Err(format!(
                "affidabilità non valida ({})",
                RELIABILITY.join(", ")
            ));
        }
    }
    if let Some(p) = project {
        if !p.is_empty() && !valid_project(p) {
            return Err("nome di progetto non valido".into());
        }
    }
    let exists: bool = store
        .conn
        .query_row("SELECT 1 FROM memory WHERE id = ?1", [id], |_| Ok(true))
        .optional()
        .map_err(db)?
        .unwrap_or(false);
    if !exists {
        return Ok(false);
    }
    if let Some(r) = reliability {
        store
            .conn
            .execute(
                "UPDATE memory SET reliability = ?2 WHERE id = ?1",
                params![id, r],
            )
            .map_err(db)?;
    }
    if let Some(p) = project {
        let value = (!p.trim().is_empty()).then(|| p.trim().to_lowercase());
        store
            .conn
            .execute(
                "UPDATE memory SET project = ?2 WHERE id = ?1",
                params![id, value],
            )
            .map_err(db)?;
    }
    op(
        store,
        "meta",
        &json!({"id": id, "reliability": reliability, "project": project}),
    )
    .map_err(db)?;
    rescore(store).map_err(db)?;
    Ok(true)
}

/// Vista dei ricordi con importanza, affidabilità, uso e progetto, dal più importante.
pub fn view(
    store: &Store,
    kind: Option<&str>,
    project: Option<&str>,
    limit: usize,
) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT id, text, kind, source, importance, reliability, uses, last_used_at, project,
                created_at, expires_at FROM memory
         WHERE (?1 IS NULL OR kind = ?1) AND (?2 IS NULL OR project = ?2)
         ORDER BY importance DESC, created_at DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![kind, project.map(str::to_lowercase), limit as i64], |r| {
        let source: Option<String> = r.get(3)?;
        let source = source.and_then(|s| serde_json::from_str::<Value>(&s).ok());
        Ok(json!({
            "id": r.get::<_, String>(0)?, "text": r.get::<_, String>(1)?, "kind": r.get::<_, String>(2)?,
            "pinned": source.as_ref().is_some_and(|s| s["pinned"] == true),
            "provenance": source.unwrap_or(Value::Null),
            "importance": r.get::<_, f64>(4)?, "reliability": r.get::<_, String>(5)?,
            "uses": r.get::<_, i64>(6)?, "last_used_at": r.get::<_, Option<String>>(7)?,
            "project": r.get::<_, Option<String>>(8)?, "created_at": r.get::<_, String>(9)?,
            "expires_at": r.get::<_, Option<String>>(10)?,
        }))
    })?;
    rows.collect()
}

/// Progetti con il numero di ricordi.
pub fn projects(store: &Store) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT project, COUNT(*) FROM memory WHERE project IS NOT NULL GROUP BY project ORDER BY project",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({"project": r.get::<_, String>(0)?, "memories": r.get::<_, i64>(1)?}))
    })?;
    rows.collect()
}

/// Consolida: ricalcola le importanze e, per i quasi-doppioni, tiene il ricordo più importante (a parità il più
/// vecchio) e toglie l'altro. I ricordi fissati non si fondono mai. Con `apply` falso è solo un'anteprima.
pub fn consolidate(store: &Store, apply: bool) -> StoreResult<Value> {
    rescore(store)?;
    let mut merges = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    for pair in store.memory_duplicates(0.93, 50)? {
        let (a, b) = (
            pair["a"]["id"].as_str().unwrap_or_default().to_string(),
            pair["b"]["id"].as_str().unwrap_or_default().to_string(),
        );
        if removed.contains(&a) || removed.contains(&b) {
            continue;
        }
        let info = |id: &str| -> StoreResult<(f64, bool, String)> {
            store.conn.query_row(
                "SELECT importance, COALESCE(source, '') LIKE '%\"pinned\":true%', created_at FROM memory WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        };
        let (ia, pa, ca) = info(&a)?;
        let (ib, pb, cb) = info(&b)?;
        if pa || pb {
            continue;
        }
        let keep_a = ia > ib || (ia == ib && ca <= cb);
        let (keep, drop) = if keep_a { (a, b) } else { (b, a) };
        merges.push(json!({"keep": keep, "remove": drop, "similarity": pair["similarity"], "kind": pair["kind"]}));
        removed.push(drop);
    }
    if apply {
        for m in &merges {
            let drop = m["remove"].as_str().unwrap_or_default();
            let keep = m["keep"].as_str().unwrap_or_default();
            // Gli usi del ricordo tolto passano a quello tenuto.
            let uses: i64 = store
                .conn
                .query_row("SELECT uses FROM memory WHERE id = ?1", [drop], |r| {
                    r.get(0)
                })
                .unwrap_or(0);
            store.conn.execute(
                "UPDATE memory SET uses = uses + ?2 WHERE id = ?1",
                params![keep, uses],
            )?;
            store.memory_delete(drop)?;
        }
        if !merges.is_empty() {
            op(store, "consolidate", &json!({"merged": merges}))?;
            rescore(store)?;
        }
    }
    Ok(json!({"applied": apply, "merges": merges, "memories": store.memory_list()?.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::NewItem;
    use std::path::Path;

    fn store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    fn add(s: &Store, id: &str, text: &str, kind: &str) {
        s.memory_insert(
            &NewItem {
                id,
                text,
                kind,
                source: json!({"kind": "owner"}),
                expires_at: None,
            },
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
    }

    #[test]
    fn importance_follows_kind_pin_use_age_and_reliability() {
        let base = importance_of("semantic", false, 0, 0.0, "stated");
        assert_eq!(base, 0.5);
        assert!(importance_of("semantic", true, 0, 0.0, "stated") > base);
        assert!(importance_of("semantic", false, 5, 0.0, "stated") > base);
        assert!(importance_of("semantic", false, 0, 400.0, "stated") < base);
        assert!(importance_of("procedural", false, 0, 400.0, "stated") > base);
        assert!(importance_of("semantic", false, 0, 0.0, "verified") > base);
        assert!(
            importance_of("semantic", false, 0, 0.0, "uncertain")
                < importance_of("semantic", false, 0, 0.0, "inferred")
        );
        assert!(importance_of("episodic", false, 0, 0.0, "inferred") >= 0.0);
        assert!(importance_of("procedural", true, 10, 0.0, "verified") <= 1.0);
    }

    #[test]
    fn rescoring_marks_episodes_as_inferred_and_use_raises_importance() {
        let s = store();
        add(&s, "a", "il mio cane si chiama Fido", "semantic");
        add(&s, "e", "ho controllato il disco", "episodic");
        rescore(&s).unwrap();
        let items = view(&s, None, None, 10).unwrap();
        let e = items.iter().find(|i| i["id"] == "e").unwrap();
        assert_eq!(e["reliability"], "inferred");
        let before = items.iter().find(|i| i["id"] == "a").unwrap()["importance"]
            .as_f64()
            .unwrap();
        touch(&s, &["a".into()]).unwrap();
        touch(&s, &["a".into()]).unwrap();
        rescore(&s).unwrap();
        let after = view(&s, None, None, 10).unwrap();
        let a = after.iter().find(|i| i["id"] == "a").unwrap();
        assert!(a["importance"].as_f64().unwrap() > before);
        assert_eq!(a["uses"], 2);
        assert!(a["last_used_at"].is_string());
        // L'ordine è per importanza.
        assert_eq!(after[0]["id"], "a");
    }

    #[test]
    fn reliability_and_project_are_validated_and_filterable() {
        let s = store();
        add(&s, "a", "il server di Alfa è a Milano", "semantic");
        assert!(set_meta(&s, "a", Some("sicurissimo"), None).is_err());
        assert!(set_meta(&s, "a", None, Some("../x/..;")).is_err());
        assert!(!set_meta(&s, "nope", Some("verified"), None).unwrap());
        assert!(set_meta(&s, "a", Some("verified"), Some("Alfa")).unwrap());
        assert_eq!(view(&s, None, Some("alfa"), 10).unwrap().len(), 1);
        assert_eq!(view(&s, None, Some("beta"), 10).unwrap().len(), 0);
        assert_eq!(
            view(&s, None, None, 10).unwrap()[0]["reliability"],
            "verified"
        );
        assert_eq!(project_of(&s, "a").unwrap().as_deref(), Some("alfa"));
        assert_eq!(projects(&s).unwrap()[0]["memories"], 1);
        // Togliere il progetto.
        set_meta(&s, "a", None, Some("")).unwrap();
        assert!(project_of(&s, "a").unwrap().is_none());
    }

    #[test]
    fn consolidation_previews_then_merges_near_duplicates_keeping_the_used_one_and_never_pinned() {
        let s = store();
        add(&s, "a", "il mio cane si chiama Fido", "semantic");
        add(&s, "b", "Il mio cane si chiama Fido.", "semantic");
        add(&s, "x", "la riunione di progetto è alle nove", "semantic");
        touch(&s, &["b".into()]).unwrap();
        let preview = consolidate(&s, false).unwrap();
        assert_eq!(preview["applied"], false);
        assert_eq!(
            s.memory_list().unwrap().len(),
            3,
            "l'anteprima non cambia nulla"
        );
        let merges = preview["merges"].as_array().unwrap();
        assert_eq!(merges.len(), 1, "{preview}");
        assert_eq!(merges[0]["keep"], "b", "il più usato resta: {preview}");
        assert_eq!(merges[0]["remove"], "a");
        let done = consolidate(&s, true).unwrap();
        assert_eq!(done["applied"], true);
        assert_eq!(s.memory_list().unwrap().len(), 2);
        // Un ricordo fissato non si fonde mai.
        let t = store();
        add(&t, "p", "il mio cane si chiama Fido", "semantic");
        add(&t, "q", "il mio cane si chiama Fido", "semantic");
        t.memory_pin("p", true).unwrap();
        consolidate(&t, true).unwrap();
        assert_eq!(t.memory_list().unwrap().len(), 2);
    }
}
