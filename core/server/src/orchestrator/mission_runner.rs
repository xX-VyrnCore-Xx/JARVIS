//! Esecuzione delle missioni autonome (ADR-0096).
//!
//! Con una chiave cloud il modello guida l'agente passo per passo: ogni richiesta al provider avviene fuori dal
//! blocco del Core (come le chat), ogni strumento rientra nel Core e passa da ambito, policy e audit. Senza
//! modello cloud la missione usa il worker locale a regole. In entrambi i casi gli strumenti sono solo quelli
//! senza conferma e a rischio non alto.

use super::*;
use crate::cloud::{AgentRequest, AgentStep, Answer, Failure, Function, Provider};
use crate::missions::{self, RunReport};

/// Strumenti di scrittura locale ammessi alle missioni: restano visibili al proprietario (note e promemoria).
const WRITE_TOOLS: [&str; 2] = ["notes.add", "todo.add"];
/// Dimensione massima del risultato di uno strumento inviato al modello.
const LATEST_CHARS: usize = 4000;
const EARLIER_CHARS: usize = 1200;

/// Esecuzione in corso di una missione. Vive tra una chiamata al Core e la successiva.
pub struct MissionRun {
    doc: Value,
    task: Task,
    history: Vec<Value>,
    out: Outcome,
    cloud: Option<(Provider, String, String)>,
    started: Timer,
}

impl MissionRun {
    /// Vero se la missione è guidata da un modello cloud.
    pub fn uses_cloud(&self) -> bool {
        self.cloud.is_some()
    }

    pub fn id(&self) -> &str {
        self.doc["id"].as_str().unwrap_or_default()
    }
}

/// Seguito di una missione guidata dal cloud.
pub enum Next {
    /// Richiesta da eseguire fuori dal blocco; la risposta torna a `mission_advance`.
    Ask(Box<AgentRequest>),
    /// Missione registrata: documento aggiornato.
    Done(Value),
}

impl Jarvis {
    /// Strumenti che una missione può usare: nessuna conferma, rischio non alto, nessuna delega né mission.*;
    /// solo lettura (più note e promemoria). I connettori solo se il proprietario li ha consentiti.
    pub(super) fn mission_tools(&self, connectors: bool) -> Vec<String> {
        self.catalog
            .iter()
            .filter(|t| {
                let name = t.name();
                !t.needs_confirmation()
                    && !matches!(t.risk(), "high" | "critical")
                    && name != "agent.delegate"
                    && name != crate::desktop::TOOL
                    && !name.starts_with("mission.")
                    && (connectors
                        || !(name.starts_with("connector.") || name.starts_with("node.")))
                    && (matches!(t.descriptor["effects"].as_str(), Some("read" | "none"))
                        || WRITE_TOOLS.contains(&name))
            })
            .map(|t| t.name().to_string())
            .collect()
    }

    /// Provider cloud da usare: quello predefinito se ha una chiave, altrimenti il primo configurato.
    fn mission_provider(
        &self,
        goal: &str,
    ) -> Result<Option<(Provider, String, String)>, CoreError> {
        let settings = crate::cloud::settings(&self.cfg.data_dir);
        let mut found = Vec::new();
        for provider in Provider::ALL {
            if let Some(key) = self.secrets.get(&provider.secret()).map_err(internal)? {
                found.push(provider.target(key, goal, Vec::new(), &settings));
            }
        }
        found.sort_by_key(|t| settings["default_provider"] != t.provider.id());
        Ok(found
            .into_iter()
            .next()
            .map(|t| (t.provider, t.model, t.key)))
    }

    /// Missione attiva con l'esecuzione scaduta, se ce n'è una.
    pub fn mission_due(&self, now_t: chrono::DateTime<chrono::Utc>) -> Option<String> {
        self.store
            .missions_due(&automation::ts(now_t), 1)
            .ok()
            .and_then(|m| m.first().and_then(|d| d["id"].as_str().map(str::to_string)))
    }

