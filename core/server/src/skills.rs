//! Competenze (skills, ADR-0048): istruzioni riutilizzabili del proprietario, con parole di attivazione e
//! strumenti suggeriti. Una competenza attiva entra nel prompt dell'agente e rende disponibili i suoi
//! strumenti per quella richiesta; non amplia i permessi (policy, conferme e audit restano invariati).

use rusqlite::params;
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

/// Competenze attive al massimo per richiesta e lunghezza massima delle istruzioni.
pub const MAX_ACTIVE: usize = 2;
pub const MAX_INSTRUCTIONS: usize = 4000;

/// Competenza inclusa: id, nome, descrizione, parole di attivazione, istruzioni, strumenti.
type Builtin = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
);

/// Competenze incluse, create al primo avvio e modificabili dal proprietario.
const BUILTIN: [Builtin; 5] = [
    (
        "briefing",
        "Briefing del mattino",
        "Riepilogo della giornata: ora, meteo, cose da fare, agenda ed email non lette.",
        &["briefing", "buongiorno", "riepilogo della giornata", "cosa ho oggi"],
        "Prepara un briefing breve in cinque punti al massimo: data e ora, meteo della città del proprietario (dalla memoria; se manca chiedila), cose da fare in scadenza, eventi di oggi dal calendario collegato e numero di email non lette con i mittenti principali. Usa solo gli strumenti disponibili: se un servizio non è collegato, dillo in una riga senza insistere. Chiudi con la priorità consigliata per la giornata.",
        &["time.now", "weather.forecast", "todo.list"],
    ),
    (
        "scrittura",
        "Scrittura e revisione",
        "Correzione, riscrittura e miglioramento di testi in italiano.",
        &["correggi", "rivedi il testo", "riscrivi", "migliora il testo", "bozza di"],
        "Quando rivedi un testo: correggi ortografia, grammatica e punteggiatura, rendi le frasi più chiare e attive, mantieni il tono e il significato dell'autore. Restituisci prima il testo corretto, poi al massimo tre note sulle modifiche importanti. Non inventare fatti e non aggiungere contenuti non richiesti.",
        &[],
    ),
    (
        "ricerca",
        "Ricerca con fonti",
        "Ricerca approfondita sul Web con confronto e citazione delle fonti.",
        &["ricerca approfondita", "approfondisci", "confronta le fonti", "fai una ricerca su"],
        "Cerca da almeno due fonti indipendenti (web.search, poi web.fetch o web.browse sulle più autorevoli). Riporta i fatti concordanti, segnala le discrepanze e cita ogni affermazione con [n] e l'URL. Distingui fatti, stime e opinioni. Se le fonti sono insufficienti o datate, dillo.",
        &["web.search", "web.fetch", "web.browse"],
    ),
    (
        "organizzazione",
        "Organizzazione",
        "Pianificazione di giornata e settimana con cose da fare e note.",
        &["organizza", "pianifica la settimana", "pianifica la giornata", "aiutami a organizzare"],
        "Aiuta il proprietario a pianificare: raccogli le cose da fare aperte e le scadenze, proponi un ordine per priorità e durata stimata, suggerisci cosa rimandare. Crea nuove cose da fare solo se il proprietario lo chiede esplicitamente.",
        &["todo.list", "todo.add", "notes.search", "time.now"],
    ),
    (
        "spese",
        "Spese",
        "Annotazione e somma delle spese personali nelle note con etichetta «spese».",
        &["ho speso", "spesa di", "budget", "quanto ho speso"],
        "Registra ogni spesa come nota con etichetta «spese» e testo «AAAA-MM-GG; importo in euro; categoria; descrizione». Per i totali cerca le note con etichetta «spese» e somma con math.calc, mostrando il calcolo. Non arrotondare gli importi.",
        &["notes.add", "notes.list", "notes.search", "math.calc"],
    ),
];

pub fn migrate(store: &Store) -> StoreResult<()> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skills (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
             triggers TEXT NOT NULL, instructions TEXT NOT NULL, tools TEXT NOT NULL, enabled INTEGER NOT NULL,
             builtin INTEGER NOT NULL, updated_at TEXT NOT NULL);",
    )?;
    for (id, name, description, triggers, instructions, tools) in BUILTIN {
        store.conn.execute(
            "INSERT OR IGNORE INTO skills (id, name, description, triggers, instructions, tools, enabled, builtin, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, ?7)",
            params![id, name, description, json!(triggers).to_string(), instructions, json!(tools).to_string(), crate::now()],
        )?;
    }
    Ok(())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |i: usize| -> rusqlite::Result<Value> {
        Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or(json!([])))
    };
    Ok(json!({
        "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "description": r.get::<_, String>(2)?,
        "triggers": parse(3)?, "instructions": r.get::<_, String>(4)?, "tools": parse(5)?,
        "enabled": r.get::<_, i64>(6)? == 1, "builtin": r.get::<_, i64>(7)? == 1, "updated_at": r.get::<_, String>(8)?,
    }))
}

