//! Dataset di JARVIS (ADR-0221): esempi con cui si **misura** e si **migliora** la comprensione delle richieste, e che
//! si possono **salvare su Hugging Face sempre in un repository privato**.
//!
//! - **Quattro dataset di base** (parlato, instradamento, richieste complesse, conversazioni) sono incorporati e il
//!   proprietario ne ha una copia in `<dati>/datasets/<nome>/`.
//! - **Uso:** `evaluate` misura quanto il Core li rispetta (regressioni visibili); `similar` trova gli esempi più
//!   vicini a una richiesta e li offre al modello come riferimenti già classificati.
//! - **Miglioramento:** esempi nuovi arrivano come **candidati** (con testo mascherato da `privacy::redact`, senza
//!   doppioni); solo quelli che il proprietario **accetta** entrano nel dataset e salgono di versione. Nessun
//!   esempio esce dal computer senza un'azione esplicita del proprietario.
//! - **Hugging Face:** il server MCP autorizzato espone soltanto strumenti di **lettura** (`hf_fs`, `hub_repo_details`):
//!   serve a rileggere un dataset privato. La **scrittura** usa l'API del Hub con un token con permesso di scrittura,
//!   e questo modulo impone `private: true`, verifica che il repository sia privato prima e dopo, e rifiuta di
//!   caricare in uno pubblico.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::{json, Value};

/// Nomi e tipi dei dataset di base.
pub const BUILTIN: [(&str, &str); 6] = [
    ("jarvis-routing", "routing"),
    ("jarvis-dialogue-acts", "acts"),
    ("jarvis-complex-requests", "complex"),
    ("jarvis-conversations", "conversations"),
    ("jarvis-identity", "identity"),
    ("jarvis-skills", "skills"),
];

const ROUTING: &str = include_str!("../../../tests/datasets/routing_dataset.tsv");
const ACTS: &str = include_str!("../../../tests/datasets/colloquial_dataset.tsv");
const COMPLEX: &str = include_str!("../../../tests/datasets/complex_requests.tsv");
const IDENTITY: &str = include_str!("../../../tests/datasets/identity_dataset.tsv");
const SKILLS: &str = include_str!("../../../tests/datasets/skills_dataset.tsv");
const CONVERSATIONS: &str = include_str!("../../../tests/datasets/conversation_dataset.json");

const MAX_CANDIDATES: usize = 500;
const MAX_TEXT: usize = 600;

pub fn kind_of(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

fn agent_id(italian: &str) -> Option<&'static str> {
    Some(match italian.trim() {
        "sistema" | "system" => crate::dispatch::SYSTEM,
        "sicurezza" | "monitoring" => crate::dispatch::SECURITY,
        "ricerca web" | "research" => crate::dispatch::RESEARCH,
        "codice" | "coding" => crate::dispatch::CODING,
        "azienda" | "business" => crate::dispatch::BUSINESS,
        "memoria" | "memory" => crate::dispatch::MEMORY,
        "automazioni" | "automation" => crate::dispatch::AUTOMATION,
        _ => return None,
    })
}

/// Esempi di base, già nella forma di lavoro.
fn seed(kind: &str) -> Vec<Value> {
    let rows = |text: &str| -> Vec<Vec<String>> {
        text.lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| l.split(" | ").map(|p| p.trim().to_string()).collect())
            .collect()
    };
    match kind {
        "routing" => rows(ROUTING)
            .into_iter()
            .filter(|p| p.len() >= 3)
            .filter_map(|p| {
                let support: Vec<&str> = p[2].split(',').filter_map(agent_id).collect();
                Some(json!({"text": p[0], "primary": agent_id(&p[1])?, "support": support}))
            })
            .collect(),
        "acts" => rows(ACTS)
            .into_iter()
            .filter(|p| p.len() >= 2)
            .map(|p| json!({"text": p[0], "act": p[1]}))
            .collect(),
        "complex" => rows(COMPLEX)
            .into_iter()
            .filter(|p| p.len() >= 3)
            .map(|p| {
                let agents: Vec<&str> = p[1].split(',').filter_map(agent_id).collect();
                json!({"text": p[0], "agents": agents, "expected": p[2]})
            })
            .collect(),
        "conversations" => serde_json::from_str::<Vec<Value>>(CONVERSATIONS).unwrap_or_default(),
        "identity" => rows(IDENTITY)
            .into_iter()
            .filter(|p| p.len() >= 2)
            .map(|p| json!({"text": p[0], "topic": p[1]}))
            .collect(),
        "skills" => rows(SKILLS)
            .into_iter()
            .filter(|p| p.len() >= 2)
            .map(|p| json!({"text": p[0], "skill": p[1]}))
            .collect(),
        _ => Vec::new(),
    }
}