    /// Prepara un'esecuzione: prenota la missione, crea il task e registra l'agente isolato.
    pub fn mission_begin(&mut self, id: &str) -> Result<Option<MissionRun>, CoreError> {
        let Some((doc, _)) = self.store.mission_get(id).map_err(internal)? else {
            return Err(CoreError::NotFound("missione inesistente".into()));
        };
        if doc["status"] != "active" {
            return Ok(None);
        }
        let lease = missions::lease(chrono::Utc::now());
        self.store
            .mission_put(&doc, Some(&lease))
            .map_err(internal)?;
        let goal = doc["goal"].as_str().unwrap_or_default().to_string();
        let mut task = Task::new(&goal, json!({"kind": "agent", "id": "general_assistant"}));
        task.doc["success_conditions"] = json!([
            "Rapporto della missione prodotto",
            "Ogni strumento eseguito ha superato la policy"
        ]);
        task.transition("planned", &format!("Missione {id}"));
        task.transition("running", "Missione in esecuzione");
        self.save(&task)?;
        self.temporary.insert(
            task.id(),
            TempAgent {
                role: "Missione autonoma".into(),
                tools: self.mission_tools(doc["connectors"] == true),
                steps: MISSION_STEPS,
                actor: "mission",
            },
        );
        let cloud = self.mission_provider(&goal)?;
        self.obs.count("mission.run");
        Ok(Some(MissionRun {
            doc,
            task,
            history: Vec::new(),
            out: Outcome::default(),
            cloud,
            started: Timer::start(),
        }))
    }

    /// Risultato di uno strumento nella forma inviata al modello: ridotto e senza credenziali.
    fn mission_step_text(entry: &Value, latest: bool) -> String {
        let raw = json!({"status": entry["status"], "output": entry["output"]}).to_string();
        let limit = if latest { LATEST_CHARS } else { EARLIER_CHARS };
        let text = clip(&raw, limit);
        if crate::cloud::contains_credentials(&text) {
            "{\"status\":\"omesso\",\"output\":\"risultato non inviato al modello: contiene possibili credenziali\"}"
                .into()
        } else {
            text
        }
    }

    fn mission_request(&self, run: &MissionRun) -> Result<Option<AgentRequest>, CoreError> {
        let Some((provider, model, key)) = &run.cloud else {
            return Ok(None);
        };
        let allows = self.task_scope(&run.task);
        let last = run.history.len() >= MISSION_STEPS;
        let tools: Vec<Function> = if last {
            Vec::new()
        } else {
            self.catalog
                .iter()
                .filter(|t| allows(t.name()))
                .map(Function::from)
                .collect()
        };
        let mut system = missions::system_prompt(&run.doc);
        if last {
            system.push_str(
                "\n\nHai esaurito i passi: scrivi ora il rapporto con ciò che hai ottenuto.",
            );
        }
        let count = run.history.len();
        let steps = run
            .history
            .iter()
            .enumerate()
            .map(|(i, h)| AgentStep {
                tool: h["tool"].as_str().unwrap_or_default().to_string(),
                arguments: h["arguments"].clone(),
                output: Self::mission_step_text(h, i + 1 == count),
            })
            .collect();
        Ok(Some(AgentRequest::new(
            *provider,
            model.clone(),
            key.clone(),
            system,
            run.doc["goal"].as_str().unwrap_or_default().to_string(),
            steps,
            tools,
            self.cloud_limiter.clone(),
        )))
    }

