//! Sessioni di ricerca persistenti (ADR-0206): una ricerca interrotta si riprende, con quanto trovato, le fonti (con
//! attendibilità) e le domande ancora aperte. Dati locali del proprietario: non escono dal computer e non contengono
//! credenziali. Le informazioni sono distinte per tipo (verificato, non confermato, ipotesi…), come chiede la ricerca
//! strutturata: JARVIS non presenta come risultato ciò che non ha trovato.

use serde_json::{json, Value};

use crate::store::Store;
use crate::tools::ToolError;

const MAX_SESSIONS: i64 = 60;
const MAX_FINDINGS: usize = 40;
const MAX_SOURCES: usize = 40;
const MAX_QUESTIONS: usize = 15;
const KINDS: [&str; 6] = [
    "verificato",
    "non_confermato",
    "opinione",
    "ipotesi",
    "inferenza",
    "obsoleto",
];

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn clip(value: &Value, max: usize) -> String {
    value
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(max)
        .collect()
}

fn texts(value: &Value, max_items: usize, max_len: usize) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| clip(v, max_len))
        .filter(|t| !t.is_empty())
        .take(max_items)
        .collect()
}

/// Crea o aggiorna una sessione. Senza `id` ne nasce una nuova (serve `title`); con `id` si aggiunge a quella
/// esistente: risultati e fonti si accumulano senza doppioni, le domande aperte si sostituiscono.
pub fn save(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let storage = |e: rusqlite::Error| err("research.unavailable", e.to_string());
    let id_arg = args["id"].as_str().map(str::trim).filter(|i| !i.is_empty());
    let mut doc = match id_arg {
        Some(id) => store
            .research_get(id)
            .map_err(storage)?
            .ok_or_else(|| err("research.not_found", "Sessione di ricerca non trovata"))?,
        None => {
            let title = clip(&args["title"], 120);
            if title.is_empty() {
                return Err(err(
                    "research.invalid",
                    "Indica un titolo per la nuova ricerca",
                ));
            }
            if store.research_count().map_err(storage)? >= MAX_SESSIONS {
                return Err(err(
                    "research.full",
                    format!("Ci sono già {MAX_SESSIONS} ricerche salvate: eliminane qualcuna"),
                ));
            }
            json!({"id": uuid::Uuid::new_v4().simple().to_string()[..8], "title": title,
                "created_at": crate::now(), "status": "open", "findings": [], "sources": [], "questions": []})
        }
    };
    if let Some(query) = args["query"].as_str().filter(|q| !q.trim().is_empty()) {
        doc["query"] = json!(clip(&json!(query), 300));
    }
    if let Some(status) = args["status"].as_str() {
        if !matches!(status, "open" | "done") {
            return Err(err("research.invalid", "Stato non valido: open o done"));
        }
        doc["status"] = json!(status);
    }
    let all_text = |v: &Value| v.to_string();
    if crate::cloud::contains_credentials(&all_text(&args["findings"]))
        || crate::cloud::contains_credentials(&all_text(&args["questions"]))
        || crate::cloud::contains_credentials(&all_text(&args["sources"]))
    {
        return Err(err(
            "research.sensitive",
            "La ricerca contiene credenziali: non si salva",
        ));
    }
    let mut findings = doc["findings"].as_array().cloned().unwrap_or_default();
    for f in args["findings"].as_array().into_iter().flatten() {
        let (text, kind) = match f {
            Value::String(t) => (clip(&json!(t), 1000), "non_confermato".to_string()),
            other => (clip(&other["text"], 1000), clip(&other["kind"], 20)),
        };
        if text.is_empty() {
            continue;
        }
        let kind = if KINDS.contains(&kind.as_str()) {
            kind
        } else {
            "non_confermato".to_string()
        };
        if !findings.iter().any(|x| x["text"] == text.as_str()) {
            findings.push(json!({"text": text, "kind": kind, "at": crate::now()}));
        }
    }
    findings.truncate(MAX_FINDINGS);
    doc["findings"] = json!(findings);
    let mut sources = doc["sources"].as_array().cloned().unwrap_or_default();
    for s in args["sources"].as_array().into_iter().flatten() {
        let url = clip(&s["url"], 500);
        if !(url.starts_with("https://") || url.starts_with("http://"))
            || sources.iter().any(|x| x["url"] == url.as_str())
        {
            continue;
        }
        let reliability = crate::web::assess_source(&url);
        sources.push(
            json!({"url": url, "title": clip(&s["title"], 200), "reliability": reliability["level"],
            "retrieved_at": crate::now()}),
        );
    }
    sources.truncate(MAX_SOURCES);
    doc["sources"] = json!(sources);
    if args.get("questions").is_some() {
        doc["questions"] = json!(texts(&args["questions"], MAX_QUESTIONS, 300));
    }
    doc["updated_at"] = json!(crate::now());
    store.research_put(&doc).map_err(storage)?;
    Ok(summary(&doc, true))
}

fn summary(doc: &Value, full: bool) -> Value {
    let count = |key: &str| doc[key].as_array().map_or(0, Vec::len);
    let mut out = json!({"id": doc["id"], "title": doc["title"], "query": doc["query"], "status": doc["status"],
        "updated_at": doc["updated_at"], "findings": count("findings"), "sources": count("sources"),
        "open_questions": count("questions")});
    if full {
        out["session"] = doc.clone();
    }
    out
}

pub fn list(store: &Store) -> Result<Value, ToolError> {
    let items = store
        .research_list()
        .map_err(|e| err("research.unavailable", e.to_string()))?;
    Ok(json!({"items": items.iter().map(|d| summary(d, false)).collect::<Vec<_>>()}))
}

