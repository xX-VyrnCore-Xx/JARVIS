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
    // Scadenza adattiva: un episodio che viene davvero usato (importanza almeno 0,3) vive almeno 90 giorni.
    let keep_until = (chrono::Utc::now() + chrono::Duration::days(90))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    store.conn.execute(
        "UPDATE memory SET expires_at = ?1 WHERE kind = 'episodic' AND importance >= 0.3
         AND expires_at IS NOT NULL AND expires_at < ?1",
        params![keep_until],
    )?;
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

// ---------------------------------------------------------------------------------------- portabilità e cura

pub const EXPORT_CONTRACT: &str = "jarvis.memory-export/1";
const MAX_IMPORT: usize = 2000;

/// Esporta la memoria (testo, tipo, provenienza, importanza, affidabilità, progetto, scadenza). Gli identificativi
/// non si esportano: si rigenerano all'importazione. Non esce nulla che sembri una credenziale.
pub fn export(store: &Store) -> StoreResult<Value> {
    rescore(store)?;
    let items: Vec<Value> = view(store, None, None, 100_000)?
        .into_iter()
        .filter(|i| !crate::cloud::contains_credentials(i["text"].as_str().unwrap_or_default()))
        .map(|i| {
            json!({
                "text": i["text"], "kind": i["kind"], "provenance": i["provenance"], "importance": i["importance"],
                "reliability": i["reliability"], "uses": i["uses"], "project": i["project"],
                "created_at": i["created_at"], "expires_at": i["expires_at"],
            })
        })
        .collect();
    Ok(json!({"contract": EXPORT_CONTRACT, "exported_at": crate::now(), "items": items}))
}