    /// Un passo della missione guidata dal cloud: `answer` è la risposta alla richiesta precedente (assente
    /// alla prima chiamata). Esegue lo strumento proposto oppure chiude la missione con il rapporto.
    pub fn mission_advance(
        &mut self,
        run: &mut MissionRun,
        answer: Option<Result<Answer, Failure>>,
    ) -> Result<Next, CoreError> {
        match answer {
            None => {}
            Some(Err(failure)) => {
                let provider = run.cloud.as_ref().map(|c| c.0);
                let text = provider.map_or_else(
                    || "Modello non raggiungibile.".to_string(),
                    |p| failure.user_message(p).0,
                );
                return self.mission_end(run, Err(text)).map(Next::Done);
            }
            Some(Ok(Answer::Text(text))) => return self.mission_end(run, Ok(text)).map(Next::Done),
            Some(Ok(Answer::Call(call))) => {
                let tool = call["tool"].as_str().unwrap_or_default().to_string();
                let version = call["version"].as_str().unwrap_or_default().to_string();
                let arguments = call["arguments"].clone();
                let (digest, status, output) = self.isolated_call(
                    &run.task,
                    "mission",
                    (&tool, &version, &arguments),
                    &run.history,
                    &mut run.out,
                )?;
                run.history
                    .push(step_record(&tool, &arguments, &digest, status, output));
            }
        }
        match self.mission_request(run)? {
            Some(request) => Ok(Next::Ask(Box::new(request))),
            None => self
                .mission_end(run, Err("nessun modello disponibile".into()))
                .map(Next::Done),
        }
    }

    /// Esecuzione completa con il worker locale (nessuna chiave cloud): comandi a regole.
    pub fn mission_run_local(&mut self, run: &mut MissionRun) -> Result<Value, CoreError> {
        let goal = run.doc["goal"].as_str().unwrap_or_default().to_string();
        let inputs = json!({"messages": [{"role": "user", "content": goal}], "memory": []});
        let (reply, history) = self.run_agent_loop(&run.task, &inputs)?;
        run.history = history;
        let reply = reply.map_err(|e| {
            e["message"]
                .as_str()
                .unwrap_or("agente non disponibile")
                .to_string()
        });
        self.mission_end(run, reply)
    }

    /// Registra l'esito: task, diario, prossima esecuzione, notifica e audit.
    pub fn mission_end(
        &mut self,
        run: &mut MissionRun,
        reply: Result<String, String>,
    ) -> Result<Value, CoreError> {
        self.temporary.remove(&run.task.id());
        let mut tools: Vec<String> = Vec::new();
        for h in run.history.iter().filter(|h| h["status"] != "denied") {
            if let Some(t) = h["tool"].as_str() {
                if !tools.iter().any(|x| x == t) {
                    tools.push(t.to_string());
                }
            }
        }
        let ok = reply.is_ok();
        let summary = reply.unwrap_or_else(|e| e);
        if ok {
            run.task.transition("succeeded", "Rapporto della missione");
        } else {
            run.task.fail(
                json!({"code": "mission.failed", "category": "unavailable", "message": clip(&summary, 500),
                       "retryable": true, "effect": "not_applied"}),
                "Missione non riuscita",
            );
        }
        self.save(&run.task)?;
        let now_t = chrono::Utc::now();
        // Il proprietario può aver messo in pausa o annullato la missione durante l'esecuzione.
        let current = self
            .store
            .mission_get(run.id())
            .map_err(internal)?
            .map_or_else(|| run.doc.clone(), |(d, _)| d);
        let stopped = current["status"] != "active";
        let report = RunReport {
            ok,
            summary,
            tools: tools.clone(),
        };
        let (mut doc, mut next) = missions::finish_run(current.clone(), &report, now_t);
        if stopped {
            doc["status"] = current["status"].clone();
            next = None;
        }
        self.store
            .mission_put(&doc, next.as_deref())
            .map_err(internal)?;
        let goal = clip(doc["goal"].as_str().unwrap_or_default(), 70);
        let (severity, title) = match doc["status"].as_str() {
            Some("completed") => ("success", format!("Missione completata: {goal}")),
            Some("failed") => ("warning", format!("Missione interrotta: {goal}")),
            _ if !ok => (
                "warning",
                format!("Missione, esecuzione non riuscita: {goal}"),
            ),
            _ => ("info", format!("Missione, rapporto: {goal}")),
        };
        let body = doc["journal"]
            .as_array()
            .and_then(|j| j.last())
            .and_then(|e| e["summary"].as_str())
            .unwrap_or_default()
            .to_string();
        self.notify(json!({
            "severity": severity, "category": "mission", "title": title,
            "body": if body.is_empty() { "Nessun risultato.".to_string() } else { body },
            "source": {"kind": "module", "id": "missions"},
            "dedupe_key": format!("mission:{}:{}", run.id(), doc["runs"]),
            "task_id": run.task.id(), "correlation_id": run.task.correlation(),
            "link": {"view": "missions", "ref": run.id()},
        }))?;
        let mut event = json!({
            "correlation_id": run.task.correlation(), "task_id": run.task.id(),
            "actor": {"kind": "agent", "id": "mission"}, "node": "core-local",
            "action": "mission.run", "resource": format!("jarvis://mission/{}", run.id()),
            "outcome": "succeeded", "effect": "applied",
        });
        if !ok {
            event["outcome"] = "failed".into();
            event["effect"] = "not_applied".into();
            event["error_code"] = "mission.failed".into();
        }
        self.audit(event)?;
        self.span(
            &run.task,
            "missione",
            "task",
            run.started.ms(),
            if ok { "ok" } else { "error" },
            json!({"mission": run.id(), "tools": tools.len(), "runs": doc["runs"]}),
        );
        Ok(missions::view(&doc, next.as_deref()))
    }