/// Legge una sessione per `id` o per parte del titolo (la più recente che corrisponde).
pub fn read(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let storage = |e: rusqlite::Error| err("research.unavailable", e.to_string());
    let doc = if let Some(id) = args["id"].as_str().filter(|i| !i.trim().is_empty()) {
        store.research_get(id.trim()).map_err(storage)?
    } else if let Some(title) = args["title"]
        .as_str()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
    {
        store
            .research_list()
            .map_err(storage)?
            .into_iter()
            .find(|d| {
                d["title"]
                    .as_str()
                    .is_some_and(|t| t.to_lowercase().contains(&title))
            })
    } else {
        return Err(err("research.invalid", "Indica «id» o parte del titolo"));
    };
    let doc = doc.ok_or_else(|| err("research.not_found", "Nessuna ricerca corrisponde"))?;
    let mut out = summary(&doc, true);
    out["markdown"] = json!(to_markdown(&doc));
    out["untrusted"] = json!(true);
    Ok(out)
}

pub fn delete(store: &Store, id: &str) -> Result<bool, ToolError> {
    store
        .research_delete(id)
        .map_err(|e| err("research.unavailable", e.to_string()))
}

/// Rapporto in Markdown: risultati per tipo, fonti con attendibilità e domande aperte.
pub fn to_markdown(doc: &Value) -> String {
    let mut out = format!("# {}\n\n", doc["title"].as_str().unwrap_or("Ricerca"));
    if let Some(q) = doc["query"].as_str() {
        out.push_str(&format!("Domanda: {q}\n\n"));
    }
    out.push_str(&format!(
        "Stato: {} · aggiornata il {}\n\n",
        doc["status"].as_str().unwrap_or("open"),
        doc["updated_at"].as_str().unwrap_or("?")
    ));
    for kind in KINDS {
        let rows: Vec<&Value> = doc["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["kind"] == kind)
            .collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n", kind.replace('_', " ")));
        for f in rows {
            out.push_str(&format!("- {}\n", f["text"].as_str().unwrap_or("")));
        }
        out.push('\n');
    }
    if let Some(sources) = doc["sources"].as_array().filter(|s| !s.is_empty()) {
        out.push_str("## Fonti\n");
        for s in sources {
            out.push_str(&format!(
                "- [{}]({}) — attendibilità {}\n",
                s["title"]
                    .as_str()
                    .filter(|t| !t.is_empty())
                    .unwrap_or(s["url"].as_str().unwrap_or("")),
                s["url"].as_str().unwrap_or(""),
                s["reliability"].as_str().unwrap_or("media")
            ));
        }
        out.push('\n');
    }
    if let Some(q) = doc["questions"].as_array().filter(|q| !q.is_empty()) {
        out.push_str("## Domande ancora aperte\n");
        for item in q {
            out.push_str(&format!("- {}\n", item.as_str().unwrap_or("")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let dir =
            std::env::temp_dir().join(format!("jarvis-research-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("t.sqlite3")).unwrap()
    }

    #[test]
    fn a_research_session_accumulates_without_duplicates_and_can_be_resumed_by_title() {
        let s = store();
        let created = save(
            &s,
            &json!({"title": "Confronto database", "query": "SQLite o PostgreSQL?",
                "findings": ["SQLite è incorporato", {"text": "PostgreSQL scala meglio in scrittura", "kind": "verificato"}],
                "sources": [{"url": "https://www.postgresql.org/docs/", "title": "Documentazione"},
                            {"url": "javascript:alert(1)"}, {"url": "https://www.reddit.com/r/x"}],
                "questions": ["Quanti utenti contemporanei?"]}),
        )
        .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["findings"], 2);
        assert_eq!(created["sources"], 2, "l'indirizzo non http(s) è scartato");
        assert_eq!(created["session"]["findings"][0]["kind"], "non_confermato");
        let again = save(
            &s,
            &json!({"id": id, "findings": ["SQLite è incorporato", "Serve un backup regolare"],
                "sources": [{"url": "https://www.postgresql.org/docs/"}], "questions": [], "status": "done"}),
        )
        .unwrap();
        assert_eq!(
            (again["findings"].as_u64(), again["sources"].as_u64()),
            (Some(3), Some(2))
        );
        assert_eq!(again["open_questions"], 0);
        assert_eq!(again["status"], "done");
        let resumed = read(&s, &json!({"title": "database"})).unwrap();
        assert_eq!(resumed["id"], id.as_str());
        let md = resumed["markdown"].as_str().unwrap();
        assert!(
            md.contains("## verificato")
                && md.contains("attendibilità alta")
                && md.contains("attendibilità bassa"),
            "{md}"
        );
        assert_eq!(list(&s).unwrap()["items"].as_array().unwrap().len(), 1);
        assert!(delete(&s, &id).unwrap());
        assert_eq!(
            read(&s, &json!({"id": id})).unwrap_err().code,
            "research.not_found"
        );
    }

    #[test]
    fn invalid_or_sensitive_sessions_are_refused() {
        let s = store();
        assert_eq!(save(&s, &json!({})).unwrap_err().code, "research.invalid");
        assert_eq!(
            save(&s, &json!({"title": "x", "status": "boh"}))
                .unwrap_err()
                .code,
            "research.invalid"
        );
        assert_eq!(
            save(
                &s,
                &json!({"title": "x", "findings": ["la chiave è nvapi-abcdefghijklmnop12345"]})
            )
            .unwrap_err()
            .code,
            "research.sensitive"
        );
        assert_eq!(
            save(&s, &json!({"id": "inesistente"})).unwrap_err().code,
            "research.not_found"
        );
        assert_eq!(read(&s, &json!({})).unwrap_err().code, "research.invalid");
    }
}
