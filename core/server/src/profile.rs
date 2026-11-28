//! Profilo del proprietario (ADR-0133): chi è, che lavoro fa e come preferisce le risposte.
//!
//! Serve a rendere le risposte pertinenti (tono, livello tecnico, contesto di lavoro) senza che il proprietario le
//! ripeta ogni volta. È facoltativo, modificabile solo dal proprietario e non contiene recapiti: nome, email,
//! telefono e credenziali sono rifiutati. Il riepilogo entra nel prompt di sistema solo se «share_with_cloud» è attivo.

use std::path::Path;
use std::sync::{OnceLock, RwLock};

use serde_json::{json, Value};

const FILE: &str = "profile.json";

/// Campi di testo libero: nome, lunghezza massima e etichetta per gli errori.
const TEXT_FIELDS: [(&str, usize, &str); 6] = [
    ("role", 80, "ruolo"),
    ("company", 80, "azienda"),
    ("sector", 80, "settore"),
    ("expertise", 160, "competenze"),
    ("interests", 200, "interessi"),
    ("goals", 200, "obiettivi"),
];

/// Campi a scelta: nome e valori ammessi (il primo è il predefinito).
const CHOICES: [(&str, &[&str]); 3] = [
    ("language", &["auto", "it", "en"]),
    ("tone", &["auto", "formale", "cordiale", "diretto"]),
    ("level", &["auto", "principiante", "intermedio", "esperto"]),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub fields: Vec<(String, String)>,
    pub share_with_cloud: bool,
}

impl Default for Profile {
    fn default() -> Self {
        let mut fields: Vec<(String, String)> = TEXT_FIELDS
            .iter()
            .map(|(k, _, _)| (k.to_string(), String::new()))
            .collect();
        fields.extend(
            CHOICES
                .iter()
                .map(|(k, values)| (k.to_string(), values[0].to_string())),
        );
        Self {
            fields,
            share_with_cloud: true,
        }
    }
}

impl Profile {
    pub fn get(&self, key: &str) -> &str {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map_or("", |(_, v)| v.as_str())
    }

    fn set(&mut self, key: &str, value: String) {
        if let Some(slot) = self.fields.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        }
    }
}

