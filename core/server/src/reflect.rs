//! Autocorrezione e riflessione (ADR-0179): classificazione dei guasti, causa probabile, strategie alternative,
//! verifica del risultato con **prove osservabili** e rilevamento di contraddizioni.
//!
//! Il Core non si fida del solo «ho fatto» di uno strumento o del modello: dopo un'operazione che modifica
//! qualcosa rilegge lo stato (il file esiste ed ha la dimensione scritta, la cartella c'è, il ricordo è stato
//! salvato) e, se la prova manca, il passo risulta fallito (`plan.unverified`). I nuovi tentativi sono limitati:
//! solo i guasti passeggeri si ripetono; un rifiuto della policy, un argomento sbagliato o una risorsa assente non
//! migliorano riprovando e vanno al recupero (ripianificazione) con una strategia diversa.

use std::path::Path;

use serde_json::{json, Value};

use crate::store::Store;

/// Esito della classificazione di un errore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    /// `authorization`, `scope`, `transient`, `precondition`, `input`, `verification`, `internal`, `unknown`.
    pub class: &'static str,
    /// Ha senso riprovare con gli stessi argomenti.
    pub retryable: bool,
    pub cause: &'static str,
    pub strategies: Vec<&'static str>,
}

/// Classifica un errore di strumento o di piano dal codice e dal messaggio.
pub fn classify(code: &str, message: &str) -> Diagnosis {
    let c = code.to_lowercase();
    let m = message.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| c.contains(n) || m.contains(n));
    if has(&[
        "outside_workspace",
        "fuori dalla workspace",
        "percorso non ammesso",
    ]) {
        return Diagnosis {
            class: "scope",
            retryable: false,
            cause: "il percorso è fuori dalla workspace consentita",
            strategies: vec![
                "usa un percorso dentro la workspace",
                "importa il file nella workspace",
            ],
        };
    }
    if c.starts_with("policy.")
        || c == "plan.constraint"
        || has(&["owner_rule", "non autorizzat", "vietat"])
    {
        return Diagnosis {
            class: "authorization",
            retryable: false,
            cause: "la policy, un vincolo o un livello di autonomia non consente l'operazione",
            strategies: vec![
                "usa uno strumento di sola lettura equivalente",
                "chiedi al proprietario la conferma o di cambiare profilo, vincolo o livello",
                "riduci l'operazione a ciò che è consentito",
            ],
        };
    }
    if c.contains("unverified") || c.contains("expectation") {
        return Diagnosis {
            class: "verification",
            retryable: false,
            cause: "il passo dichiara di aver finito ma la prova osservabile non c'è",
            strategies: vec![
                "rileggi lo stato con uno strumento di lettura prima di proseguire",
                "ripianifica il passo con parametri diversi",
            ],
        };
    }
    if has(&[
        "timeout",
        "timed out",
        "scadut",
        "unavailable",
        "non disponibil",
        "busy",
        "rate",
        "connessione",
        "network",
        "temporar",
    ]) {
        return Diagnosis {
            class: "transient",
            retryable: true,
            cause: "guasto passeggero (tempo scaduto, servizio occupato o non raggiungibile)",
            strategies: vec![
                "riprova dopo una breve attesa",
                "usa una fonte o un percorso alternativo",
            ],
        };
    }
    if has(&[
        "not_found",
        "non trovat",
        "inesistent",
        "does not exist",
        "missing",
        "manca",
    ]) {
        return Diagnosis {
            class: "precondition",
            // Può comparire più tardi (un altro passo la crea): un nuovo tentativo ha senso.
            retryable: true,
            cause: "manca una risorsa da cui il passo dipende",
            strategies: vec![
                "verifica che la risorsa esista (elenco o ricerca dei file)",
                "crea ciò che manca prima di riprovare",
            ],
        };
    }
    if has(&[
        "invalid",
        "non valid",
        "bad_request",
        "bad_reference",
        "schema",
        "sconosciut",
        "argoment",
    ]) {
        return Diagnosis {
            class: "input",
            retryable: false,
            cause: "argomenti non validi o riferimento a un risultato che non esiste",
            strategies: vec![
                "correggi gli argomenti",
                "ricontrolla il risultato del passo precedente",
            ],
        };
    }
    if has(&["panic", "internal", "interno"]) {
        return Diagnosis {
            class: "internal",
            retryable: true,
            cause: "errore interno dello strumento",
            strategies: vec![
                "riprova una volta",
                "segnala il guasto e usa uno strumento alternativo",
            ],
        };
    }
    Diagnosis {
        class: "unknown",
        retryable: false,
        cause: "causa non riconosciuta",
        strategies: vec![
            "leggi il messaggio d'errore",
            "chiedi un piano di recupero (ripianificazione)",
        ],
    }
}