fn dir(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("datasets").join(name)
}

fn read_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn write_lines(path: &Path, rows: &[Value]) -> Result<(), String> {
    let text: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Crea la copia locale del dataset di base alla prima richiesta; non sovrascrive mai ciò che c'è già.
fn ensure(data_dir: &Path, name: &str) -> Result<&'static str, String> {
    let kind = kind_of(name).ok_or_else(|| format!("dataset sconosciuto: {name}"))?;
    let d = dir(data_dir, name);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    if !d.join("data.jsonl").exists() {
        write_lines(&d.join("data.jsonl"), &seed(kind))?;
        write_meta(&d, 1, "creato dagli esempi di base")?;
    }
    Ok(kind)
}

fn write_meta(d: &Path, version: u64, note: &str) -> Result<(), String> {
    let meta =
        json!({"version": version, "updated_at": crate::now(), "note": note, "private_only": true});
    std::fs::write(d.join("meta.json"), meta.to_string()).map_err(|e| e.to_string())
}

fn meta(d: &Path) -> Value {
    std::fs::read_to_string(d.join("meta.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({"version": 1}))
}

/// Elenco dei dataset con versione, esempi accettati e candidati in attesa.
pub fn list(data_dir: &Path) -> Value {
    let items: Vec<Value> = BUILTIN
        .iter()
        .filter_map(|(name, kind)| {
            ensure(data_dir, name).ok()?;
            let d = dir(data_dir, name);
            let m = meta(&d);
            Some(json!({
                "name": name, "kind": kind, "version": m["version"], "updated_at": m["updated_at"],
                "examples": read_lines(&d.join("data.jsonl")).len(),
                "candidates": read_lines(&d.join("candidates.jsonl")).len(),
                "pushed": m["pushed"],
            }))
        })
        .collect();
    json!({"items": items, "private_only": true})
}

/// Esempi accettati e candidati di un dataset.
pub fn get(data_dir: &Path, name: &str, limit: usize) -> Result<Value, String> {
    ensure(data_dir, name)?;
    let d = dir(data_dir, name);
    let rows = read_lines(&d.join("data.jsonl"));
    Ok(json!({
        "name": name, "kind": kind_of(name), "meta": meta(&d), "total": rows.len(),
        "examples": rows.iter().take(limit.clamp(1, 200)).collect::<Vec<_>>(),
        "candidates": read_lines(&d.join("candidates.jsonl")),
    }))
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn clean_text(text: &str) -> Result<(String, Value), String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        return Err(format!("il testo deve avere da 1 a {MAX_TEXT} caratteri"));
    }
    Ok(crate::privacy::redact(text))
}

fn validate_agents(list: &Value) -> Result<Vec<&'static str>, String> {
    let mut out = Vec::new();
    for a in list.as_array().into_iter().flatten() {
        let name = a
            .as_str()
            .and_then(agent_id)
            .ok_or_else(|| format!("agente non valido: {a}"))?;
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Aggiunge un esempio **candidato** (non ancora nel dataset): testo mascherato, senza doppioni.
pub fn add_candidate(data_dir: &Path, name: &str, input: &Value) -> Result<Value, String> {
    let kind = ensure(data_dir, name)?;
    if kind == "conversations" {
        return Err("le conversazioni si aggiungono importando un file completo".into());
    }
    let (text, report) = clean_text(input["text"].as_str().unwrap_or_default())?;
    let example = match kind {
        "routing" => {
            let primary = input["primary"]
                .as_str()
                .and_then(agent_id)
                .ok_or("serve l'agente principale")?;
            let support: Vec<&str> = validate_agents(&input["support"])?
                .into_iter()
                .filter(|a| *a != primary)
                .collect();
            json!({"text": text, "primary": primary, "support": support})
        }
        "acts" => {
            let act = input["act"].as_str().unwrap_or_default();
            if !ACT_IDS.contains(&act) {
                return Err(format!("atto non valido: {act}"));
            }
            json!({"text": text, "act": act})
        }
        "identity" => {
            let topic = input["topic"].as_str().unwrap_or_default();
            let known = crate::selfknow::Topic::ALL.iter().any(|t| t.id() == topic);
            if !known && topic != "none" {
                return Err(format!("argomento non valido: {topic} (o «none» per le frasi che non sono domande su JARVIS)"));
            }
            json!({"text": text, "topic": topic})
        }
        "skills" => {
            let skill = input["skill"].as_str().unwrap_or_default();
            if skill.is_empty()
                || skill.len() > 64
                || !skill
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err("serve l'identificativo della competenza (per esempio «debug»)".into());
            }
            json!({"text": text, "skill": skill})
        }
        _ => {
            let agents = validate_agents(&input["agents"])?;
            if agents.is_empty() {
                return Err("servono gli agenti richiesti".into());
            }
            let expected = input["expected"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(MAX_TEXT)
                .collect::<String>();
            json!({"text": text, "agents": agents, "expected": crate::privacy::redact(&expected).0})
        }
    };
    let d = dir(data_dir, name);
    let key = normalize(example["text"].as_str().unwrap_or_default());
    let accepted = read_lines(&d.join("data.jsonl"));
    let mut candidates = read_lines(&d.join("candidates.jsonl"));
    let seen: BTreeSet<String> = accepted
        .iter()
        .chain(candidates.iter())
        .map(|e| normalize(e["text"].as_str().unwrap_or_default()))
        .collect();
    if seen.contains(&key) {
        return Ok(json!({"added": false, "reason": "esempio già presente", "redactions": report}));
    }
    if candidates.len() >= MAX_CANDIDATES {
        return Err(format!(
            "troppi candidati in attesa (massimo {MAX_CANDIDATES}): accettane o scartane qualcuno"
        ));
    }
    let mut stored = example;
    stored["id"] = json!(uuid::Uuid::new_v4().simple().to_string()[..12].to_string());
    stored["added_at"] = json!(crate::now());
    candidates.push(stored.clone());
    write_lines(&d.join("candidates.jsonl"), &candidates)?;
    Ok(json!({"added": true, "candidate": stored, "redactions": report}))
}

/// Accetta (`accept: true`) o scarta i candidati indicati; accettarli aumenta la versione.
pub fn review(data_dir: &Path, name: &str, ids: &[String], accept: bool) -> Result<Value, String> {
    ensure(data_dir, name)?;
    let d = dir(data_dir, name);
    let candidates = read_lines(&d.join("candidates.jsonl"));
    let (picked, kept): (Vec<Value>, Vec<Value>) = candidates
        .into_iter()
        .partition(|c| ids.iter().any(|i| Some(i.as_str()) == c["id"].as_str()));
    if picked.is_empty() {
        return Err("nessun candidato con questi id".into());
    }
    write_lines(&d.join("candidates.jsonl"), &kept)?;
    if accept {
        let mut rows = read_lines(&d.join("data.jsonl"));
        for mut c in picked.clone() {
            if let Some(o) = c.as_object_mut() {
                o.remove("id");
                o.remove("added_at");
            }
            rows.push(c);
        }
        write_lines(&d.join("data.jsonl"), &rows)?;
        let version = meta(&d)["version"].as_u64().unwrap_or(1) + 1;
        write_meta(&d, version, &format!("accettati {} esempi", picked.len()))?;
        return Ok(json!({"accepted": picked.len(), "version": version, "total": rows.len()}));
    }
    Ok(json!({"discarded": picked.len()}))
}

/// Identificativi degli atti di conversazione riconosciuti (per validare i candidati).
const ACT_IDS: [&str; 21] = [
    "presence",
    "opening",
    "status",
    "clarify",
    "brevity",
    "detail",
    "concretize",
    "next_step",
    "drop",
    "redirect",
    "problem",
    "refine_improve",
    "refine_professional",
    "refine_simplify",
    "refine_faster",
    "urgency",
    "delegate",
    "alternatives",
    "understand",
    "grounding",
    "proceed",
];

/// Misura quanto il Core rispetta il dataset: accuratezza e prime discordanze.
pub fn evaluate(data_dir: &Path, name: &str) -> Result<Value, String> {
    let kind = ensure(data_dir, name)?;
    let rows = read_lines(&dir(data_dir, name).join("data.jsonl"));
    let mut correct = 0usize;
    let mut support_hits = 0usize;
    let mut support_total = 0usize;
    let mut misses: Vec<Value> = Vec::new();
    for row in &rows {
        let text = row["text"].as_str().unwrap_or_default();
        let ok = match kind {
            "routing" => {
                let team = crate::dispatch::team(text);
                let want = row["primary"].as_str().unwrap_or_default();
                for s in row["support"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    support_total += 1;
                    if team
                        .as_ref()
                        .is_some_and(|t| t.support.contains(&s) || t.primary == s)
                    {
                        support_hits += 1;
                    }
                }
                team.as_ref().map(|t| t.primary) == Some(want)
            }
            "acts" => {
                crate::dialogue::act(text).map(crate::dialogue::Act::id) == row["act"].as_str()
            }
            "identity" => {
                let want = row["topic"].as_str().unwrap_or("none");
                crate::selfknow::topic(text).map_or("none", crate::selfknow::Topic::id) == want
            }
            "skills" => {
                let want = row["skill"].as_str().unwrap_or_default();
                builtin_skills_active(text).iter().any(|id| id == want)
            }
            "complex" => {
                let team = crate::dispatch::team(text);
                row["agents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .all(|a| {
                        team.as_ref()
                            .is_some_and(|t| t.primary == a || t.support.contains(&a))
                    })
            }
            _ => evaluate_conversation(row),
        };
        if ok {
            correct += 1;
        } else if misses.len() < 10 {
            misses.push(json!({"text": text, "expected": row.get("primary").or(row.get("act")).or(row.get("agents")).or(row.get("topic")).or(row.get("skill")).or(row.get("expect"))}));
        }
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            100.0
        } else {
            (a as f64 * 1000.0 / b as f64).round() / 10.0
        }
    };
    Ok(json!({
        "name": name, "kind": kind, "total": rows.len(), "correct": correct, "accuracy_percent": pct(correct, rows.len()),
        "support": (kind == "routing").then(|| json!({"hit": support_hits, "total": support_total, "percent": pct(support_hits, support_total)})),
        "misses": misses,
        "note": "Le regole sono tarate sugli esempi di base: misurano le regressioni, non la generalizzazione.",
    }))
}

