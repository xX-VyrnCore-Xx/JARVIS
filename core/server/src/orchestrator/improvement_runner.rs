//! Auto-miglioramento controllato (ADR-0163): suggerimenti calcolati dalle evidenze locali e un'unica azione
//! applicabile, approvata dal proprietario: salvare come procedura un piano ricorrente.

use super::*;
use crate::improvements;

const FILE: &str = "improvements.json";

impl Jarvis {
    fn improvements_dismissed(&self) -> Vec<String> {
        std::fs::read(self.cfg.data_dir.join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["dismissed"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    fn improvement_list(&self) -> Result<Vec<Value>, CoreError> {
        let plans = self.store.plans_recent(200).map_err(internal)?;
        let tools = self.store.tool_stats().map_err(internal)?;
        let agents = self.store.agent_stats().map_err(internal)?;
        Ok(improvements::analyze(
            &plans,
            &tools,
            &agents,
            &self.user_templates(),
            &self.improvements_dismissed(),
        ))
    }

    /// Suggerimenti attuali. Non cambiano nulla: servono a decidere.
    pub fn improvements(&self) -> Result<Value, CoreError> {
        let items = self.improvement_list()?;
        Ok(json!({
            "items": items,
            "policy": "JARVIS non modifica da solo codice, regole di sicurezza, permessi o chiavi: propone e tu decidi.",
        }))
    }

    /// Applica un suggerimento che ha un'azione (salvare una procedura). Gli altri sono solo da valutare.
    pub fn improvement_apply(&mut self, id: &str) -> Result<Value, CoreError> {
        let items = self.improvement_list()?;
        let item = items
            .iter()
            .find(|s| s["id"] == id)
            .ok_or_else(|| CoreError::NotFound("suggerimento non trovato".into()))?;
        let action = &item["action"];
        if action["type"] != "save_template" {
            return Err(CoreError::BadRequest(
                "Questo suggerimento non ha un'azione automatica: è da valutare a mano.".into(),
            ));
        }
        let template = self.plan_template_save(&json!({
            "plan_id": action["plan_id"],
            "name": action["name"],
            "description": "Procedura salvata dai piani ricorrenti",
        }))?;
        self.record_owner_action("improvement.apply")?;
        Ok(json!({"applied": id, "template": template}))
    }

    /// Nasconde un suggerimento (non viene riproposto).
    pub fn improvement_dismiss(&mut self, id: &str) -> Result<Value, CoreError> {
        let mut dismissed = self.improvements_dismissed();
        if !dismissed.iter().any(|d| d == id) {
            dismissed.push(id.to_string());
        }
        let extra = dismissed.len().saturating_sub(improvements::MAX_DISMISSED);
        dismissed.drain(..extra);
        std::fs::write(
            self.cfg.data_dir.join(FILE),
            json!({"dismissed": dismissed}).to_string(),
        )
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        self.record_owner_action("improvement.dismiss")?;
        Ok(json!({"dismissed": id}))
    }
}