    /// Azioni del proprietario sulle missioni.
    pub fn missions(&self) -> Result<Value, CoreError> {
        let mut v = missions::list(&self.store).map_err(tool_error)?;
        v["items"] = v["missions"].take();
        v["max_open"] = missions::MAX_OPEN.into();
        v["cloud"] = json!(self.mission_provider("missione")?.is_some());
        v["proactive"] = self.proactive_settings();
        Ok(v)
    }

    pub fn mission_create(&mut self, body: &Value) -> Result<Value, CoreError> {
        let view = missions::start(&self.store, body).map_err(tool_error)?;
        self.mission_owner_audit("mission.create", view["id"].as_str().unwrap_or_default())?;
        Ok(view)
    }

    pub fn mission_action(&mut self, id: &str, action: &str) -> Result<Value, CoreError> {
        let view = match action {
            "pause" | "resume" | "cancel" => {
                missions::transition(&self.store, id, action).map_err(tool_error)?
            }
            "delete" => {
                let finished =
                    self.store
                        .mission_get(id)
                        .map_err(internal)?
                        .is_some_and(|(d, _)| {
                            matches!(
                                d["status"].as_str(),
                                Some("completed" | "failed" | "cancelled")
                            )
                        });
                if !finished {
                    return Err(CoreError::BadRequest(
                        "si eliminano solo le missioni concluse o annullate".into(),
                    ));
                }
                self.store.mission_delete(id).map_err(internal)?;
                json!({"id": id, "deleted": true})
            }
            "run" => {
                let Some((mut doc, _)) = self.store.mission_get(id).map_err(internal)? else {
                    return Err(CoreError::NotFound("missione inesistente".into()));
                };
                if doc["status"] != "active" {
                    return Err(CoreError::BadRequest("la missione non è attiva".into()));
                }
                doc["updated_at"] = now().into();
                self.store
                    .mission_put(&doc, Some(&automation::ts(chrono::Utc::now())))
                    .map_err(internal)?;
                missions::view(&doc, None)
            }
            _ => return Err(CoreError::BadRequest("azione sconosciuta".into())),
        };
        self.mission_owner_audit(&format!("mission.{action}"), id)?;
        Ok(view)
    }

    fn mission_owner_audit(&mut self, action: &str, id: &str) -> Result<(), CoreError> {
        self.audit(json!({
            "correlation_id": uuid::Uuid::new_v4().to_string(),
            "actor": {"kind": "owner", "id": "owner"}, "node": "core-local",
            "action": action, "resource": format!("jarvis://mission/{id}"),
            "outcome": "succeeded", "effect": "applied",
        }))?;
        Ok(())
    }
}

fn tool_error(e: crate::tools::ToolError) -> CoreError {
    match e.code {
        "mission.not_found" => CoreError::NotFound(e.message),
        "mission.unavailable" => CoreError::Internal(e.message),
        _ => CoreError::BadRequest(e.message),
    }
}