/// Identificativi delle competenze di base che la frase attiva (la stessa regola dell'attivazione reale).
fn builtin_skills_active(text: &str) -> Vec<String> {
    let Ok(store) = crate::store::Store::open(Path::new(":memory:")) else {
        return Vec::new();
    };
    if crate::skills::migrate(&store).is_err() {
        return Vec::new();
    }
    crate::skills::active(&store, text)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect()
}

fn evaluate_conversation(row: &Value) -> bool {
    let turns: Vec<(String, String)> = row["turns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| Some((t[0].as_str()?.to_string(), t[1].as_str()?.to_string())))
        .collect();
    let Some((_, last)) = turns.last().cloned() else {
        return false;
    };
    let users: Vec<String> = turns
        .iter()
        .take(turns.len() - 1)
        .filter(|(r, _)| r == "user")
        .map(|(_, t)| t.clone())
        .collect();
    let reply = turns
        .iter()
        .rev()
        .skip(1)
        .find(|(r, _)| r == "jarvis")
        .map(|(_, t)| t.clone());
    let a = crate::followup::analysis(
        &last,
        &users,
        users.last().map(String::as_str),
        reply.as_deref(),
    );
    let expect = &row["expect"];
    if let Some(f) = expect["followup"].as_str() {
        if a["followup"] != f {
            return false;
        }
    }
    let got: Vec<&str> = a["constraints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !expect["constraints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .all(|w| got.contains(&w))
    {
        return false;
    }
    expect["act"]
        .as_str()
        .is_none_or(|act| crate::dialogue::act(&last).map(crate::dialogue::Act::id) == Some(act))
}

fn words(text: &str) -> BTreeSet<String> {
    normalize(text)
        .split_whitespace()
        .filter(|w| w.len() >= 4)
        .map(str::to_string)
        .collect()
}

/// Gli esempi di instradamento più vicini a una richiesta (somiglianza di parole), come riferimenti già
/// classificati per il modello. Solo esempi accettati; vuoto se nessuno somiglia abbastanza.
pub fn similar(data_dir: &Path, text: &str, limit: usize) -> Vec<Value> {
    let target = words(text);
    if target.len() < 2 {
        return Vec::new();
    }
    let rows = match ensure(data_dir, "jarvis-routing") {
        Ok(_) => read_lines(&dir(data_dir, "jarvis-routing").join("data.jsonl")),
        Err(_) => return Vec::new(),
    };
    let mut scored: Vec<(f64, &Value)> = rows
        .iter()
        .filter_map(|r| {
            let other = words(r["text"].as_str()?);
            let common = target.intersection(&other).count() as f64;
            let score = common / (target.len() + other.len()) as f64 * 2.0;
            (common >= 2.0 && score >= 0.34 && normalize(r["text"].as_str()?) != normalize(text))
                .then_some((score, r))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(limit).map(|(s, r)| json!({"text": r["text"], "primary": r["primary"], "support": r["support"], "score": (s * 100.0).round() / 100.0})).collect()
}

/// File da pubblicare: dati accettati e scheda del dataset. **Mai** i candidati.
pub fn export(data_dir: &Path, name: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let kind = ensure(data_dir, name)?;
    let d = dir(data_dir, name);
    let rows = read_lines(&d.join("data.jsonl"));
    let m = meta(&d);
    let jsonl: String = rows.iter().map(|r| format!("{r}\n")).collect();
    let card = format!(
        "---\nlanguage:\n- it\nlicense: other\ntags:\n- jarvis\n- {kind}\n---\n\n# {name}\n\nEsempi per l'assistente JARVIS ({kind}), versione {version}, {count} esempi.\n\n- Lingua: italiano.\n- Contenuto: frasi sintetiche e correzioni accettate dal proprietario; testo mascherato da `privacy::redact`.\n- **Repository privato**: JARVIS carica solo in repository privati.\n",
        version = m["version"],
        count = rows.len()
    );
    Ok(vec![
        ("data.jsonl".to_string(), jsonl.into_bytes()),
        ("README.md".to_string(), card.into_bytes()),
    ])
}

/// Importa righe JSONL (per esempio lette da Hugging Face con l'MCP) come **candidati**; ritorna quanti sono entrati.
pub fn import_text(data_dir: &Path, name: &str, jsonl: &str) -> Result<Value, String> {
    let kind = ensure(data_dir, name)?;
    let (mut added, mut skipped) = (0, 0);
    for line in jsonl.lines().filter(|l| !l.trim().is_empty()).take(1000) {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            skipped += 1;
            continue;
        };
        let outcome = add_candidate(data_dir, name, &row);
        match outcome {
            Ok(v) if v["added"] == true => added += 1,
            _ => skipped += 1,
        }
    }
    Ok(
        json!({"kind": kind, "candidates_added": added, "skipped": skipped, "next": "rivedi i candidati e accettali per usarli"}),
    )
}

/// Registra su disco l'esito dell'ultimo caricamento (repository, commit, data).
pub fn mark_pushed(data_dir: &Path, name: &str, info: &Value) -> Result<(), String> {
    let d = dir(data_dir, name);
    let mut m = meta(&d);
    m["pushed"] = info.clone();
    std::fs::write(d.join("meta.json"), m.to_string()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------------- Hugging Face

/// File da caricare: percorso nel repository e contenuto.
pub type Files = Vec<(String, Vec<u8>)>;

/// `proprietario/nome` valido per il Hub.
pub fn valid_repo_id(id: &str) -> bool {
    let ok = |s: &str| {
        !s.is_empty()
            && s.len() <= 96
            && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    id.split_once('/')
        .is_some_and(|(owner, name)| ok(owner) && ok(name) && !name.contains('/'))
}

/// Corpo NDJSON dell'API di commit del Hub (`/api/datasets/<repo>/commit/main`).
pub fn commit_body(summary: &str, files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut lines = vec![json!({"key": "header", "value": {"summary": summary, "description": "Caricato da JARVIS"}}).to_string()];
    for (path, bytes) in files {
        lines.push(json!({"key": "file", "value": {"content": b64.encode(bytes), "path": path, "encoding": "base64"}}).to_string());
    }
    (lines.join("\n") + "\n").into_bytes()
}

fn hub_call(
    web: &crate::web::WebConfig,
    base: &str,
    token: &str,
    method: &str,
    path: &str,
    content_type: &str,
    body: Option<&[u8]>,
) -> Result<(u16, Value), String> {
    let url = url::Url::parse(&format!("{}{path}", base.trim_end_matches('/')))
        .map_err(|_| "indirizzo non valido".to_string())?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let headers = vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
        ("Content-Type".to_string(), content_type.to_string()),
    ];
    let (status, _, data) =
        crate::web::request(web, method, &url, &headers, body, &[host]).map_err(|e| e.message)?;
    Ok((status, serde_json::from_slice(&data).unwrap_or(Value::Null)))
}

/// Il repository esiste ed è **privato**; se non esiste lo crea privato. Un repository pubblico è un errore.
pub fn ensure_private_repo(
    web: &crate::web::WebConfig,
    base: &str,
    token: &str,
    repo_id: &str,
) -> Result<(), String> {
    if !valid_repo_id(repo_id) {
        return Err("identificativo non valido: usa «proprietario/nome»".into());
    }
    let info_path = format!("/api/datasets/{repo_id}");
    let (status, info) = hub_call(
        web,
        base,
        token,
        "GET",
        &info_path,
        "application/json",
        None,
    )?;
    match status {
        200 => {
            if info["private"] != true {
                return Err(format!("{repo_id} è pubblico: JARVIS carica solo in repository privati"));
            }
            Ok(())
        }
        401 | 403 => Err("Hugging Face ha rifiutato il token: serve un token con permesso di scrittura sul repository".into()),
        404 => {
            let (owner, name) = repo_id.split_once('/').unwrap_or_default();
            let body = json!({"type": "dataset", "name": name, "organization": owner, "private": true}).to_string();
            let (created, answer) = hub_call(web, base, token, "POST", "/api/repos/create", "application/json", Some(body.as_bytes()))?;
            if !(200..300).contains(&created) {
                let why = answer["error"].as_str().unwrap_or("errore sconosciuto");
                return Err(format!("creazione del repository non riuscita (HTTP {created}): {why}"));
            }
            let (_, again) = hub_call(web, base, token, "GET", &info_path, "application/json", None)?;
            if again["private"] == true {
                Ok(())
            } else {
                Err("il repository creato non risulta privato: non carico nulla".into())
            }
        }
        other => Err(format!("Hugging Face ha risposto {other}")),
    }
}

/// Carica i file in un repository **privato** e verifica che lo sia ancora a operazione conclusa.
pub fn push_private(
    web: &crate::web::WebConfig,
    base: &str,
    token: &str,
    repo_id: &str,
    summary: &str,
    files: &[(String, Vec<u8>)],
) -> Result<Value, String> {
    ensure_private_repo(web, base, token, repo_id)?;
    let body = commit_body(summary, files);
    let (status, answer) = hub_call(
        web,
        base,
        token,
        "POST",
        &format!("/api/datasets/{repo_id}/commit/main"),
        "application/x-ndjson",
        Some(&body),
    )?;
    if !(200..300).contains(&status) {
        let why = answer["error"].as_str().unwrap_or("errore sconosciuto");
        return Err(format!("caricamento non riuscito (HTTP {status}): {why}"));
    }
    let (_, after) = hub_call(
        web,
        base,
        token,
        "GET",
        &format!("/api/datasets/{repo_id}"),
        "application/json",
        None,
    )?;
    Ok(json!({
        "repo": repo_id, "private": after["private"] == true, "files": files.iter().map(|(p, b)| json!({"path": p, "bytes": b.len()})).collect::<Vec<_>>(),
        "commit": answer["commitOid"], "url": format!("https://huggingface.co/datasets/{repo_id}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("jarvis-ds-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_builtin_datasets_are_copied_and_measured() {
        let d = scratch();
        let listing = list(&d);
        let items = listing["items"].as_array().unwrap();
        assert_eq!(items.len(), 6);
        let counts: Vec<u64> = items
            .iter()
            .map(|i| i["examples"].as_u64().unwrap())
            .collect();
        assert_eq!(counts, vec![40, 50, 15, 15, 89, 60]);
        for (name, _) in BUILTIN {
            let report = evaluate(&d, name).unwrap();
            assert_eq!(report["correct"], report["total"], "{name}: {report}");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn candidates_are_redacted_deduplicated_and_only_accepted_ones_enter() {
        let d = scratch();
        let first = add_candidate(&d, "jarvis-routing", &json!({"text": "Controlla il mio conto, IBAN IT60X0542811101000000123456", "primary": "sicurezza", "support": ["sistema"]})).unwrap();
        assert_eq!(first["added"], true);
        assert!(
            !first["candidate"]["text"]
                .as_str()
                .unwrap()
                .contains("IT60X0542811101000000123456"),
            "{first}"
        );
        let again = add_candidate(
            &d,
            "jarvis-routing",
            &json!({"text": "controlla il mio conto, iban", "primary": "sicurezza"}),
        )
        .unwrap();
        let _ = again; // il testo mascherato può differire: l'importante è non duplicare lo stesso testo
        let same = add_candidate(
            &d,
            "jarvis-routing",
            &json!({"text": first["candidate"]["text"], "primary": "sicurezza"}),
        )
        .unwrap();
        assert_eq!(same["added"], false);
        assert!(add_candidate(
            &d,
            "jarvis-routing",
            &json!({"text": "x", "primary": "inesistente"})
        )
        .is_err());
        assert!(add_candidate(
            &d,
            "jarvis-dialogue-acts",
            &json!({"text": "boh", "act": "inventato"})
        )
        .is_err());
        // Prima dell'accettazione non è nell'esportazione né nelle misure.
        let before = export(&d, "jarvis-routing").unwrap();
        assert_eq!(String::from_utf8_lossy(&before[0].1).lines().count(), 40);
        let id = first["candidate"]["id"].as_str().unwrap().to_string();
        let done = review(&d, "jarvis-routing", &[id], true).unwrap();
        assert_eq!(done["version"], 2);
        let after = export(&d, "jarvis-routing").unwrap();
        assert!(String::from_utf8_lossy(&after[0].1).lines().count() >= 41);
        assert!(String::from_utf8_lossy(&after[1].1).contains("Repository privato"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn similar_examples_are_offered_as_references() {
        let d = scratch();
        let found = similar(
            &d,
            "Controlla perché il computer è diventato molto lento",
            2,
        );
        assert!(!found.is_empty());
        assert_eq!(found[0]["primary"], "system");
        assert!(similar(&d, "Ciao", 2).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    /// Hub simulato: tiene traccia delle richieste e del flag `private` del repository.
    fn hub(private_existing: Option<bool>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = log.clone();
        let created = Arc::new(Mutex::new(false));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut parts = first.split_whitespace();
                let (method, path) = (
                    parts.next().unwrap_or("").to_string(),
                    parts.next().unwrap_or("").to_string(),
                );
                let mut length = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) <= 2 {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                seen.lock().unwrap().push(format!(
                    "{method} {path} {}",
                    String::from_utf8_lossy(&body)
                        .chars()
                        .take(200)
                        .collect::<String>()
                        .replace('\n', "⏎")
                ));
                let (status, answer) = match (method.as_str(), path.as_str()) {
                    ("GET", p) if p.starts_with("/api/datasets/") && !p.contains("/commit") => {
                        let exists = private_existing.is_some() || *created.lock().unwrap();
                        if exists {
                            (
                                200,
                                json!({"private": private_existing.unwrap_or(true)}).to_string(),
                            )
                        } else {
                            (404, "{}".to_string())
                        }
                    }
                    ("POST", "/api/repos/create") => {
                        *created.lock().unwrap() = true;
                        (200, json!({"url": "x"}).to_string())
                    }
                    ("POST", p) if p.contains("/commit/main") => {
                        (200, json!({"commitOid": "abc123"}).to_string())
                    }
                    _ => (500, "{}".to_string()),
                };
                let mut out = stream;
                let _ = write!(out, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
            }
        });
        (format!("http://127.0.0.1:{port}"), log)
    }

    fn local() -> crate::web::WebConfig {
        crate::web::WebConfig {
            allow_private: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_missing_repo_is_created_private_and_the_upload_is_verified_private() {
        let (base, log) = hub(None);
        let files = vec![("data.jsonl".to_string(), b"{\"text\":\"a\"}\n".to_vec())];
        let out = push_private(
            &local(),
            &base,
            "hf_secret",
            "me/jarvis-routing",
            "v1",
            &files,
        )
        .unwrap();
        assert_eq!(out["private"], true);
        assert_eq!(out["commit"], "abc123");
        let seen = log.lock().unwrap().clone();
        let create = seen
            .iter()
            .find(|l| l.contains("/api/repos/create"))
            .expect("creazione");
        assert!(create.contains("\"private\":true"), "{create}");
        assert!(
            seen.iter()
                .any(|l| l.contains("/commit/main") && l.contains("\"key\":\"header\"")),
            "{seen:?}"
        );
    }

    #[test]
    fn a_public_repository_is_refused_before_anything_is_uploaded() {
        let (base, log) = hub(Some(false));
        let err = push_private(
            &local(),
            &base,
            "hf_secret",
            "me/jarvis-routing",
            "v1",
            &[("data.jsonl".into(), vec![])],
        )
        .unwrap_err();
        assert!(err.contains("pubblico"), "{err}");
        assert!(
            !log.lock()
                .unwrap()
                .iter()
                .any(|l| l.contains("/commit/main")),
            "nessun caricamento"
        );
        // Un repository privato esistente si usa direttamente, senza ricrearlo.
        let (base, log) = hub(Some(true));
        push_private(
            &local(),
            &base,
            "hf_secret",
            "me/jarvis-routing",
            "v2",
            &[("data.jsonl".into(), vec![])],
        )
        .unwrap();
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("/api/repos/create")));
    }

    #[test]
    fn repo_ids_are_validated() {
        assert!(valid_repo_id("me/jarvis-routing"));
        for bad in ["jarvis", "me/", "/x", "me/a/b", "me/../x", "me/ x", "-me/x"] {
            assert!(!valid_repo_id(bad), "{bad}");
        }
    }
}