impl Diagnosis {
    /// Riprovare con gli stessi argomenti non può cambiare l'esito (rifiuto, argomento errato, percorso fuori workspace…).
    pub fn futile_to_retry(&self) -> bool {
        matches!(
            self.class,
            "authorization" | "scope" | "input" | "verification"
        )
    }
}

/// Aggiunge classe, causa e strategie a un errore di passo, senza toccare codice e messaggio originali.
pub fn annotate(error: &mut Value) {
    let d = classify(
        error["code"].as_str().unwrap_or_default(),
        error["message"].as_str().unwrap_or_default(),
    );
    if let Some(obj) = error.as_object_mut() {
        obj.insert("class".into(), json!(d.class));
        obj.insert("retryable".into(), json!(d.retryable));
        obj.insert("cause".into(), json!(d.cause));
        obj.insert("strategies".into(), json!(d.strategies));
    }
}

/// Prova osservabile che un'operazione ha avuto l'effetto dichiarato. `None`: nessuna prova prevista per lo
/// strumento. `Some(Err)`: l'effetto non è stato osservato.
pub fn evidence(
    tool: &str,
    args: &Value,
    output: &Value,
    workspace: &Path,
    store: &Store,
) -> Option<Result<(), String>> {
    let path = |key: &str| {
        crate::tools::resolve(workspace, args[key].as_str().unwrap_or_default(), false).ok()
    };
    match tool {
        "fs.write" => {
            let file = path("path")?;
            let expected = output["bytes_written"].as_u64()?;
            Some(match std::fs::metadata(&file) {
                Ok(meta) if meta.len() == expected => Ok(()),
                Ok(meta) => Err(format!(
                    "il file ha {} byte invece di {expected}",
                    meta.len()
                )),
                Err(_) => Err("il file scritto non esiste".into()),
            })
        }
        "fs.mkdir" => {
            let dir = path("path")?;
            Some(if dir.is_dir() {
                Ok(())
            } else {
                Err("la cartella non esiste dopo la creazione".into())
            })
        }
        "fs.delete" => {
            let file = path("path")?;
            Some(if file.exists() {
                Err("l'elemento esiste ancora dopo l'eliminazione".into())
            } else {
                Ok(())
            })
        }
        "fs.move" => {
            let (from, to) = (path("from")?, path("to")?);
            Some(if to.exists() && !from.exists() {
                Ok(())
            } else {
                Err(
                    "lo spostamento non risulta: destinazione assente o origine ancora presente"
                        .into(),
                )
            })
        }
        "memory.remember" => {
            let id = output["id"].as_str()?;
            Some(match store.memory_get(id) {
                Ok(Some(_)) => Ok(()),
                _ => Err("il ricordo non risulta salvato".into()),
            })
        }
        _ => None,
    }
}