/// Un testo del profilo non deve contenere recapiti né segreti.
fn clean(label: &str, value: &str, max: usize) -> Result<String, String> {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > max {
        return Err(format!("«{label}» può avere al massimo {max} caratteri"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("«{label}» contiene caratteri non validi"));
    }
    let digits = text.chars().filter(char::is_ascii_digit).count();
    if text.contains('@')
        || text.contains("://")
        || digits >= 7
        || crate::cloud::contains_credentials(&text)
    {
        return Err(format!(
            "«{label}» non può contenere email, indirizzi web, numeri di telefono o credenziali"
        ));
    }
    Ok(text)
}

pub fn load(dir: &Path) -> Profile {
    let doc: Value = std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let mut profile = Profile::default();
    for (key, max, label) in TEXT_FIELDS {
        if let Some(value) = doc[key].as_str().and_then(|v| clean(label, v, max).ok()) {
            profile.set(key, value);
        }
    }
    for (key, values) in CHOICES {
        if let Some(value) = doc[key].as_str().filter(|v| values.contains(v)) {
            profile.set(key, value.to_string());
        }
    }
    profile.share_with_cloud = doc["share_with_cloud"].as_bool().unwrap_or(true);
    profile
}

/// Percentuale di campi compilati e suggerimenti per i principali mancanti.
fn completion(profile: &Profile) -> (u32, Vec<&'static str>) {
    let mut filled = 0;
    let mut missing = Vec::new();
    for (key, hint) in [
        ("role", "il tuo ruolo"),
        ("sector", "il settore in cui lavori"),
        ("goals", "i tuoi obiettivi"),
        ("tone", "il tono che preferisci"),
        ("level", "il tuo livello tecnico"),
        ("interests", "i tuoi interessi"),
    ] {
        let value = profile.get(key);
        if value.is_empty() || value == "auto" {
            missing.push(hint);
        } else {
            filled += 1;
        }
    }
    (filled * 100 / 6, missing)
}

pub fn to_json(profile: &Profile) -> Value {
    let mut doc = json!({"share_with_cloud": profile.share_with_cloud});
    for (key, value) in &profile.fields {
        doc[key] = json!(value);
    }
    let (percent, missing) = completion(profile);
    doc["completion_percent"] = json!(percent);
    doc["missing"] = json!(missing);
    doc["choices"] = json!(CHOICES
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect::<serde_json::Map<_, _>>());
    doc
}

/// Aggiorna solo i campi presenti nella richiesta; valida tutto prima di scrivere.
pub fn update(dir: &Path, patch: &Value) -> Result<Value, String> {
    let mut profile = load(dir);
    for (key, max, label) in TEXT_FIELDS {
        if let Some(value) = patch.get(key) {
            let text = value
                .as_str()
                .ok_or(format!("«{label}» deve essere un testo"))?;
            profile.set(key, clean(label, text, max)?);
        }
    }
    for (key, values) in CHOICES {
        if let Some(value) = patch.get(key) {
            match value.as_str().filter(|v| values.contains(v)) {
                Some(v) => profile.set(key, v.to_string()),
                None => {
                    return Err(format!(
                        "valore non valido per «{key}» (ammessi: {})",
                        values.join(", ")
                    ))
                }
            }
        }
    }
    if let Some(value) = patch.get("share_with_cloud") {
        profile.share_with_cloud = value
            .as_bool()
            .ok_or("«share_with_cloud» deve essere vero o falso")?;
    }
    let mut doc = json!({"share_with_cloud": profile.share_with_cloud});
    for (key, value) in &profile.fields {
        doc[key] = json!(value);
    }
    std::fs::write(dir.join(FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(to_json(&profile))
}

/// Riepilogo per il prompt di sistema: vuoto se non c'è nulla da dire o se il proprietario lo ha escluso.
pub fn guidance(profile: &Profile) -> String {
    if !profile.share_with_cloud {
        return String::new();
    }
    let mut parts = Vec::new();
    let (role, company, sector) = (
        profile.get("role"),
        profile.get("company"),
        profile.get("sector"),
    );
    match (role.is_empty(), company.is_empty()) {
        (false, false) => parts.push(format!("ruolo: {role} presso {company}")),
        (false, true) => parts.push(format!("ruolo: {role}")),
        (true, false) => parts.push(format!("azienda: {company}")),
        _ => {}
    }
    for (label, key) in [
        ("settore", "sector"),
        ("competenze", "expertise"),
        ("obiettivi", "goals"),
        ("interessi", "interests"),
    ] {
        let value = profile.get(key);
        if !(value.is_empty() || (key == "sector" && sector.is_empty())) {
            parts.push(format!("{label}: {value}"));
        }
    }
    match profile.get("language") {
        "it" => parts.push("risponde in italiano".into()),
        "en" => parts.push("preferisce l'inglese".into()),
        _ => {}
    }
    match profile.get("tone") {
        "formale" => parts.push("tono formale".into()),
        "cordiale" => parts.push("tono cordiale e caldo".into()),
        "diretto" => parts.push("tono diretto, senza fronzoli".into()),
        _ => {}
    }
    match profile.get("level") {
        "principiante" => parts.push("livello tecnico: principiante, spiega i termini".into()),
        "intermedio" => parts.push("livello tecnico: intermedio".into()),
        "esperto" => parts.push("livello tecnico: esperto, salta le basi".into()),
        _ => {}
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(
        "Profilo del proprietario (usalo solo quando rende la risposta più pertinente e non citarlo): {}.",
        parts.join("; ")
    )
}

fn current() -> &'static RwLock<String> {
    static CURRENT: OnceLock<RwLock<String>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(String::new()))
}

/// Rende attivo il profilo per le prossime richieste ai modelli.
pub fn set_current(profile: &Profile) {
    *current().write().unwrap_or_else(|e| e.into_inner()) = guidance(profile);
}

pub fn current_guidance() -> String {
    current().read().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("jarvis-profile-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_empty_profile_adds_nothing_and_reports_what_is_missing() {
        let d = dir();
        let profile = load(&d);
        assert_eq!(guidance(&profile), "");
        let doc = to_json(&profile);
        assert_eq!(doc["completion_percent"], 0);
        assert_eq!(doc["missing"].as_array().unwrap().len(), 6);
        assert_eq!(doc["tone"], "auto");
    }

    #[test]
    fn updates_are_validated_saved_and_summarised() {
        let d = dir();
        let doc = update(
            &d,
            &json!({"role": "Direttore operativo", "company": "Rossi Meccanica", "sector": "manifattura",
                    "tone": "diretto", "level": "esperto", "goals": "ridurre i costi del 10%"}),
        )
        .unwrap();
        assert_eq!(doc["completion_percent"], 83);
        let text = guidance(&load(&d));
        for expected in [
            "ruolo: Direttore operativo presso Rossi Meccanica",
            "settore: manifattura",
            "tono diretto",
            "esperto",
            "ridurre i costi",
        ] {
            assert!(text.contains(expected), "{expected} in {text}");
        }
        // Una modifica parziale conserva il resto.
        update(&d, &json!({"tone": "formale"})).unwrap();
        assert!(load(&d).get("role") == "Direttore operativo" && load(&d).get("tone") == "formale");
    }

    #[test]
    fn contact_details_credentials_and_bad_values_are_refused() {
        let d = dir();
        for patch in [
            json!({"role": "scrivimi a mario@example.org"}),
            json!({"company": "chiama 333 1234567"}),
            json!({"goals": "vedi https://example.org"}),
            json!({"interests": "token ghp_abcdefghijklmnopqrstuvwxyz0123456789"}),
            json!({"tone": "scherzoso"}),
            json!({"role": 5}),
            json!({"role": "x".repeat(81)}),
            json!({"share_with_cloud": "sì"}),
        ] {
            assert!(update(&d, &patch).is_err(), "{patch}");
        }
        assert!(!d.join(FILE).exists());
    }

    #[test]
    fn opting_out_removes_the_profile_from_prompts() {
        let d = dir();
        update(&d, &json!({"role": "Medico", "share_with_cloud": false})).unwrap();
        let profile = load(&d);
        assert_eq!(profile.get("role"), "Medico");
        assert_eq!(guidance(&profile), "");
        set_current(&profile);
        assert_eq!(current_guidance(), "");
    }

    #[test]
    fn a_hand_edited_file_with_unsafe_values_is_ignored_field_by_field() {
        let d = dir();
        std::fs::write(
            d.join(FILE),
            json!({"role": "Analista", "company": "a@b.it", "tone": "urlato"}).to_string(),
        )
        .unwrap();
        let profile = load(&d);
        assert_eq!(
            (
                profile.get("role"),
                profile.get("company"),
                profile.get("tone")
            ),
            ("Analista", "", "auto")
        );
    }
}
