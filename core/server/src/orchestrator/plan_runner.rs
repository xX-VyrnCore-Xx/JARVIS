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

    /// Richiesta al modello per proporre un piano (ADR-0160). `None` se non c'è alcun modello cloud configurato.
    pub fn plan_propose_request(
        &self,
        goal: &str,
    ) -> Result<Option<crate::cloud::AgentRequest>, CoreError> {
        self.plan_model_request(goal, None)
    }

    /// Richiesta di ripianificazione (ADR-0161) per un piano fermo.
    pub fn plan_replan_request(
        &self,
        id: &str,
    ) -> Result<Option<crate::cloud::AgentRequest>, CoreError> {
        let doc = self.plan_load(id)?;
        let stopped = matches!(
            doc["status"].as_str(),
            Some("failed" | "cancelled" | "interrupted")
        );
        let broken = doc["steps"].as_array().is_some_and(|s| {
            s.iter()
                .any(|x| matches!(x["status"].as_str(), Some("failed" | "skipped")))
        });
        if !stopped && !broken {
            return Err(CoreError::BadRequest(
                "Il piano non si è fermato: si ripianifica solo un piano fallito, annullato o interrotto.".into(),
            ));
        }
        if doc["replan_depth"].as_u64().unwrap_or(0) >= crate::planner::MAX_REPLANS {
            return Err(CoreError::BadRequest(
                "Questo piano è già stato ripianificato più volte: decidi tu come proseguire."
                    .into(),
            ));
        }
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        self.plan_model_request(&goal, Some(&doc))
    }

    fn plan_model_request(
        &self,
        goal: &str,
        stopped: Option<&Value>,
    ) -> Result<Option<crate::cloud::AgentRequest>, CoreError> {
        if crate::permissions::emergency_active(&self.cfg.data_dir) {
            return Err(CoreError::Unavailable(
                "Arresto di emergenza attivo: non propongo piani finché non lo disattivi.".into(),
            ));
        }
        let goal = goal.trim();
        if goal.is_empty() || goal.chars().count() > 300 {
            return Err(CoreError::BadRequest(
                "«goal» è obbligatorio (massimo 300 caratteri)".into(),
            ));
        }
        if crate::cloud::contains_credentials(goal) {
            return Err(CoreError::BadRequest(
                "L'obiettivo sembra contenere una credenziale: non lo invio al modello.".into(),
            ));
        }
        let Some((provider, model, key)) = self.mission_provider(goal)? else {
            return Ok(None);
        };
        let tools: Vec<crate::planner::ToolInfo> = self
            .catalog
            .iter()
            .filter(|t| {
                let n = t.name();
                !n.starts_with("mission.") && !n.starts_with("plan.")
            })
            .map(|t| {
                let schema = &t.descriptor["input_schema"];
                let names = |v: &Value| -> Vec<String> {
                    v.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                };
                let mut properties: Vec<String> = schema["properties"]
                    .as_object()
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                properties.sort();
                crate::planner::ToolInfo {
                    name: t.name().to_string(),
                    summary: t.descriptor["summary"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    required: names(&schema["required"]),
                    properties,
                    risk: t.risk().to_string(),
                }
            })
            .collect();
        let (system, ask) = match stopped {
            Some(doc) => {
                let report = crate::planner::replan_report(doc);
                if crate::cloud::contains_credentials(&report) {
                    return Err(CoreError::BadRequest(
                        "Il piano contiene una credenziale: non lo invio al modello.".into(),
                    ));
                }
                (crate::planner::replan_system_prompt(&tools), report)
            }
            None => (crate::planner::system_prompt(&tools), goal.to_string()),
        };
        Ok(Some(crate::cloud::AgentRequest::new(
            provider,
            model,
            key,
            system,
            ask,
            Vec::new(),
            Vec::new(),
            self.cloud_limiter.clone(),
        )))
    }

    /// Chiude la proposta: valida la risposta del modello. Con `create` salva la bozza (non la avvia).
    pub fn plan_propose_finish(
        &mut self,
        goal: &str,
        answer: Result<crate::cloud::Answer, crate::cloud::Failure>,
        provider: crate::cloud::Provider,
        create: bool,
    ) -> Result<Value, CoreError> {
        self.plan_finish(goal, answer, provider, create, None)
    }

    /// Chiude la ripianificazione di `id`: il piano di recupero è una nuova bozza collegata all'originale.
    pub fn plan_replan_finish(
        &mut self,
        id: &str,
        answer: Result<crate::cloud::Answer, crate::cloud::Failure>,
        provider: crate::cloud::Provider,
        create: bool,
    ) -> Result<Value, CoreError> {
        let parent = self.plan_load(id)?;
        let depth = parent["replan_depth"].as_u64().unwrap_or(0) + 1;
        let goal = parent["goal"].as_str().unwrap_or_default().to_string();
        self.plan_finish(
            &goal,
            answer,
            provider,
            create,
            Some((id.to_string(), depth)),
        )
    }

    fn plan_finish(
        &mut self,
        goal: &str,
        answer: Result<crate::cloud::Answer, crate::cloud::Failure>,
        provider: crate::cloud::Provider,
        create: bool,
        parent: Option<(String, u64)>,
    ) -> Result<Value, CoreError> {
        let text = match answer {
            Ok(crate::cloud::Answer::Text(t)) => t,
            Ok(crate::cloud::Answer::Call(_)) => {
                return Err(CoreError::BadRequest(
                    "Il modello ha risposto con una chiamata invece che con un piano.".into(),
                ))
            }
            Err(failure) => return Err(CoreError::Unavailable(failure.user_message(provider).0)),
        };
        let catalog = &self.catalog;
        let known = |t: &str| {
            catalog.get(t).is_some() && !t.starts_with("mission.") && !t.starts_with("plan.")
        };
        let risk = |t: &str| {
            catalog
                .get(t)
                .map(|x| x.risk().to_string())
                .unwrap_or_default()
        };
        let proposal =
            crate::planner::proposal(&text, goal, &known, &risk).map_err(CoreError::BadRequest)?;
        self.record_owner_action(if parent.is_some() {
            "plan.replan"
        } else {
            "plan.propose"
        })?;
        let mut proposal = proposal;
        if let Some((from, depth)) = &parent {
            proposal["replan_of"] = json!(from);
            proposal["replan_depth"] = json!(depth);
        }
        if !create {
            return Ok(json!({"created": false, "proposal": proposal}));
        }
        let mut input = proposal.clone();
        for key in [
            "proposed_by",
            "max_risk",
            "risk_by_step",
            "replan_of",
            "replan_depth",
        ] {
            input.as_object_mut().map(|o| o.remove(key));
        }
        let mut doc = self.plan_create(&input)?;
        doc["proposed_by"] = proposal["proposed_by"].clone();
        doc["max_risk"] = proposal["max_risk"].clone();
        if let Some((from, depth)) = &parent {
            doc["replan_of"] = json!(from);
            doc["replan_depth"] = json!(depth);
            event(
                &mut doc,
                "",
                "replanned",
                format!("Ripianificazione del piano {from}"),
            );
        }
        self.plan_save(&mut doc)?;
        Ok(json!({"created": true, "proposal": proposal, "plan": doc}))
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

    pub(super) fn user_templates(&self) -> Vec<Value> {
        std::fs::read(self.cfg.data_dir.join("plan-templates.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["templates"].as_array().cloned())
            .unwrap_or_default()
    }

    /// Modelli di piano: incorporati e quelli salvati dal proprietario (procedure approvate).
    pub fn plan_templates(&self) -> Value {
        let mut items: Vec<Value> = serde_json::from_str::<Vec<Value>>(plans::BUILTIN_TEMPLATES)
            .unwrap_or_default()
            .into_iter()
            .map(|mut t| {
                t["builtin"] = json!(true);
                t
            })
            .collect();
        items.extend(self.user_templates().into_iter().map(|mut t| {
            t["builtin"] = json!(false);
            t
        }));
        json!({"items": items})
    }

    /// Crea un piano da un modello e dai parametri.
    pub fn plan_from_template(&mut self, body: &Value) -> Result<Value, CoreError> {
        let name = body["template"].as_str().unwrap_or_default();
        let templates = self.plan_templates();
        let template = templates["items"]
            .as_array()
            .and_then(|l| l.iter().find(|t| t["name"] == name))
            .ok_or_else(|| CoreError::NotFound(format!("Modello «{name}» non trovato")))?;
        let mut plan =
            plans::instantiate(template, &body["params"]).map_err(CoreError::BadRequest)?;
        plan["idempotency_key"] = body["idempotency_key"].clone();
        self.plan_create(&plan)
    }

    /// Salva un piano come procedura riutilizzabile. Solo il proprietario: nulla viene appreso da solo.
    pub fn plan_template_save(&mut self, body: &Value) -> Result<Value, CoreError> {
        let plan = self.plan_load(body["plan_id"].as_str().unwrap_or_default())?;
        let template = plans::template_from_plan(
            &plan,
            body["name"].as_str().unwrap_or_default(),
            body["description"].as_str().unwrap_or_default(),
        )
        .map_err(CoreError::BadRequest)?;
        let name = template["name"].as_str().unwrap_or_default().to_string();
        let builtin: Vec<Value> =
            serde_json::from_str(plans::BUILTIN_TEMPLATES).unwrap_or_default();
        if builtin.iter().any(|t| t["name"] == name.as_str()) {
            return Err(CoreError::BadRequest(
                "Il nome è di un modello incorporato".into(),
            ));
        }
        let mut saved: Vec<Value> = self
            .user_templates()
            .into_iter()
            .filter(|t| t["name"] != name.as_str())
            .collect();
        if saved.len() >= 30 {
            return Err(CoreError::BadRequest(
                "Al massimo 30 modelli personali".into(),
            ));
        }
        saved.push(template.clone());
        std::fs::write(
            self.cfg.data_dir.join("plan-templates.json"),
            json!({"templates": saved}).to_string(),
        )
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        self.record_owner_action("plan.template.save")?;
        Ok(template)
    }

    pub fn plan_template_delete(&mut self, name: &str) -> Result<Value, CoreError> {
        let before = self.user_templates();
        let kept: Vec<Value> = before
            .iter()
            .filter(|t| t["name"] != name)
            .cloned()
            .collect();
        if kept.len() == before.len() {
            return Err(CoreError::NotFound("Modello personale non trovato".into()));
        }
        std::fs::write(
            self.cfg.data_dir.join("plan-templates.json"),
            json!({"templates": kept}).to_string(),
        )
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        self.record_owner_action("plan.template.delete")?;
        Ok(self.plan_templates())
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
        if let Err(denied) = self.plan_authorize(&task, tool, arguments, confirmed)? {
            return Ok(("denied", denied));
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
        let batch = self.parallel_batch(&doc, index);
        if batch.len() >= 2 {
            self.plan_run_batch(&mut doc, &batch)?;
        } else if self.plan_run_single(&mut doc, index)? {
            // Il piano si è fermato in attesa di conferma: già salvato.
            return Ok(doc);
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

    /// Passi pronti che possono girare insieme al primo: indipendenti, senza stato e senza conferma da attendere.
    fn parallel_batch(&self, doc: &Value, first: usize) -> Vec<usize> {
        let rules = crate::permissions::Rules::load(&self.cfg.data_dir);
        let eligible = |i: usize| -> bool {
            let step = &doc["steps"][i];
            let tool = step["tool"].as_str().unwrap_or_default();
            crate::toolsys::parallel_safe(tool)
                && (!self.requires_confirmation(tool, &rules) || step["approved"] == true)
        };
        if !eligible(first) {
            return Vec::new();
        }
        let mut batch = vec![first];
        let steps = doc["steps"].as_array().cloned().unwrap_or_default();
        for (i, step) in steps.iter().enumerate() {
            if i == first || batch.len() >= plans::MAX_PARALLEL || step["status"] != "pending" {
                continue;
            }
            let ready = step["depends_on"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .all(|d| steps.iter().any(|s| s["id"] == d && s["status"] == "done"));
            if ready && eligible(i) {
                batch.push(i);
            }
        }
        batch
    }

    /// Autorizza una chiamata (policy e audit) senza eseguirla. `Err` porta il risultato di un rifiuto.
    fn plan_authorize(
        &mut self,
        task: &Task,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
    ) -> Result<Result<(), Value>, CoreError> {
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
        if allowed {
            Ok(Ok(()))
        } else {
            let code = decision["reasons"][0].as_str().unwrap_or("policy.denied");
            Ok(Err(json!({"code": code, "message": policy::explain(code)})))
        }
    }

    /// Esegue un solo passo. Restituisce `true` se il piano si è fermato (attesa di conferma) ed è già salvato.
    fn plan_run_single(&mut self, doc: &mut Value, index: usize) -> Result<bool, CoreError> {
        let step = doc["steps"][index].clone();
        let step_id = step["id"].as_str().unwrap_or_default().to_string();
        let tool = step["tool"].as_str().unwrap_or_default().to_string();
        let rules = crate::permissions::Rules::load(&self.cfg.data_dir);
        let approved = step["approved"] == true;
        if self.requires_confirmation(&tool, &rules) && !approved {
            doc["steps"][index]["status"] = json!("waiting_confirmation");
            doc["status"] = json!("waiting");
            event(
                doc,
                &step_id,
                "waiting_confirmation",
                format!("{tool} richiede la conferma del proprietario"),
            );
            self.plan_save(doc)?;
            return Ok(true);
        }
        let arguments = match plans::resolve(&step["arguments"], doc) {
            Ok(arguments) => arguments,
            Err(message) => {
                apply_result(
                    doc,
                    index,
                    "denied",
                    json!({"code": "plan.bad_reference", "message": message}),
                );
                return Ok(false);
            }
        };
        doc["steps"][index]["status"] = json!("running");
        if doc["steps"][index]["started_at"].is_null() {
            doc["steps"][index]["started_at"] = json!(now());
        }
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        let (status, output) = self.plan_exec(&goal, &tool, &arguments, approved)?;
        apply_result(doc, index, status, output);
        Ok(false)
    }

    /// Esegue insieme più passi indipendenti e senza stato (ADR-0154). Policy e audit restano sequenziali e
    /// prima dell'esecuzione; ogni strumento gira in un thread con un archivio privato e vuoto.
    fn plan_run_batch(&mut self, doc: &mut Value, batch: &[usize]) -> Result<(), CoreError> {
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        let mut prepared: Vec<(usize, String, Value, Task)> = Vec::new();
        for &index in batch {
            let step = doc["steps"][index].clone();
            let tool = step["tool"].as_str().unwrap_or_default().to_string();
            let arguments = match plans::resolve(&step["arguments"], doc) {
                Ok(arguments) => arguments,
                Err(message) => {
                    apply_result(
                        doc,
                        index,
                        "denied",
                        json!({"code": "plan.bad_reference", "message": message}),
                    );
                    continue;
                }
            };
            let task = Task::new(&goal, json!({"kind": "owner", "id": "owner"}));
            match self.plan_authorize(&task, &tool, &arguments, step["approved"] == true)? {
                Ok(()) => {
                    doc["steps"][index]["status"] = json!("running");
                    if doc["steps"][index]["started_at"].is_null() {
                        doc["steps"][index]["started_at"] = json!(now());
                    }
                    prepared.push((index, tool, arguments, task));
                }
                Err(denied) => apply_result(doc, index, "denied", denied),
            }
        }
        if prepared.is_empty() {
            return Ok(());
        }
        let workspace = self.cfg.workspace();
        let web = self.cfg.web.clone();
        let started = Timer::start();
        let results: Vec<(Result<Value, tools::ToolError>, f64)> = std::thread::scope(|scope| {
            let handles: Vec<_> = prepared
                .iter()
                .map(|(_, tool, arguments, _)| {
                    let (workspace, web) = (&workspace, &web);
                    scope.spawn(move || {
                        let timer = Timer::start();
                        let result = match Store::open(std::path::Path::new(":memory:")) {
                            Ok(scratch) => tools::execute(
                                tool,
                                arguments,
                                &tools::Context {
                                    workspace,
                                    store: &scratch,
                                    web,
                                },
                            ),
                            Err(error) => {
                                Err(tool_error("plan.scratch_unavailable", error.to_string()))
                            }
                        };
                        (result, timer.ms())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        (
                            Err(tool_error("plan.worker_panic", "esecuzione interrotta")),
                            0.0,
                        )
                    })
                })
                .collect()
        });
        event(
            doc,
            "",
            "parallel",
            format!(
                "{} passi eseguiti insieme in {:.0} ms",
                prepared.len(),
                started.ms()
            ),
        );
        for ((index, tool, _, task), (result, ms)) in prepared.into_iter().zip(results) {
            self.obs
                .observe(&format!("tool {tool}"), ms, result.is_err());
            let _ = self.store.tool_run_add(
                &tool,
                &now(),
                ms,
                result.is_ok(),
                result.as_ref().err().map(|e| e.code),
            );
            let base = json!({"correlation_id": task.correlation(), "task_id": task.id(),
                "actor": {"kind": "module", "id": "core"}, "node": "core-local", "action": tool});
            let (status, output) = match result {
                Ok(output) => {
                    self.audit(merge(
                        &base,
                        json!({"outcome": "succeeded", "effect": "applied"}),
                    ))?;
                    ("succeeded", output)
                }
                Err(e) => {
                    let effect = self.failure_effect(&tool, e.code);
                    self.audit(merge(
                        &base,
                        json!({"outcome": "failed", "effect": effect, "error_code": e.code}),
                    ))?;
                    (
                        "failed",
                        json!({"code": e.code, "message": e.message, "effect": effect}),
                    )
                }
            };
            apply_result(doc, index, status, output);
        }
        Ok(())
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

/// Registra l'esito di un passo: conteggi, controllo delle condizioni di successo, ripetizione o fallimento.
fn apply_result(doc: &mut Value, index: usize, status: &str, output: Value) {
    let step_id = doc["steps"][index]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let tool = doc["steps"][index]["tool"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    doc["steps_executed"] = json!(doc["steps_executed"].as_u64().unwrap_or(0) + 1);
    let attempts = doc["steps"][index]["attempts"].as_u64().unwrap_or(0) + 1;
    doc["steps"][index]["attempts"] = json!(attempts);
    let retries = doc["steps"][index]["retries"].as_u64().unwrap_or(0);
    // Un risultato che non soddisfa le condizioni dichiarate vale come un errore del passo.
    let (status, output) = if status == "succeeded" {
        match plans::check_expect(&output, &doc["steps"][index]["expect"]) {
            Ok(()) => (status, output),
            Err(message) => (
                "failed",
                json!({"code": "plan.expectation_failed", "message": message}),
            ),
        }
    } else {
        (status, output)
    };
    if status == "succeeded" {
        finish_step(doc, index, "done", plans::clip_output(&output));
        event(doc, &step_id, "done", format!("{tool} eseguito"));
    } else if status != "denied" && attempts <= retries {
        doc["steps"][index]["status"] = json!("pending");
        doc["steps"][index]["error"] = output;
        event(
            doc,
            &step_id,
            "retry",
            format!("Tentativo {attempts} non riuscito: nuovo tentativo"),
        );
    } else {
        let code = output["code"].as_str().unwrap_or("errore").to_string();
        finish_step(doc, index, "failed", output);
        event(doc, &step_id, "failed", code);
        after_failure(doc, index);
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
