//! Pacchetti di estensione (ADR-0165): installazione, elenco e rimozione, solo dal proprietario.
//! L'installazione è tutto-o-niente: il pacchetto si valida per intero prima di toccare competenze o modelli.

use super::*;
use crate::plugins;

const FILE: &str = "plugins.json";

impl Jarvis {
    fn plugin_registry(&self) -> Vec<Value> {
        std::fs::read(self.cfg.data_dir.join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["plugins"].as_array().cloned())
            .unwrap_or_default()
    }

    fn plugin_registry_save(&self, plugins: &[Value]) -> Result<(), CoreError> {
        std::fs::write(
            self.cfg.data_dir.join(FILE),
            json!({"plugins": plugins}).to_string(),
        )
        .map_err(|e| CoreError::Internal(e.to_string()))
    }

    /// Pacchetti installati, con i componenti che hanno portato.
    pub fn plugins(&self) -> Value {
        json!({"items": self.plugin_registry(), "max": plugins::MAX_PLUGINS,
            "policy": "Un pacchetto contiene solo competenze e modelli di piano: nessun codice, nessuna rete, nessuna chiave. I suoi componenti restano sotto policy, conferme e audit."})
    }

    fn plugin_uninstall(&mut self, name: &str) -> Result<bool, CoreError> {
        let mut registry = self.plugin_registry();
        let Some(index) = registry.iter().position(|p| p["name"] == name) else {
            return Ok(false);
        };
        let entry = registry.remove(index);
        for id in entry["skills"].as_array().into_iter().flatten() {
            if let Some(id) = id.as_str() {
                let _ = crate::skills::delete(&self.store, id);
            }
        }
        let kept: Vec<Value> = self
            .user_templates()
            .into_iter()
            .filter(|t| t["plugin"] != name)
            .collect();
        self.save_templates(&kept)?;
        self.plugin_registry_save(&registry)?;
        Ok(true)
    }

    /// Installa (o aggiorna) un pacchetto. Se qualcosa non è valido non cambia nulla.
    pub fn plugin_install(&mut self, package: &Value) -> Result<Value, CoreError> {
        let catalog = &self.catalog;
        let known = |t: &str| catalog.get(t).is_some();
        let valid = plugins::validate(package, &known).map_err(CoreError::BadRequest)?;
        let name = valid["name"].as_str().unwrap_or_default().to_string();
        let registry = self.plugin_registry();
        let replacing = registry.iter().any(|p| p["name"] == name.as_str());
        if !replacing && registry.len() >= plugins::MAX_PLUGINS {
            return Err(CoreError::BadRequest(format!(
                "Al massimo {} pacchetti installati",
                plugins::MAX_PLUGINS
            )));
        }
        // Il totale dei modelli personali (compresi quelli dei pacchetti) resta entro il limite.
        let others = self
            .user_templates()
            .into_iter()
            .filter(|t| t["plugin"] != name.as_str())
            .count();
        let incoming = valid["plan_templates"].as_array().map_or(0, Vec::len);
        if others + incoming > 30 {
            return Err(CoreError::BadRequest(
                "Con questo pacchetto i modelli personali supererebbero 30".into(),
            ));
        }
        let builtin: Vec<Value> =
            serde_json::from_str(crate::plans::BUILTIN_TEMPLATES).unwrap_or_default();
        for template in valid["plan_templates"].as_array().into_iter().flatten() {
            if builtin.iter().any(|b| b["name"] == template["name"]) {
                return Err(CoreError::BadRequest(
                    "Un modello ha lo stesso nome di uno incorporato".into(),
                ));
            }
        }
        if replacing {
            self.plugin_uninstall(&name)?;
        }
        let mut skill_ids = Vec::new();
        for skill in valid["skills"].as_array().into_iter().flatten() {
            let saved = self.skill_save(None, skill)?;
            skill_ids.push(saved["id"].clone());
        }
        let mut templates = self.user_templates();
        let mut template_names = Vec::new();
        for template in valid["plan_templates"].as_array().into_iter().flatten() {
            template_names.push(template["name"].clone());
            templates.retain(|t| t["name"] != template["name"]);
            templates.push(template.clone());
        }
        self.save_templates(&templates)?;
        let entry = json!({
            "name": name, "version": valid["version"], "description": valid["description"],
            "author": valid["author"], "digest": valid["digest"], "installed_at": now(),
            "skills": skill_ids, "templates": template_names,
        });
        let mut registry = self.plugin_registry();
        registry.push(entry.clone());
        self.plugin_registry_save(&registry)?;
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
            "node": "core-local", "action": "plugin.install",
            "resource": format!("jarvis://plugin/{name}?digest={}", valid["digest"].as_str().unwrap_or_default()),
            "outcome": "succeeded", "effect": "applied"}))?;
        Ok(entry)
    }

    /// Rimuove un pacchetto con tutte le competenze e i modelli che aveva portato.
    pub fn plugin_remove(&mut self, name: &str) -> Result<Value, CoreError> {
        if !self.plugin_uninstall(name)? {
            return Err(CoreError::NotFound("pacchetto non installato".into()));
        }
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
            "node": "core-local", "action": "plugin.remove", "resource": format!("jarvis://plugin/{name}"),
            "outcome": "succeeded", "effect": "applied"}))?;
        Ok(json!({"removed": name}))
    }
}