/// Contraddizioni nella memoria: stessa coppia (soggetto, relazione) con oggetti diversi, ricavate dal grafo.
pub fn memory_contradictions(store: &Store) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT e.subject, e.relation, GROUP_CONCAT(DISTINCT e.object), COUNT(DISTINCT e.object)
         FROM memory_edges e JOIN memory m ON m.id = e.memory_id
         GROUP BY e.subject, e.relation HAVING COUNT(DISTINCT e.object) > 1
         ORDER BY e.subject, e.relation LIMIT 50",
    )?;
    let rows = stmt.query_map([], |r| {
        let objects: String = r.get(2)?;
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, objects))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (subject, relation, objects) = row?;
        let mut ids_stmt = store.conn.prepare(
            "SELECT DISTINCT memory_id FROM memory_edges WHERE subject = ?1 AND relation = ?2",
        )?;
        let ids: Vec<String> = ids_stmt
            .query_map(rusqlite::params![subject, relation], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        out.push(json!({
            "subject": subject, "relation": relation,
            "values": objects.split(',').collect::<Vec<_>>(), "memory_ids": ids,
            "suggestion": "controlla quale è corretta e correggi o dimentica l'altra",
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_classified_and_only_transient_ones_are_worth_retrying() {
        let cases = [
            ("policy.owner_rule_deny", "", "authorization", false),
            (
                "plan.constraint",
                "vincolo «solo lettura»",
                "authorization",
                false,
            ),
            (
                "fs.outside_workspace",
                "percorso non ammesso",
                "scope",
                false,
            ),
            ("plan.unverified", "", "verification", false),
            ("web.timeout", "tempo scaduto", "transient", true),
            ("fs.not_found", "file inesistente", "precondition", true),
            (
                "plan.bad_reference",
                "riferimento sconosciuto",
                "input",
                false,
            ),
            (
                "plan.worker_panic",
                "esecuzione interrotta",
                "internal",
                true,
            ),
            ("strano", "boh", "unknown", false),
        ];
        for (code, message, class, retry) in cases {
            let d = classify(code, message);
            assert_eq!((d.class, d.retryable), (class, retry), "{code}");
            assert!(!d.strategies.is_empty() && !d.cause.is_empty());
        }
    }

    #[test]
    fn annotate_keeps_the_original_error_and_adds_the_diagnosis() {
        let mut e = json!({"code": "web.timeout", "message": "scaduto"});
        annotate(&mut e);
        assert_eq!(e["code"], "web.timeout");
        assert_eq!(e["class"], "transient");
        assert_eq!(e["retryable"], true);
        assert!(e["strategies"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn evidence_is_read_back_from_the_system_and_a_missing_effect_is_caught() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-evid-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(Path::new(":memory:")).unwrap();
        std::fs::write(dir.join("a.txt"), "abc").unwrap();
        let ok = evidence(
            "fs.write",
            &json!({"path": "a.txt"}),
            &json!({"bytes_written": 3}),
            &dir,
            &store,
        );
        assert_eq!(ok, Some(Ok(())));
        let wrong = evidence(
            "fs.write",
            &json!({"path": "a.txt"}),
            &json!({"bytes_written": 9}),
            &dir,
            &store,
        );
        assert!(matches!(wrong, Some(Err(_))));
        let missing = evidence(
            "fs.write",
            &json!({"path": "b.txt"}),
            &json!({"bytes_written": 1}),
            &dir,
            &store,
        );
        assert!(matches!(missing, Some(Err(_))));
        assert_eq!(
            evidence(
                "fs.delete",
                &json!({"path": "a.txt"}),
                &json!({}),
                &dir,
                &store
            ),
            Some(Err("l'elemento esiste ancora dopo l'eliminazione".into()))
        );
        assert!(evidence("time.now", &json!({}), &json!({}), &dir, &store).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contradictions_in_memory_are_found_from_the_graph() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        for (id, text) in [
            ("a", "il mio cane si chiama Fido"),
            ("b", "il mio cane si chiama Rex"),
        ] {
            store
                .memory_insert(
                    &crate::memory::NewItem {
                        id,
                        text,
                        kind: "semantic",
                        source: json!({"kind": "owner"}),
                        expires_at: None,
                    },
                    "2026-01-01T00:00:00Z",
                )
                .unwrap();
        }
        let found = memory_contradictions(&store).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["values"].as_array().unwrap().len(), 2);
    }
}