pub fn list(store: &Store) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT id, name, description, triggers, instructions, tools, enabled, builtin, updated_at FROM skills ORDER BY builtin DESC, name",
    )?;
    let rows = stmt.query_map([], row)?;
    rows.collect()
}

fn fold(text: &str) -> String {
    text.to_lowercase()
        .replace('à', "a")
        .replace(['è', 'é'], "e")
        .replace('ì', "i")
        .replace('ò', "o")
        .replace('ù', "u")
}

/// Competenze attive per un messaggio: richiamate per nome («competenza X», «usa X») o da una parola di
/// attivazione; al massimo `MAX_ACTIVE`, solo quelle abilitate.
pub fn active(store: &Store, message: &str) -> Vec<Value> {
    let text = fold(message);
    list(store)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s["enabled"] == true)
        .filter(|s| {
            let name = fold(s["name"].as_str().unwrap_or_default());
            let named = text.contains(&format!("competenza {name}"))
                || text.contains(&format!("usa {name}"));
            named
                || s["triggers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|t| !t.trim().is_empty() && text.contains(&fold(t)))
        })
        .take(MAX_ACTIVE)
        .collect()
}

/// Valida e normalizza una competenza; gli strumenti devono esistere nel catalogo.
pub fn validate(input: &Value, known_tool: impl Fn(&str) -> bool) -> Result<Value, String> {
    let text = |k: &str, max: usize| -> Result<String, String> {
        let v = input[k].as_str().unwrap_or_default().trim().to_string();
        if v.chars().count() > max {
            return Err(format!("«{k}» supera {max} caratteri"));
        }
        Ok(v)
    };
    let name = text("name", 80)?;
    if name.is_empty() {
        return Err("serve un nome".into());
    }
    let instructions = text("instructions", MAX_INSTRUCTIONS)?;
    if instructions.is_empty() {
        return Err("servono le istruzioni".into());
    }
    let list = |k: &str, max: usize| -> Vec<String> {
        input[k]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|s| s.trim().chars().take(80).collect::<String>())
            .filter(|s| !s.is_empty())
            .take(max)
            .collect()
    };
    let triggers = list("triggers", 12);
    let tools = list("tools", 12);
    if let Some(bad) = tools.iter().find(|t| !known_tool(t)) {
        return Err(format!("strumento sconosciuto: {bad}"));
    }
    Ok(
        json!({"name": name, "description": text("description", 300)?, "instructions": instructions,
              "triggers": triggers, "tools": tools, "enabled": input["enabled"].as_bool().unwrap_or(true)}),
    )
}

/// Crea o aggiorna una competenza (id derivato dal nome se nuovo).
pub fn save(store: &Store, id: Option<&str>, skill: &Value) -> StoreResult<String> {
    let id = id.map(str::to_string).unwrap_or_else(|| {
        let base: String = fold(skill["name"].as_str().unwrap_or("competenza"))
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .chars()
            .take(40)
            .collect();
        format!("{base}-{}", &uuid::Uuid::new_v4().simple().to_string()[..4])
    });
    store.conn.execute(
        "INSERT INTO skills (id, name, description, triggers, instructions, tools, enabled, builtin, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, triggers = excluded.triggers,
             instructions = excluded.instructions, tools = excluded.tools, enabled = excluded.enabled, updated_at = excluded.updated_at",
        params![
            id,
            skill["name"].as_str(),
            skill["description"].as_str(),
            skill["triggers"].to_string(),
            skill["instructions"].as_str(),
            skill["tools"].to_string(),
            skill["enabled"].as_bool().unwrap_or(true) as i64,
            crate::now()
        ],
    )?;
    Ok(id)
}

pub fn delete(store: &Store, id: &str) -> StoreResult<bool> {
    Ok(store
        .conn
        .execute("DELETE FROM skills WHERE id = ?1", [id])?
        > 0)
}
