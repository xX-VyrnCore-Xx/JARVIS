//! Profili di workspace (ADR-0185): un progetto o una modalità di lavoro associa applicazioni e cartelle
//! della workspace. Il profilo è solo un elenco scelto dal proprietario: **attivarlo non esegue nulla**, produce
//! il piano dei passi (`app.open`, `fs.list`) che il proprietario conferma uno a uno attraverso i normali
//! controlli di permesso. Un profilo non può contenere comandi, URL o credenziali.

use std::path::Path;

use serde_json::{json, Value};

use crate::appcontrol::valid_app_name;
use crate::tools::lexically_inside;

const FILE: &str = "workspace_profiles.json";
pub const MAX_PROFILES: usize = 24;
pub const MAX_ITEMS: usize = 12;

fn load(dir: &Path) -> Vec<Value> {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["profiles"].as_array().cloned())
        .unwrap_or_default()
}

fn save(dir: &Path, profiles: &[Value]) -> Result<(), String> {
    std::fs::write(dir.join(FILE), json!({"profiles": profiles}).to_string())
        .map_err(|e| e.to_string())
}

pub fn list(dir: &Path) -> Value {
    json!({"items": load(dir), "max": MAX_PROFILES})
}

fn strings(body: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(items) = body[key].as_array() else {
        return Ok(Vec::new());
    };
    if items.len() > MAX_ITEMS {
        return Err(format!("Al massimo {MAX_ITEMS} voci in «{key}»"));
    }
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| format!("«{key}» contiene un valore non testuale"))?;
        let text = text.trim().to_string();
        if !out.contains(&text) {
            out.push(text);
        }
    }
    Ok(out)
}

/// Crea o aggiorna (stesso nome) un profilo. Applicazioni con nome valido, cartelle relative alla workspace.
pub fn save_profile(dir: &Path, body: &Value) -> Result<Value, String> {
    let name = body["name"].as_str().unwrap_or_default().trim().to_string();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err("Il nome del profilo deve avere 1–40 caratteri".into());
    }
    let apps = strings(body, "apps")?;
    if let Some(bad) = apps.iter().find(|a| !valid_app_name(a)) {
        return Err(format!("Nome applicazione non valido: {bad}"));
    }
    let dirs = strings(body, "dirs")?;
    if let Some(bad) = dirs
        .iter()
        .find(|d| d.is_empty() || d.chars().count() > 200 || !lexically_inside(d))
    {
        return Err(format!(
            "Cartella non ammessa (solo percorsi nella workspace): {bad}"
        ));
    }
    let description = body["description"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if description.chars().count() > 200 {
        return Err("La descrizione supera 200 caratteri".into());
    }
    let mut profiles = load(dir);
    let existing = profiles.iter().position(|p| {
        p["name"]
            .as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(&name))
    });
    if existing.is_none() && profiles.len() >= MAX_PROFILES {
        return Err(format!("Al massimo {MAX_PROFILES} profili"));
    }
    let id = existing
        .and_then(|i| profiles[i]["id"].as_str().map(str::to_string))
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..8].to_string());
    let profile =
        json!({"id": id, "name": name, "description": description, "apps": apps, "dirs": dirs});
    match existing {
        Some(i) => profiles[i] = profile.clone(),
        None => profiles.push(profile.clone()),
    }
    save(dir, &profiles)?;
    Ok(profile)
}

pub fn remove(dir: &Path, key: &str) -> Result<bool, String> {
    let mut profiles = load(dir);
    let before = profiles.len();
    profiles.retain(|p| {
        p["id"] != key
            && !p["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(key))
    });
    if profiles.len() == before {
        return Ok(false);
    }
    save(dir, &profiles)?;
    Ok(true)
}

/// Piano di attivazione: non esegue nulla. Ogni passo è una chiamata di strumento da confermare.
pub fn activation_plan(dir: &Path, key: &str) -> Option<Value> {
    let profile = load(dir).into_iter().find(|p| {
        p["id"] == key
            || p["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(key))
    })?;
    let mut steps = Vec::new();
    for dir in profile["dirs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        steps.push(json!({"tool": "fs.list", "args": {"path": dir}, "needs_confirmation": false}));
    }
    for app in profile["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        steps.push(json!({"tool": "app.open", "args": {"app": app}, "needs_confirmation": true}));
    }
    Some(
        json!({"profile": profile, "steps": steps, "executes": false,
        "note": "Il profilo non apre nulla da solo: ogni applicazione richiede la tua conferma."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::current_dir().unwrap().join(format!(
            "target/jarvis-wsp-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn profiles_are_saved_updated_planned_and_removed_without_executing_anything() {
        let d = dir();
        let first = save_profile(
            &d,
            &json!({"name": "Sviluppo", "apps": ["Terminal", "Visual Studio Code", "Terminal"],
            "dirs": ["progetti/jarvis"]}),
        )
        .unwrap();
        assert_eq!(first["apps"].as_array().unwrap().len(), 2);
        let again = save_profile(&d, &json!({"name": "sviluppo", "apps": ["Safari"]})).unwrap();
        assert_eq!(
            again["id"], first["id"],
            "stesso nome: aggiorna, non duplica"
        );
        assert_eq!(list(&d)["items"].as_array().unwrap().len(), 1);
        save_profile(
            &d,
            &json!({"name": "Lavoro", "apps": ["Mail"], "dirs": ["docs"]}),
        )
        .unwrap();
        let plan = activation_plan(&d, "lavoro").unwrap();
        assert_eq!(plan["executes"], false);
        assert_eq!(plan["steps"][0]["tool"], "fs.list");
        assert_eq!(plan["steps"][1]["needs_confirmation"], true);
        assert!(activation_plan(&d, "inesistente").is_none());
        assert!(remove(&d, "Lavoro").unwrap());
        assert!(!remove(&d, "Lavoro").unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn commands_paths_outside_the_workspace_and_oversized_profiles_are_refused() {
        let d = dir();
        assert!(save_profile(&d, &json!({"name": "x", "apps": ["rm -rf /; Safari"]})).is_err());
        assert!(save_profile(&d, &json!({"name": "x", "dirs": ["../fuori"]})).is_err());
        assert!(save_profile(&d, &json!({"name": "x", "dirs": ["/etc"]})).is_err());
        assert!(save_profile(&d, &json!({"name": ""})).is_err());
        let many: Vec<String> = (0..13).map(|i| format!("App{i}")).collect();
        assert!(save_profile(&d, &json!({"name": "x", "apps": many})).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