/// Importa un'esportazione: valida ogni voce, salta i doppioni (stesso testo normalizzato) e rifiuta credenziali e
/// voci malformate indicandone il motivo. Una voce «verificata» arriva come «detta»: la verifica è del proprietario
/// su questo computer; una di origine ignota arriva come «da ricontrollare». Con `apply` falso è un'anteprima.
pub fn import(store: &Store, doc: &Value, apply: bool) -> Result<Value, String> {
    if doc["contract"] != EXPORT_CONTRACT {
        return Err(format!(
            "formato non riconosciuto (serve {EXPORT_CONTRACT})"
        ));
    }
    let items = doc["items"]
        .as_array()
        .ok_or("«items» deve essere un elenco")?;
    if items.len() > MAX_IMPORT {
        return Err(format!("al massimo {MAX_IMPORT} voci per importazione"));
    }
    let (mut imported, mut duplicates) = (0usize, 0usize);
    let mut rejected: Vec<Value> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let text = item["text"].as_str().unwrap_or_default().trim();
        let kind = item["kind"].as_str().unwrap_or("semantic");
        let reject = |reason: &str| json!({"index": index, "reason": reason});
        if text.is_empty() || text.chars().count() > 1000 {
            rejected.push(reject("testo vuoto o oltre i 1000 caratteri"));
            continue;
        }
        if !crate::memory::KINDS.contains(&kind) {
            rejected.push(reject("tipo sconosciuto"));
            continue;
        }
        if crate::cloud::contains_credentials(text) {
            rejected.push(reject("sembra contenere una credenziale"));
            continue;
        }
        let project = item["project"].as_str().filter(|p| !p.is_empty());
        if project.is_some_and(|p| !valid_project(p)) {
            rejected.push(reject("nome di progetto non valido"));
            continue;
        }
        if store
            .memory_find_same(text)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            duplicates += 1;
            continue;
        }
        imported += 1;
        if !apply {
            continue;
        }
        let reliability = match item["reliability"].as_str() {
            Some("stated") | Some("verified") => "stated",
            Some("inferred") => "inferred",
            _ => "uncertain",
        };
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        store
            .memory_insert(
                &crate::memory::NewItem {
                    id: &id,
                    text,
                    kind,
                    source: json!({"kind": "import", "exported_at": doc["exported_at"], "original": item["provenance"]["kind"]}),
                    expires_at: item["expires_at"].as_str().map(str::to_string),
                },
                &crate::now(),
            )
            .map_err(|e| e.to_string())?;
        set_meta(store, &id, Some(reliability), project)?;
    }
    if apply && imported > 0 {
        op(
            store,
            "import",
            &json!({"imported": imported, "duplicates": duplicates}),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(
        json!({"applied": apply, "imported": imported, "duplicates": duplicates, "rejected": rejected}),
    )
}

/// Risolve una contraddizione: tiene la voce scelta dal proprietario (diventa «verificata») ed elimina le altre della
/// stessa coppia soggetto-relazione. L'operazione resta nel registro.
pub fn resolve_contradiction(
    store: &Store,
    subject: &str,
    relation: &str,
    keep: &str,
) -> Result<Value, String> {
    let db = |e: rusqlite::Error| e.to_string();
    let mut stmt = store
        .conn
        .prepare("SELECT DISTINCT memory_id FROM memory_edges WHERE subject = ?1 AND relation = ?2")
        .map_err(db)?;
    let group: Vec<String> = stmt
        .query_map(params![subject, relation], |r| r.get(0))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;
    if group.len() < 2 {
        return Err("nessuna contraddizione per questa coppia".into());
    }
    if !group.iter().any(|id| id == keep) {
        return Err("la voce da tenere non fa parte della contraddizione".into());
    }
    let pinned_others: Vec<&String> = group
        .iter()
        .filter(|id| id.as_str() != keep)
        .filter(|id| {
            store
                .conn
                .query_row(
                    "SELECT COALESCE(source, '') LIKE '%\"pinned\":true%' FROM memory WHERE id = ?1",
                    [id],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
        })
        .collect();
    if !pinned_others.is_empty() {
        return Err("una delle altre voci è fissata: liberala prima di eliminarla".into());
    }
    let removed: Vec<&String> = group.iter().filter(|id| id.as_str() != keep).collect();
    for id in &removed {
        store.memory_delete(id).map_err(db)?;
    }
    set_meta(store, keep, Some("verified"), None)?;
    op(
        store,
        "resolve",
        &json!({"subject": subject, "relation": relation, "kept": keep, "removed": removed}),
    )
    .map_err(db)?;
    Ok(json!({"kept": keep, "removed": removed}))
}

/// Riassume gli episodi più vecchi di `days` giorni in un riassunto per settimana (al massimo 12 punti da 120
/// caratteri), eliminando gli originali non fissati. Il riassunto è «dedotto», ricorda gli identificativi di
/// origine in `source` e vive 180 giorni. Con `apply` falso è un'anteprima.
pub fn compact_episodes(store: &Store, days: i64, apply: bool) -> StoreResult<Value> {
    let limit = (chrono::Utc::now() - chrono::Duration::days(days.clamp(7, 3650)))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut stmt = store.conn.prepare(
        "SELECT id, text, created_at FROM memory
         WHERE kind = 'episodic' AND created_at < ?1 AND COALESCE(source, '') NOT LIKE '%\"pinned\":true%'
           AND COALESCE(source, '') NOT LIKE '%\"kind\":\"consolidation\"%'
         ORDER BY created_at",
    )?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([&limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut weeks: std::collections::BTreeMap<String, Vec<(String, String)>> = Default::default();
    for (id, text, created) in rows {
        let week = crate::automation::parse_ts(&created)
            .map(|t| t.format("%G-W%V").to_string())
            .unwrap_or_else(|| "senza-data".into());
        weeks.entry(week).or_default().push((id, text));
    }
    let mut groups = Vec::new();
    for (week, entries) in &weeks {
        if entries.len() < 2 {
            continue;
        }
        let bullets: Vec<String> = entries
            .iter()
            .take(12)
            .map(|(_, t)| {
                format!(
                    "• {}",
                    t.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(120)
                        .collect::<String>()
                )
            })
            .collect();
        let summary = format!(
            "Settimana {week}: {} attività.\n{}",
            entries.len(),
            bullets.join("\n")
        );
        let ids: Vec<&String> = entries.iter().map(|(id, _)| id).collect();
        if apply {
            let new_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
            let expires = (chrono::Utc::now() + chrono::Duration::days(180))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            store.memory_insert(
                &crate::memory::NewItem {
                    id: &new_id,
                    text: &summary.chars().take(1000).collect::<String>(),
                    kind: "episodic",
                    source: json!({"kind": "consolidation", "from": ids}),
                    expires_at: Some(expires),
                },
                &crate::now(),
            )?;
            for id in &ids {
                store.memory_delete(id)?;
            }
        }
        groups.push(json!({"week": week, "episodes": entries.len(), "summary": summary}));
    }
    if apply && !groups.is_empty() {
        op(
            store,
            "compact",
            &json!({"days": days, "weeks": groups.len()}),
        )?;
        rescore(store)?;
    }
    Ok(json!({"applied": apply, "groups": groups}))
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

    #[test]
    fn export_then_import_round_trips_without_duplicates_and_rejects_bad_entries() {
        let a = store();
        add(&a, "1", "il mio cane si chiama Fido", "semantic");
        add(&a, "2", "preferisco il tè verde", "procedural");
        set_meta(&a, "1", Some("verified"), Some("casa")).unwrap();
        let doc = export(&a).unwrap();
        assert_eq!(doc["contract"], EXPORT_CONTRACT);
        assert_eq!(doc["items"].as_array().unwrap().len(), 2);
        assert!(
            !doc.to_string().contains("\"id\""),
            "gli identificativi non si esportano"
        );
        let b = store();
        let preview = import(&b, &doc, false).unwrap();
        assert_eq!(preview["imported"], 2);
        assert_eq!(b.memory_list().unwrap().len(), 0, "l'anteprima non scrive");
        let done = import(&b, &doc, true).unwrap();
        assert_eq!(done["imported"], 2);
        let items = view(&b, None, None, 10).unwrap();
        let fido = items
            .iter()
            .find(|i| i["text"].as_str().unwrap().contains("Fido"))
            .unwrap();
        assert_eq!(
            fido["reliability"], "stated",
            "«verified» non si importa come tale"
        );
        assert_eq!(fido["project"], "casa");
        assert_eq!(fido["provenance"]["kind"], "import");
        // Una seconda importazione non crea doppioni.
        let again = import(&b, &doc, true).unwrap();
        assert_eq!(
            (again["imported"].as_u64(), again["duplicates"].as_u64()),
            (Some(0), Some(2))
        );
        let bad = json!({"contract": EXPORT_CONTRACT, "items": [
            {"text": "chiave sk-abcdefghijklmnopqrstuvwxyz0123456789"}, {"text": ""}, {"text": "x", "kind": "strano"},
            {"text": "ok", "project": "../x"}, {"text": "una voce valida da importare"}]});
        let out = import(&b, &bad, true).unwrap();
        assert_eq!(out["rejected"].as_array().unwrap().len(), 4, "{out}");
        assert_eq!(out["imported"], 1);
        let unknown = view(&b, None, None, 10).unwrap();
        assert_eq!(
            unknown
                .iter()
                .find(|i| i["text"] == "una voce valida da importare")
                .unwrap()["reliability"],
            "uncertain"
        );
        assert!(import(&b, &json!({"contract": "altro"}), true).is_err());
    }

    #[test]
    fn a_contradiction_is_resolved_by_keeping_the_chosen_entry_and_marking_it_verified() {
        let s = store();
        add(&s, "a", "il mio cane si chiama Fido", "semantic");
        add(&s, "b", "il mio cane si chiama Rex", "semantic");
        assert!(resolve_contradiction(&s, "mio cane", "si chiama", "zzz").is_err());
        let found = crate::reflect::memory_contradictions(&s).unwrap();
        let (subject, relation) = (
            found[0]["subject"].as_str().unwrap(),
            found[0]["relation"].as_str().unwrap(),
        );
        let out = resolve_contradiction(&s, subject, relation, "b").unwrap();
        assert_eq!(out["kept"], "b");
        let left = view(&s, None, None, 10).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0]["reliability"], "verified");
        assert!(crate::reflect::memory_contradictions(&s)
            .unwrap()
            .is_empty());
        assert!(
            resolve_contradiction(&s, subject, relation, "b").is_err(),
            "non c'è più contraddizione"
        );
    }

    #[test]
    fn old_episodes_are_summarised_per_week_and_pinned_ones_are_left_alone() {
        let s = store();
        for (id, text, day) in [
            ("e1", "ho controllato il disco", "2026-01-05T10:00:00Z"),
            ("e2", "ho aggiornato il server", "2026-01-06T10:00:00Z"),
            ("e3", "ho riavviato il router", "2026-01-07T10:00:00Z"),
            (
                "e4",
                "episodio di un'altra settimana",
                "2026-03-02T10:00:00Z",
            ),
        ] {
            s.memory_insert(
                &crate::memory::NewItem {
                    id,
                    text,
                    kind: "episodic",
                    source: json!({"kind": "episode"}),
                    expires_at: None,
                },
                day,
            )
            .unwrap();
        }
        s.memory_pin("e3", true).unwrap();
        let preview = compact_episodes(&s, 30, false).unwrap();
        assert_eq!(
            s.memory_list().unwrap().len(),
            4,
            "l'anteprima non cambia nulla"
        );
        let groups = preview["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "{preview}");
        assert_eq!(
            groups[0]["episodes"], 2,
            "il fissato e l'episodio solitario restano"
        );
        let done = compact_episodes(&s, 30, true).unwrap();
        assert_eq!(done["applied"], true);
        let left: Vec<String> = s
            .memory_list()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect();
        assert!(left.contains(&"e3".to_string()) && left.contains(&"e4".to_string()));
        assert!(!left.contains(&"e1".to_string()) && !left.contains(&"e2".to_string()));
        let summary = view(&s, Some("episodic"), None, 10).unwrap();
        let sum = summary
            .iter()
            .find(|i| i["provenance"]["kind"] == "consolidation")
            .unwrap();
        assert!(
            sum["text"].as_str().unwrap().contains("disco")
                && sum["text"].as_str().unwrap().contains("server")
        );
        assert_eq!(sum["reliability"], "inferred");
        assert!(sum["expires_at"].is_string());
        // Un riassunto non si riassume di nuovo.
        assert_eq!(
            compact_episodes(&s, 30, true).unwrap()["groups"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn episodes_that_are_really_used_live_longer() {
        let s = store();
        let soon = (chrono::Utc::now() + chrono::Duration::days(3))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        for id in ["usato", "ignorato"] {
            s.memory_insert(
                &crate::memory::NewItem {
                    id,
                    text: id,
                    kind: "episodic",
                    source: json!({"kind": "episode"}),
                    expires_at: Some(soon.clone()),
                },
                &crate::now(),
            )
            .unwrap();
        }
        for _ in 0..6 {
            touch(&s, &["usato".into()]).unwrap();
        }
        rescore(&s).unwrap();
        let items = view(&s, Some("episodic"), None, 10).unwrap();
        let expiry = |id: &str| {
            items.iter().find(|i| i["id"] == id).unwrap()["expires_at"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert!(
            expiry("usato") > soon,
            "l'episodio usato è stato prolungato"
        );
        assert_eq!(
            expiry("ignorato"),
            soon,
            "quello ignorato scade come previsto"
        );
    }
}
