//! Esecuzione dei piani (ADR-0152). Un passo alla volta: il Core non resta bloccato durante un piano lungo e
//! ogni passo lascia un checkpoint nel database. Ogni strumento passa dalla policy (con le regole del
//! proprietario e l'arresto di emergenza), dall'audit e, se richiesto, dalla conferma esplicita del proprietario.

use super::*;
use crate::plans;

const MAX_EVENTS: usize = 200;

fn event(doc: &mut Value, step: &str, kind: &str, detail: impl Into<String>) {
    let list = doc["events"].as_array_mut();
    if let Some(list) = list {
        list.push(json!({"at": now(), "step": step, "kind": kind, "detail": detail.into().chars().take(200).collect::<String>()}));
        let extra = list.len().saturating_sub(MAX_EVENTS);
        list.drain(..extra);
    }
}

impl Jarvis {
    fn plan_load(&self, id: &str) -> Result<Value, CoreError> {
        self.store
            .plan_get(id)
            .map_err(internal)?
            .ok_or_else(|| CoreError::NotFound("piano non trovato".into()))
    }

    fn plan_save(&self, doc: &mut Value) -> Result<(), CoreError> {
        doc["updated_at"] = json!(now());
        self.store.plan_put(doc).map_err(internal)
    }

    fn plan_check_tools(&self, doc: &Value) -> Result<(), CoreError> {
        for step in doc["steps"].as_array().into_iter().flatten() {
            for tool in [step["tool"].as_str(), step["undo"]["tool"].as_str()]
                .into_iter()
                .flatten()
            {
                if self.catalog.get(tool).is_none() {
                    return Err(CoreError::BadRequest(format!(
                        "Strumento sconosciuto: «{tool}»"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Crea un piano. Con la stessa `idempotency_key` restituisce il piano già creato invece di duplicarlo.
    pub fn plan_create(&mut self, input: &Value) -> Result<Value, CoreError> {
        let mut doc = plans::validate(input).map_err(CoreError::BadRequest)?;
        self.plan_check_tools(&doc)?;
        if let Some(key) = doc["idempotency_key"].as_str() {
            if let Some(existing) = self.store.plan_by_key(key).map_err(internal)? {
                return Ok(existing);
            }
        }
        doc["id"] = json!(uuid::Uuid::new_v4().simple().to_string());
        doc["created_at"] = json!(now());
        event(&mut doc, "", "created", "Piano creato");
        self.plan_save(&mut doc)?;
        self.record_owner_action("plan.create")?;
        Ok(doc)
    }

    pub fn plan_list(&self) -> Result<Value, CoreError> {
        let items: Vec<Value> = self
            .store
            .plans_recent(30)
            .map_err(internal)?
            .into_iter()
            .map(|d| {
                let steps = d["steps"].as_array().cloned().unwrap_or_default();
                json!({
                    "id": d["id"], "goal": d["goal"], "status": d["status"], "created_at": d["created_at"],
                    "updated_at": d["updated_at"], "steps": steps.len(),
                    "done": steps.iter().filter(|s| s["status"] == "done").count(),
                })
            })
            .collect();
        Ok(json!({"items": items}))
    }

    pub fn plan_get(&self, id: &str) -> Result<Value, CoreError> {
        self.plan_load(id)
    }

    /// Avvia o riprende un piano: da «pending», «interrupted» o «waiting» (dopo un'approvazione) a «running».
    pub fn plan_start(&mut self, id: &str) -> Result<Value, CoreError> {
        let mut doc = self.plan_load(id)?;
        if crate::permissions::emergency_active(&self.cfg.data_dir) {
            return Err(CoreError::Unauthorized(
                policy::explain("policy.emergency_stop").into(),
            ));
        }
        match doc["status"].as_str() {
            Some("pending" | "interrupted" | "waiting") => {
                // Il budget di tempo vale per ogni esecuzione: l'attesa di una conferma non lo consuma.
                doc["started_at"] = json!(now());
                doc["status"] = json!("running");
                event(&mut doc, "", "started", "Esecuzione avviata");
                self.plan_save(&mut doc)?;
                Ok(doc)
            }
            Some("running") => Ok(doc),
            _ => Err(CoreError::BadRequest(
                "Il piano è già concluso: usa il replay per rieseguirlo".into(),
            )),
        }
    }

    /// Approva uno o più passi in attesa di conferma (human-in-the-loop). Solo il proprietario, nell'audit.
    pub fn plan_approve(&mut self, id: &str, steps: &[String]) -> Result<Value, CoreError> {
        let mut doc = self.plan_load(id)?;
        let mut approved = 0;
        if let Some(list) = doc["steps"].as_array_mut() {
            for step in list
                .iter_mut()
                .filter(|s| s["status"] == "waiting_confirmation")
            {
                if steps.is_empty() || steps.iter().any(|s| step["id"] == s.as_str()) {
                    step["approved"] = json!(true);
                    step["status"] = json!("pending");
                    approved += 1;
                }
            }
        }
        if approved == 0 {
            return Err(CoreError::BadRequest(
                "Nessun passo in attesa di conferma corrisponde".into(),
            ));
        }
        event(
            &mut doc,
            "",
            "approved",
            format!("{approved} passi approvati dal proprietario"),
        );
        doc["status"] = json!("running");
        self.plan_save(&mut doc)?;
        self.record_owner_action("plan.approve")?;
        Ok(doc)
    }

    pub fn plan_cancel(&mut self, id: &str) -> Result<Value, CoreError> {
        let mut doc = self.plan_load(id)?;
        if matches!(
            doc["status"].as_str(),
            Some("succeeded" | "failed" | "cancelled" | "budget_exceeded")
        ) {
            return Err(CoreError::BadRequest("Il piano è già concluso".into()));
        }
        if let Some(list) = doc["steps"].as_array_mut() {
            for step in list.iter_mut().filter(|s| {
                matches!(
                    s["status"].as_str(),
                    Some("pending" | "waiting_confirmation")
                )
            }) {
                step["status"] = json!("cancelled");
            }
        }
        doc["status"] = json!("cancelled");
        doc["finished_at"] = json!(now());
        event(&mut doc, "", "cancelled", "Annullato dal proprietario");
        self.plan_save(&mut doc)?;
        self.record_owner_action("plan.cancel")?;
        Ok(doc)
    }

    /// Una copia del piano pronta per essere eseguita di nuovo da zero (replay).
    pub fn plan_replay(&mut self, id: &str) -> Result<Value, CoreError> {
        let doc = self.plan_load(id)?;
        let mut copy = plans::reset(&doc);
        copy["id"] = json!(uuid::Uuid::new_v4().simple().to_string());
        copy["created_at"] = json!(now());
        copy["replay_of"] = json!(id);
        event(
            &mut copy,
            "",
            "created",
            format!("Replay del piano {}", &id[..id.len().min(8)]),
        );
        self.plan_save(&mut copy)?;
        self.record_owner_action("plan.replay")?;
        Ok(copy)
    }

    /// Esegue una chiamata come passo di un piano: policy, audit ed esecuzione. Restituisce stato e risultato.
    fn plan_exec(
        &mut self,
        goal: &str,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
    ) -> Result<(&'static str, Value), CoreError> {
        let task = Task::new(goal, json!({"kind": "owner", "id": "owner"}));
        let version = self
            .catalog
            .get(tool)
            .map(|t| t.version().to_string())
            .unwrap_or_default();
        let digest =
            canonical_digest(&json!({"tool": tool, "version": version, "arguments": arguments}));
        let (tid, corr) = (task.id(), task.correlation());
        let decision = policy::decide(
            &self.catalog,
            &self.cfg.web,
            &policy::Request {
                subject: json!({"kind": "agent", "id": "planner"}),
                task_id: &tid,
                correlation_id: &corr,
                tool,
                version: &version,
                arguments,
                digest: &digest,
                perms: &crate::permissions::Rules::load(&self.cfg.data_dir),
            },
        );
        // «require_confirmation» passa solo se il proprietario ha approvato esplicitamente il passo.
        let allowed = decision["decision"] == "allow"
            || (decision["decision"] == "require_confirmation" && confirmed);
        self.audit(json!({
            "correlation_id": corr, "task_id": tid, "actor": {"kind": "agent", "id": "planner"}, "node": "core-local",
            "action": decision["request"]["action"]["tool"], "decision_id": decision["decision_id"],
            "outcome": if allowed { "allowed" } else { "denied" },
        }))?;
        if !allowed {
            let code = decision["reasons"][0].as_str().unwrap_or("policy.denied");
            return Ok((
                "denied",
                json!({"code": code, "message": policy::explain(code)}),
            ));
        }
        let mut out = Outcome::default();
        self.run_tool(&task, tool, arguments, confirmed, &mut out)
    }

    fn requires_confirmation(&self, tool: &str, rules: &crate::permissions::Rules) -> bool {
        let Some(t) = self.catalog.get(tool) else {
            return false;
        };
        t.needs_confirmation()
            || rules.verdict(
                tool,
                t.descriptor["effects"].as_str().unwrap_or("write_local"),
            ) == Some(crate::permissions::Mode::Ask)
    }

    /// Esegue al più un passo del piano e ne salva il checkpoint. Restituisce il piano aggiornato.
    pub fn plan_step(&mut self, id: &str) -> Result<Value, CoreError> {
        let mut doc = self.plan_load(id)?;
        if doc["status"] != "running" {
            return Ok(doc);
        }
        if crate::permissions::emergency_active(&self.cfg.data_dir) {
            doc["status"] = json!("interrupted");
            event(
                &mut doc,
                "",
                "emergency",
                "Arresto di emergenza: piano interrotto, riprendibile dopo la disattivazione",
            );
            self.plan_save(&mut doc)?;
            return Ok(doc);
        }
        let started = doc["started_at"].as_str().and_then(automation::parse_ts);
        let max_seconds = doc["budget"]["max_seconds"]
            .as_i64()
            .unwrap_or(plans::DEFAULT_SECONDS as i64);
        let over_time =
            started.is_some_and(|s| (chrono::Utc::now() - s).num_seconds() > max_seconds);
        let over_steps = doc["steps_executed"].as_u64().unwrap_or(0)
            >= doc["budget"]["max_steps"].as_u64().unwrap_or(u64::MAX);
        if over_time || over_steps {
            cancel_pending(&mut doc);
            doc["status"] = json!("budget_exceeded");
            doc["finished_at"] = json!(now());
            event(
                &mut doc,
                "",
                "budget",
                if over_time {
                    "Tempo massimo superato"
                } else {
                    "Numero massimo di passi eseguiti"
                },
            );
            self.plan_save(&mut doc)?;
            return Ok(doc);
        }
        plans::skip_blocked(&mut doc);
        let Some(index) = plans::next_ready(&doc) else {
            let status = plans::final_status(&doc);
            doc["status"] = json!(status);
            if status != "running" && status != "waiting" {
                doc["finished_at"] = json!(now());
                event(
                    &mut doc,
                    "",
                    "finished",
                    format!("Piano concluso: {status}"),
                );
            }
            self.plan_save(&mut doc)?;
            return Ok(doc);
        };
        let step = doc["steps"][index].clone();
        let step_id = step["id"].as_str().unwrap_or_default().to_string();
        let tool = step["tool"].as_str().unwrap_or_default().to_string();
        let rules = crate::permissions::Rules::load(&self.cfg.data_dir);
        let approved = step["approved"] == true;
        if self.requires_confirmation(&tool, &rules) && !approved {
            doc["steps"][index]["status"] = json!("waiting_confirmation");
            doc["status"] = json!("waiting");
            event(
                &mut doc,
                &step_id,
                "waiting_confirmation",
                format!("{tool} richiede la conferma del proprietario"),
            );
            self.plan_save(&mut doc)?;
            return Ok(doc);
        }
        let arguments = match plans::resolve(&step["arguments"], &doc) {
            Ok(arguments) => arguments,
            Err(message) => {
                finish_step(
                    &mut doc,
                    index,
                    "failed",
                    json!({"code": "plan.bad_reference", "message": message}),
                );
                event(&mut doc, &step_id, "failed", "Riferimento non risolvibile");
                after_failure(&mut doc, index);
                self.plan_save(&mut doc)?;
                return Ok(doc);
            }
        };
        doc["steps"][index]["status"] = json!("running");
        if doc["steps"][index]["started_at"].is_null() {
            doc["steps"][index]["started_at"] = json!(now());
        }
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        let (status, output) = self.plan_exec(&goal, &tool, &arguments, approved)?;
        doc["steps_executed"] = json!(doc["steps_executed"].as_u64().unwrap_or(0) + 1);
        let attempts = doc["steps"][index]["attempts"].as_u64().unwrap_or(0) + 1;
        doc["steps"][index]["attempts"] = json!(attempts);
        if status == "succeeded" {
            finish_step(&mut doc, index, "done", plans::clip_output(&output));
            event(&mut doc, &step_id, "done", format!("{tool} eseguito"));
        } else if status != "denied" && attempts <= step["retries"].as_u64().unwrap_or(0) {
            doc["steps"][index]["status"] = json!("pending");
            doc["steps"][index]["error"] = output.clone();
            event(
                &mut doc,
                &step_id,
                "retry",
                format!("Tentativo {attempts} non riuscito: nuovo tentativo"),
            );
        } else {
            finish_step(&mut doc, index, "failed", output.clone());
            event(
                &mut doc,
                &step_id,
                "failed",
                output["code"].as_str().unwrap_or("errore").to_string(),
            );
            after_failure(&mut doc, index);
        }
        plans::skip_blocked(&mut doc);
        if plans::next_ready(&doc).is_none() {
            let status = plans::final_status(&doc);
            doc["status"] = json!(status);
            if status != "running" && status != "waiting" {
                doc["finished_at"] = json!(now());
                event(
                    &mut doc,
                    "",
                    "finished",
                    format!("Piano concluso: {status}"),
                );
            }
        }
        self.plan_save(&mut doc)?;
        Ok(doc)
    }

    /// Annulla gli effetti dei passi riusciti che dichiarano un'operazione inversa, dall'ultimo al primo.
    /// Con `confirm` il proprietario approva le operazioni inverse che richiedono conferma.
    pub fn plan_rollback(&mut self, id: &str, confirm: bool) -> Result<Value, CoreError> {
        let mut doc = self.plan_load(id)?;
        if doc["status"] == "running" {
            return Err(CoreError::BadRequest(
                "Interrompi o attendi il piano prima di annullarne gli effetti".into(),
            ));
        }
        let rules = crate::permissions::Rules::load(&self.cfg.data_dir);
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        let order: Vec<usize> = doc["steps"]
            .as_array()
            .map(|s| {
                s.iter()
                    .enumerate()
                    .filter(|(_, s)| s["status"] == "done" && s["undo"].is_object())
                    .map(|(i, _)| i)
                    .rev()
                    .collect()
            })
            .unwrap_or_default();
        let mut report = Vec::new();
        for index in order {
            let step_id = doc["steps"][index]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let undo = doc["steps"][index]["undo"].clone();
            let tool = undo["tool"].as_str().unwrap_or_default().to_string();
            if self.requires_confirmation(&tool, &rules) && !confirm {
                report.push(json!({"step": step_id, "status": "needs_confirmation", "tool": tool}));
                continue;
            }
            let arguments = plans::resolve(&undo["arguments"], &doc)
                .unwrap_or_else(|_| undo["arguments"].clone());
            let (status, output) = self.plan_exec(&goal, &tool, &arguments, confirm)?;
            if status == "succeeded" {
                doc["steps"][index]["status"] = json!("rolled_back");
                event(
                    &mut doc,
                    &step_id,
                    "rolled_back",
                    format!("{tool} eseguito"),
                );
            } else {
                event(
                    &mut doc,
                    &step_id,
                    "rollback_failed",
                    output["code"].as_str().unwrap_or("errore").to_string(),
                );
            }
            report.push(json!({"step": step_id, "status": status, "tool": tool}));
        }
        self.plan_save(&mut doc)?;
        self.record_owner_action("plan.rollback")?;
        Ok(json!({"plan": doc, "rollback": report}))
    }
}

fn cancel_pending(doc: &mut Value) {
    if let Some(list) = doc["steps"].as_array_mut() {
        for step in list.iter_mut().filter(|s| {
            matches!(
                s["status"].as_str(),
                Some("pending" | "waiting_confirmation")
            )
        }) {
            step["status"] = json!("cancelled");
        }
    }
}

fn finish_step(doc: &mut Value, index: usize, status: &str, output: Value) {
    doc["steps"][index]["status"] = json!(status);
    doc["steps"][index]["finished_at"] = json!(now());
    if status == "done" {
        doc["steps"][index]["output"] = output;
        doc["steps"][index]["error"] = Value::Null;
    } else {
        doc["steps"][index]["error"] = output;
    }
}

/// Dopo un fallimento definitivo: con `on_error = stop` il resto del piano non parte.
fn after_failure(doc: &mut Value, index: usize) {
    if doc["steps"][index]["on_error"] == "stop" {
        cancel_pending(doc);
    }
}
