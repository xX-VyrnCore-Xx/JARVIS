//! Orchestrator del prototipo: task, delega all'agente, policy, conferma, esecuzione e audit.
//!
//! Flusso (ARCHITECTURE.md): il client invia un messaggio → il Core registra il
//! task → l'Orchestrator delega al General Assistant (worker Python) → se
//! l'agente propone uno strumento, il Security Engine decide → l'esecuzione
//! avviene nel Core (dopo conferma se richiesta) → l'agente compone la risposta
//! → il task si chiude. Ogni passo produce stato ed evidenze.

use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use jarvis_bridge::Worker;
use jarvis_contracts::{canonical_digest, validate, Contract};
use serde_json::{json, Map, Value};

use crate::audit::AuditLog;
use crate::catalog::Catalog;
use crate::config::Config;
use crate::store::Store;
use crate::{automation, now, policy, tools};

const HISTORY_MESSAGES: u32 = 10;
const MAX_MESSAGE: usize = 8000;

#[derive(Debug)]
pub enum CoreError {
    BadRequest(String),
    NotFound(String),
    Gone(String),
    Internal(String),
}

fn internal(e: impl std::fmt::Display) -> CoreError {
    CoreError::Internal(e.to_string())
}

/// Task `jarvis.task/1` con transizioni registrate.
struct Task {
    doc: Value,
}

impl Task {
    fn new(message: &str, requester: Value) -> Self {
        let at = now();
        let objective: String = message.chars().take(4000).collect();
        Self {
            doc: json!({
                "contract": "jarvis.task/1",
                "task_id": uuid::Uuid::new_v4().to_string(),
                "correlation_id": uuid::Uuid::new_v4().to_string(),
                "requester": requester.clone(),
                "objective": objective,
                "success_conditions": ["Risposta fornita al proprietario", "Ogni strumento eseguito ha superato la policy"],
                "dependencies": [],
                "scope": {"nodes": ["core-local"]},
                "state": "pending",
                "history": [{"to": "pending", "at": at, "by": requester, "reason": "Richiesta ricevuta"}],
                "created_at": at,
            }),
        }
    }

    fn id(&self) -> String {
        self.doc["task_id"].as_str().unwrap_or_default().to_string()
    }

    fn correlation(&self) -> String {
        self.doc["correlation_id"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn transition(&mut self, to: &str, reason: &str) {
        let from = self.doc["state"].clone();
        let step = json!({"from": from, "to": to, "at": now(), "by": {"kind": "module", "id": "orchestrator"}, "reason": reason});
        self.doc["history"]
            .as_array_mut()
            .expect("history")
            .push(step);
        self.doc["state"] = to.into();
    }

    fn fail(&mut self, error: Value, reason: &str) {
        self.doc["error"] = error;
        self.transition("failed", reason);
    }
}

struct Pending {
    task: Task,
    decision: Value,
    tool: String,
    version: String,
    arguments: Value,
    inputs: Value,
}

/// Esito restituito al client.
#[derive(Default)]
pub struct Outcome {
    pub task: Value,
    pub reply: Option<String>,
    pub confirmation: Option<Value>,
    pub steps: Vec<Value>,
    pub backend: Option<Value>,
    /// Fonti Web consultate (RF-06): aggiunte dal Core, indipendentemente dal testo del modello.
    pub sources: Vec<Value>,
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        json!({
            "task": self.task,
            "reply": self.reply,
            "confirmation": self.confirmation,
            "steps": self.steps,
            "backend": self.backend,
            "sources": self.sources,
        })
    }
}

pub struct Jarvis {
    cfg: Config,
    store: Store,
    audit: AuditLog,
    catalog: Catalog,
    worker: Option<Worker>,
    pending: HashMap<String, Pending>,
}

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

impl Jarvis {
    pub fn new(cfg: Config) -> Result<Self, String> {
        std::fs::create_dir_all(cfg.workspace()).map_err(|e| format!("workspace: {e}"))?;
        std::fs::create_dir_all(cfg.data_dir.join("audit")).map_err(|e| format!("audit: {e}"))?;
        let store = Store::open(&cfg.data_dir.join("jarvis.sqlite3"))
            .map_err(|e| format!("database: {e}"))?;
        let audit = AuditLog::open(
            &cfg.data_dir.join("audit/core-security.jsonl"),
            "core-security",
        )
        .map_err(|e| format!("audit: {e}"))?;
        let catalog = Catalog::load()?;
        Ok(Self {
            cfg,
            store,
            audit,
            catalog,
            worker: None,
            pending: HashMap::new(),
        })
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    // ------------------------------------------------------------------ worker

    fn worker(&mut self) -> Result<&mut Worker, Value> {
        if self.worker.as_mut().is_some_and(|w| w.has_exited()) {
            self.worker = None;
        }
        if self.worker.is_none() {
            let mut cmd = Command::new(&self.cfg.python);
            cmd.args(["-m", "jarvis_ai.worker", "--handler", "assistant"])
                .current_dir(&self.cfg.ai_dir);
            if let Some(backend) = &self.cfg.ai_backend {
                cmd.env("JARVIS_AI_BACKEND", backend);
            }
            self.worker = Some(Worker::spawn(cmd).map_err(|e| e.to_contract())?);
        }
        Ok(self.worker.as_mut().expect("worker presente"))
    }

    pub fn health(&mut self) -> Value {
        let ai = match self.worker() {
            Ok(w) => {
                let (t, c) = (
                    uuid::Uuid::new_v4().to_string(),
                    uuid::Uuid::new_v4().to_string(),
                );
                match w.request(
                    &t,
                    &c,
                    json!({"kind": "module", "id": "ai-worker"}),
                    "health.request",
                    &json!({}),
                    Duration::from_secs(15),
                ) {
                    Ok(env) => json!({"status": "up", "gateway": env["payload"]}),
                    Err(e) => {
                        self.worker = None;
                        json!({"status": "down", "error": e.to_contract()})
                    }
                }
            }
            Err(e) => json!({"status": "down", "error": e}),
        };
        let (errors, events) = self.check_audit();
        if ai["status"] == "down" {
            let _ = self.notify(json!({
                "severity": "error", "category": "system", "title": "Agente AI non disponibile",
                "body": ai["error"]["message"], "source": {"kind": "module", "id": "monitor"}, "dedupe_key": "ai-unavailable",
            }));
        } else if let Some(reason) = ai["gateway"]["backend"]["degraded_reason"].as_str() {
            let _ = self.notify(json!({
                "severity": "warning", "category": "system", "title": "AI in modalità offline degradata",
                "body": reason, "source": {"kind": "module", "id": "monitor"}, "dedupe_key": "ai-degraded",
            }));
        }
        json!({
            "core": {"version": env!("CARGO_PKG_VERSION"), "status": "up"},
            "web": self.cfg.web.describe(),
            "ai": ai,
            "audit": {"events": events, "chain_valid": errors.is_empty()},
            "pending_confirmations": self.pending.len(),
            "notifications_unread": self.store.notifications_unread().unwrap_or(0),
            "automations": self.store.workflows().map(|w| w.iter().filter(|(d, _)| d["enabled"] == true).count()).unwrap_or(0),
        })
    }

    fn delegation(&self, task: &Task, inputs: &Value) -> Value {
        let issued = chrono::Utc::now();
        let expires =
            issued + chrono::Duration::milliseconds(self.cfg.delegation_timeout_ms as i64 + 60_000);
        let ts = |t: chrono::DateTime<chrono::Utc>| {
            t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        json!({
            "contract": "jarvis.delegation/1",
            "delegation_id": uuid::Uuid::new_v4().to_string(),
            "task_id": task.id(),
            "objective": task.doc["objective"],
            "delegate": {"agent": "general_assistant", "temporary": false},
            "context": {"refs": [format!("jarvis://task/{}", task.id())]},
            "tools": self.catalog.iter().map(|t| json!({"name": t.name(), "version": t.version()})).collect::<Vec<_>>(),
            "limits": {"max_steps": 4, "max_subdelegations": 0, "allow_cloud": false},
            "timeout_ms": self.cfg.delegation_timeout_ms,
            "completion_conditions": ["Esito jarvis.agent-result/1 valido"],
            "issued_at": ts(issued),
            "expires_at": ts(expires),
            "inputs": inputs,
        })
    }

    /// Delega all'agente; restituisce l'esito `jarvis.agent-result/1` o un errore v1.
    fn call_agent(&mut self, task: &Task, inputs: &Value) -> Result<Value, Value> {
        let delegation = self.delegation(task, inputs);
        let (id, corr) = (task.id(), task.correlation());
        let worker = self.worker()?;
        match worker.delegate(&id, &corr, &delegation) {
            Ok(env) if env["status"] == "succeeded" => {
                let report = validate(Contract::AgentResult, &env["payload"]);
                if report.is_valid() {
                    Ok(env["payload"].clone())
                } else {
                    Err(
                        json!({"code": "agent.invalid_result", "category": "internal", "message": "esito dell'agente non conforme", "retryable": false, "effect": "not_applied"}),
                    )
                }
            }
            Ok(env) => Err(env["error"].clone()),
            Err(e) => {
                self.worker = None;
                Err(e.to_contract())
            }
        }
    }

    fn inputs(&self) -> Result<Value, CoreError> {
        let messages: Vec<Value> = self
            .store
            .messages_recent(HISTORY_MESSAGES)
            .map_err(internal)?
            .into_iter()
            .map(|m| json!({"role": m["role"], "content": m["content"].as_str().unwrap_or_default().chars().take(MAX_MESSAGE).collect::<String>()}))
            .collect();
        let memory: Vec<Value> = self
            .store
            .memory_list()
            .map_err(internal)?
            .into_iter()
            .take(50)
            .map(|m| m["text"].clone())
            .collect();
        Ok(json!({"messages": messages, "memory": memory}))
    }

    fn save(&self, task: &Task) -> Result<(), CoreError> {
        let report = validate(Contract::Task, &task.doc);
        if !report.is_valid() {
            return Err(CoreError::Internal(format!(
                "task non conforme al contratto: {report:?}"
            )));
        }
        self.store.task_put(&task.doc, &now()).map_err(internal)
    }

    fn audit(&mut self, fields: Value) -> Result<Value, CoreError> {
        self.audit.append(obj(fields)).map_err(internal)
    }

    // ------------------------------------------------------------------ chat

    pub fn chat(&mut self, message: &str) -> Result<Outcome, CoreError> {
        let message = message.trim();
        if message.is_empty() || message.chars().count() > MAX_MESSAGE {
            return Err(CoreError::BadRequest(format!(
                "il messaggio deve avere da 1 a {MAX_MESSAGE} caratteri"
            )));
        }
        self.expire_pending()?;
        let mut task = Task::new(message, json!({"kind": "owner", "id": "owner"}));
        self.store
            .message_add("user", message, &task.id(), &now(), None)
            .map_err(internal)?;
        task.transition("planned", "Delega al General Assistant");
        task.transition("running", "Agente in esecuzione");
        self.save(&task)?;

        let inputs = self.inputs()?;
        let mut out = Outcome::default();
        let result = match self.call_agent(&task, &inputs) {
            Ok(r) => r,
            Err(error) => {
                let text = format!(
                    "Non riesco a contattare l'agente AI: {}",
                    error["message"].as_str().unwrap_or("errore sconosciuto")
                );
                self.notify(json!({
                    "severity": "error", "category": "system", "title": "Agente AI non disponibile",
                    "body": format!("La richiesta non è stata elaborata: {}", error["message"].as_str().unwrap_or("errore sconosciuto")),
                    "source": {"kind": "module", "id": "orchestrator"}, "correlation_id": task.correlation(), "task_id": task.id(),
                    "dedupe_key": "ai-unavailable", "link": {"view": "tasks", "ref": task.id()},
                }))?;
                task.fail(error, "Delega non riuscita");
                return self.finish(task, out, text);
            }
        };
        out.backend = Some(result["backend"].clone());
        out.steps.push(json!({"kind": "delegation", "agent": "general_assistant", "backend": result["backend"]}));
        if result["kind"] == "reply" {
            task.transition("succeeded", "Risposta dell'agente");
            let text = result["text"].as_str().unwrap_or_default().to_string();
            return self.finish(task, out, text);
        }

        let call = &result["tool_call"];
        let tool = call["tool"].as_str().unwrap_or_default().to_string();
        let version = call["version"].as_str().unwrap_or_default().to_string();
        let arguments = call["arguments"].clone();
        out.steps.push(json!({"kind": "tool_call", "tool": tool, "arguments": arguments, "rationale": call["rationale"]}));
        let digest =
            canonical_digest(&json!({"tool": tool, "version": version, "arguments": arguments}));
        let (tid, corr) = (task.id(), task.correlation());
        let decision = policy::decide(
            &self.catalog,
            &self.cfg.web,
            &policy::Request {
                subject: json!({"kind": "agent", "id": "general_assistant"}),
                task_id: &tid,
                correlation_id: &corr,
                tool: &tool,
                version: &version,
                arguments: &arguments,
                digest: &digest,
            },
        );
        out.steps.push(json!({"kind": "policy", "decision": decision["decision"], "reasons": decision["reasons"], "risk": decision["risk"]}));
        let outcome_name = if decision["decision"] == "deny" {
            "denied"
        } else {
            "allowed"
        };
        self.audit(json!({
            "correlation_id": corr, "task_id": tid, "actor": {"kind": "agent", "id": "general_assistant"}, "node": "core-local",
            "action": decision["request"]["action"]["tool"], "decision_id": decision["decision_id"], "outcome": outcome_name,
        }))?;

        match decision["decision"].as_str() {
            Some("deny") => {
                let code = decision["reasons"][0].as_str().unwrap_or("policy.denied");
                let output = json!({"code": code, "message": policy::explain(code)});
                let text = self.compose(&task, &inputs, &tool, "denied", output, &mut out);
                task.transition(
                    "succeeded",
                    "Strumento negato dalla policy; risposta fornita",
                );
                self.finish(task, out, text)
            }
            Some("require_confirmation") => {
                let conf = decision["confirmation"].clone();
                task.transition(
                    "waiting_confirmation",
                    "Conferma del proprietario richiesta",
                );
                self.save(&task)?;
                let text = format!(
                    "Prima di procedere mi serve la tua conferma: {}",
                    conf["expected_effect"].as_str().unwrap_or_default()
                );
                self.store
                    .message_add("assistant", &text, &task.id(), &now(), None)
                    .map_err(internal)?;
                out.task = task.doc.clone();
                out.reply = Some(text);
                out.confirmation = Some(json!({
                    "confirmation_id": conf["confirmation_id"], "tool": tool, "arguments": arguments,
                    "target": conf["target"], "expected_effect": conf["expected_effect"], "expires_at": conf["expires_at"], "risk": decision["risk"],
                }));
                let id = conf["confirmation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                self.notify(json!({
                    "severity": "warning", "category": "confirmation", "title": format!("Conferma richiesta: {tool}"),
                    "body": conf["expected_effect"], "source": {"kind": "module", "id": "security"},
                    "correlation_id": task.correlation(), "task_id": task.id(),
                    "dedupe_key": format!("confirmation:{id}"), "link": {"view": "chat", "ref": id},
                }))?;
                self.pending.insert(
                    id,
                    Pending {
                        task,
                        decision,
                        tool,
                        version,
                        arguments,
                        inputs,
                    },
                );
                Ok(out)
            }
            _ => {
                let (status, output) = self.run_tool(&task, &tool, &arguments, false, &mut out)?;
                let text = self.compose(&task, &inputs, &tool, status, output, &mut out);
                task.transition("succeeded", "Strumento eseguito e risposta composta");
                self.finish(task, out, text)
            }
        }
    }

    pub fn confirm(&mut self, confirmation_id: &str, approve: bool) -> Result<Outcome, CoreError> {
        let Pending {
            mut task,
            decision,
            tool,
            version,
            arguments,
            inputs,
        } = self
            .pending
            .remove(confirmation_id)
            .ok_or_else(|| CoreError::NotFound("conferma inesistente o già utilizzata".into()))?;
        self.store
            .notification_read_by_key(&format!("confirmation:{confirmation_id}"), &now())
            .map_err(internal)?;
        let conf = &decision["confirmation"];
        let (tid, corr) = (task.id(), task.correlation());
        let base = json!({"correlation_id": corr, "task_id": tid, "actor": {"kind": "owner", "id": "owner"}, "node": "core-local",
                          "action": tool, "decision_id": decision["decision_id"]});
        let expired = conf["expires_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_none_or(|t| t < chrono::Utc::now());
        if expired {
            self.audit(merge(
                &base,
                json!({"outcome": "cancelled", "effect": "not_applied"}),
            ))?;
            task.transition("cancelled", "Conferma scaduta");
            self.save(&task)?;
            self.notify(json!({
                "severity": "info", "category": "confirmation", "title": format!("Conferma scaduta: {tool}"),
                "body": "L'azione non è stata eseguita. Ripeti la richiesta se serve ancora.",
                "source": {"kind": "module", "id": "security"}, "correlation_id": corr, "task_id": tid, "link": {"view": "tasks", "ref": tid},
            }))?;
            return Err(CoreError::Gone(
                "la conferma è scaduta: ripeti la richiesta".into(),
            ));
        }
        let digest =
            canonical_digest(&json!({"tool": tool, "version": version, "arguments": arguments}));
        if conf["parameters_digest"] != digest.as_str() {
            task.fail(json!({"code": "security.confirmation_mismatch", "category": "forbidden", "message": "parametri diversi da quelli confermati", "retryable": false, "effect": "not_applied"}), "Conferma non corrispondente");
            self.save(&task)?;
            return Err(CoreError::Internal(
                "conferma non corrispondente ai parametri".into(),
            ));
        }
        let mut out = Outcome::default();
        if approve {
            self.audit(merge(&base, json!({"outcome": "confirmed"})))?;
            out.steps
                .push(json!({"kind": "confirmation", "approved": true}));
            task.transition("running", "Confermato dal proprietario");
            let (status, output) = self.run_tool(&task, &tool, &arguments, true, &mut out)?;
            let text = self.compose(&task, &inputs, &tool, status, output, &mut out);
            task.transition("succeeded", "Strumento eseguito dopo conferma");
            self.finish(task, out, text)
        } else {
            self.audit(merge(
                &base,
                json!({"outcome": "cancelled", "effect": "not_applied"}),
            ))?;
            out.steps
                .push(json!({"kind": "confirmation", "approved": false}));
            let text = self.compose(
                &task,
                &inputs,
                &tool,
                "cancelled",
                json!({"message": "rifiutato dal proprietario"}),
                &mut out,
            );
            task.transition("cancelled", "Rifiutato dal proprietario");
            self.finish(task, out, text)
        }
    }

    pub fn pending_confirmations(&self) -> Vec<Value> {
        self.pending
            .values()
            .map(|p| {
                let c = &p.decision["confirmation"];
                json!({"confirmation_id": c["confirmation_id"], "task_id": p.task.id(), "tool": p.tool, "arguments": p.arguments,
                       "target": c["target"], "expected_effect": c["expected_effect"], "expires_at": c["expires_at"], "risk": p.decision["risk"]})
            })
            .collect()
    }

    fn expire_pending(&mut self) -> Result<(), CoreError> {
        let now_t = chrono::Utc::now();
        let expired: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| {
                p.decision["confirmation"]["expires_at"]
                    .as_str()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .is_none_or(|t| t < now_t)
            })
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            let _ = self.confirm(&id, false).or_else(|e| match e {
                CoreError::Gone(_) => Ok(Outcome::default()),
                other => Err(other),
            })?;
        }
        Ok(())
    }

    fn run_tool(
        &mut self,
        task: &Task,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
        out: &mut Outcome,
    ) -> Result<(&'static str, Value), CoreError> {
        let result = self.execute_tool(tool, arguments, confirmed);
        let base = json!({"correlation_id": task.correlation(), "task_id": task.id(), "actor": {"kind": "module", "id": "core"},
                          "node": "core-local", "action": tool});
        match result {
            Ok(output) => {
                self.audit(merge(
                    &base,
                    json!({"outcome": "succeeded", "effect": "applied"}),
                ))?;
                out.steps
                    .push(json!({"kind": "tool_result", "tool": tool, "status": "succeeded"}));
                out.sources.extend(sources_of(tool, &output));
                Ok(("succeeded", output))
            }
            Err(e) => {
                self.audit(merge(
                    &base,
                    json!({"outcome": "failed", "effect": "not_applied", "error_code": e.code}),
                ))?;
                out.steps.push(json!({"kind": "tool_result", "tool": tool, "status": "failed", "error": e.code}));
                Ok(("failed", json!({"code": e.code, "message": e.message})))
            }
        }
    }

    /// Chiede all'agente di comporre la risposta finale; se non riesce, usa un testo locale.
    fn compose(
        &mut self,
        task: &Task,
        inputs: &Value,
        tool: &str,
        status: &str,
        output: Value,
        out: &mut Outcome,
    ) -> String {
        let mut inputs = inputs.clone();
        inputs["tool_result"] = json!({"tool": tool, "status": status, "output": output});
        match self.call_agent(task, &inputs) {
            Ok(r) if r["kind"] == "reply" => {
                out.backend = Some(r["backend"].clone());
                r["text"].as_str().unwrap_or_default().to_string()
            }
            _ => {
                let raw = output.to_string();
                let short: String = raw.chars().take(2000).collect();
                format!("Esito di {tool} ({status}): {short}")
            }
        }
    }

    fn finish(&mut self, task: Task, mut out: Outcome, text: String) -> Result<Outcome, CoreError> {
        self.save(&task)?;
        self.store
            .message_add(
                "assistant",
                &text,
                &task.id(),
                &now(),
                Some(&Value::Array(out.sources.clone())),
            )
            .map_err(internal)?;
        out.task = task.doc;
        out.reply = Some(text);
        Ok(out)
    }

    // ------------------------------------------------------------------ letture e azioni dirette

    pub fn tasks(&self, limit: u32) -> Result<Vec<Value>, CoreError> {
        self.store.tasks_recent(limit).map_err(internal)
    }

    pub fn conversation(&self, limit: u32) -> Result<Vec<Value>, CoreError> {
        self.store.messages_recent(limit).map_err(internal)
    }

    pub fn memory(&self) -> Result<Vec<Value>, CoreError> {
        self.store.memory_list().map_err(internal)
    }

    /// Cancellazione diretta richiesta dal proprietario dall'interfaccia (RF-08).
    pub fn forget(&mut self, id: &str) -> Result<(), CoreError> {
        if !self.store.memory_delete(id).map_err(internal)? {
            return Err(CoreError::NotFound(format!(
                "nessun elemento di memoria con id {id}"
            )));
        }
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "memory.forget", "outcome": "succeeded", "effect": "applied"}))?;
        Ok(())
    }

    pub fn audit_log(&self, limit: usize) -> Result<Value, CoreError> {
        let events = self.audit.read_all().map_err(internal)?;
        let errors = jarvis_contracts::audit_chain_errors(&events);
        let total = events.len();
        let recent: Vec<Value> = events.into_iter().rev().take(limit).collect();
        Ok(
            json!({"total": total, "chain_valid": errors.is_empty(), "chain_errors": errors, "events": recent}),
        )
    }

    pub fn workspace_listing(&self) -> Result<Value, CoreError> {
        let workspace = self.cfg.workspace();
        tools::execute(
            "fs.list",
            &json!({"path": "."}),
            &tools::Context {
                workspace: &workspace,
                store: &self.store,
                web: &self.cfg.web,
            },
        )
        .map_err(|e| CoreError::Internal(e.message))
    }
}

// ---------------------------------------------------------------------- Automation Engine (RF-07)

fn tool_error(code: &'static str, message: impl Into<String>) -> tools::ToolError {
    tools::ToolError {
        code,
        message: message.into(),
    }
}

impl Jarvis {
    /// Esegue uno strumento del catalogo. Gli strumenti `workflow.*` sono gestiti dal Core stesso.
    fn execute_tool(
        &mut self,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
    ) -> Result<Value, tools::ToolError> {
        match tool {
            "workflow.create" => {
                if !confirmed {
                    return Err(tool_error(
                        "workflow.not_confirmed",
                        "la creazione richiede la conferma del proprietario",
                    ));
                }
                let doc = self.build_workflow(arguments, "chat_confirmation")?;
                self.store_new_workflow(&doc)
            }
            "workflow.update" => {
                if !confirmed {
                    return Err(tool_error(
                        "workflow.not_confirmed",
                        "la modifica richiede la conferma del proprietario",
                    ));
                }
                let (current, next) =
                    self.find_workflow(arguments["id"].as_str().unwrap_or_default())?;
                self.apply_update(&current, next, arguments, "chat_confirmation")
            }
            "workflow.list" => Ok(
                json!({"items": self.workflow_views().map_err(|e| tool_error("workflow.unavailable", format!("{e:?}")))?}),
            ),
            "workflow.run" => {
                let (doc, _) = self.find_workflow(arguments["id"].as_str().unwrap_or_default())?;
                let run = self
                    .run_workflow(&doc, "manual", None)
                    .map_err(|e| tool_error("workflow.unavailable", format!("{e:?}")))?;
                Ok(json!({"name": doc["name"], "status": run["status"], "steps": run["steps"]}))
            }
            "workflow.delete" => {
                let (doc, _) = self.find_workflow(arguments["id"].as_str().unwrap_or_default())?;
                self.store
                    .workflow_delete(doc["workflow_id"].as_str().unwrap_or_default())
                    .map_err(|e| tool_error("workflow.unavailable", e.to_string()))?;
                Ok(json!({"id": short(&doc), "name": doc["name"]}))
            }
            _ => {
                let workspace = self.cfg.workspace();
                tools::execute(
                    tool,
                    arguments,
                    &tools::Context {
                        workspace: &workspace,
                        store: &self.store,
                        web: &self.cfg.web,
                    },
                )
            }
        }
    }

    fn find_workflow(&self, key: &str) -> Result<(Value, Option<String>), tools::ToolError> {
        let found = self
            .store
            .workflow_find(key)
            .map_err(|e| tool_error("workflow.unavailable", e.to_string()))?;
        match found.len() {
            1 => Ok(found.into_iter().next().expect("un elemento")),
            0 => Err(tool_error(
                "workflow.not_found",
                format!("nessuna automazione con id {key}"),
            )),
            _ => Err(tool_error(
                "workflow.ambiguous",
                format!("l'id {key} corrisponde a più automazioni: usane uno più lungo"),
            )),
        }
    }

    /// Costruisce un `jarvis.workflow/1` da argomenti dello strumento o del modulo dell'interfaccia.
    /// Valida i passi contro il catalogo e fissa la versione di ogni strumento.
    fn normalize_steps(&self, raw: &Value) -> Result<Value, tools::ToolError> {
        let invalid = |m: String| tool_error("workflow.invalid", m);
        let raw_steps = raw
            .as_array()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("servono da 1 a 10 passi".into()))?;
        let mut steps = Vec::new();
        for (i, step) in raw_steps.iter().enumerate() {
            let name = step["tool"].as_str().unwrap_or_default();
            if name.starts_with("workflow.") {
                return Err(invalid(format!(
                    "passo {}: un'automazione non può gestire altre automazioni",
                    i + 1
                )));
            }
            let tool = self.catalog.get(name).ok_or_else(|| {
                invalid(format!("passo {}: strumento sconosciuto '{name}'", i + 1))
            })?;
            let arguments = if step["arguments"].is_object() {
                step["arguments"].clone()
            } else {
                json!({})
            };
            if !tool.input.is_valid(&arguments) {
                return Err(invalid(format!(
                    "passo {}: argomenti non validi per {name}",
                    i + 1
                )));
            }
            steps.push(json!({"tool": name, "version": tool.version(), "arguments": arguments}));
        }
        Ok(Value::Array(steps))
    }

    fn build_workflow(&self, args: &Value, method: &str) -> Result<Value, tools::ToolError> {
        let invalid = |m: String| tool_error("workflow.invalid", m);
        let steps = self.normalize_steps(&args["steps"])?;
        let at = now();
        let doc = json!({
            "contract": "jarvis.workflow/1",
            "workflow_id": uuid::Uuid::new_v4().to_string(),
            "name": args["name"].as_str().map(|n| n.chars().take(200).collect::<String>()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Automazione".into()),
            "version": 1,
            "enabled": true,
            "trigger": args["trigger"],
            "steps": steps,
            "on_failure": if args["on_failure"] == "continue" { "continue" } else { "stop" },
            "approval": {"approved_by": {"kind": "owner", "id": "owner"}, "method": method, "approved_at": at, "steps_digest": automation::steps_digest(&steps)},
            "created_at": at,
        });
        let report = validate(Contract::Workflow, &doc);
        if !report.is_valid() {
            return Err(invalid(format!(
                "automazione non conforme: {}",
                report.schema.join("; ")
            )));
        }
        if automation::first_run(&doc["trigger"], chrono::Utc::now(), &chrono::Local).is_none() {
            return Err(invalid("la data indicata è già passata".into()));
        }
        Ok(doc)
    }

    fn store_new_workflow(&mut self, doc: &Value) -> Result<Value, tools::ToolError> {
        let next = automation::first_run(&doc["trigger"], chrono::Utc::now(), &chrono::Local)
            .map(automation::ts);
        self.store
            .workflow_put(doc, next.as_deref())
            .and_then(|()| self.store.workflow_version_put(doc))
            .map_err(|e| tool_error("workflow.unavailable", e.to_string()))?;
        Ok(json!({
            "id": short(doc), "workflow_id": doc["workflow_id"], "name": doc["name"],
            "trigger": automation::describe(&doc["trigger"]),
            "next_run": next.as_deref().map(automation::fmt_local),
            "steps": doc["steps"].as_array().map(|s| s.iter().map(|x| x["tool"].clone()).collect::<Vec<_>>()),
        }))
    }

    fn workflow_views(&self) -> Result<Vec<Value>, CoreError> {
        let mut out = Vec::new();
        for (doc, next) in self.store.workflows().map_err(internal)? {
            let id = doc["workflow_id"].as_str().unwrap_or_default().to_string();
            let last = self
                .store
                .runs(&id, 1)
                .map_err(internal)?
                .into_iter()
                .next();
            out.push(json!({
                "id": short(&doc), "workflow_id": id, "name": doc["name"], "enabled": doc["enabled"], "version": doc["version"], "updated_at": doc["updated_at"],
                "trigger": doc["trigger"], "trigger_text": automation::describe(&doc["trigger"]),
                "steps": doc["steps"], "on_failure": doc["on_failure"], "approval": doc["approval"],
                "next_run_at": next, "next_run": next.as_deref().map(automation::fmt_local),
                "last_run": last.map(|r| json!({"status": r["status"], "started_at": r["started_at"], "trigger": r["trigger"]["kind"]})),
            }));
        }
        Ok(out)
    }

    /// Esegue un workflow con la policy vigente. I passi a rischio alto passano solo se coperti
    /// dall'approvazione del proprietario, verificata ricalcolando il digest dei passi.
    pub fn run_workflow(
        &mut self,
        doc: &Value,
        kind: &str,
        scheduled_for: Option<&str>,
    ) -> Result<Value, CoreError> {
        let name = doc["name"].as_str().unwrap_or("automazione");
        let mut task = Task::new(
            &format!("Automazione «{name}»"),
            json!({"kind": "module", "id": "automation"}),
        );
        task.transition("planned", &format!("Trigger {kind}"));
        task.transition("running", "Esecuzione dei passi");
        let (tid, corr) = (task.id(), task.correlation());
        let started = now();
        let subject = json!({"kind": "module", "id": "automation"});
        let steps = doc["steps"].as_array().cloned().unwrap_or_default();
        let approved =
            doc["approval"]["steps_digest"] == automation::steps_digest(&doc["steps"]).as_str();
        let stop_on_failure = doc["on_failure"] != "continue";
        let mut results: Vec<Value> = Vec::new();
        let mut first_problem: Option<(String, &str)> = None;

        for (index, step) in steps.iter().enumerate() {
            let tool = step["tool"].as_str().unwrap_or_default().to_string();
            if !approved || (first_problem.is_some() && stop_on_failure) {
                results.push(json!({"index": index, "tool": tool, "decision": "not_evaluated", "status": "skipped"}));
                continue;
            }
            let version = step["version"].as_str().unwrap_or_default().to_string();
            let arguments = step["arguments"].clone();
            let digest = canonical_digest(
                &json!({"tool": tool, "version": version, "arguments": arguments}),
            );
            let decision = policy::decide(
                &self.catalog,
                &self.cfg.web,
                &policy::Request {
                    subject: subject.clone(),
                    task_id: &tid,
                    correlation_id: &corr,
                    tool: &tool,
                    version: &version,
                    arguments: &arguments,
                    digest: &digest,
                },
            );
            let verdict = decision["decision"].as_str().unwrap_or("deny").to_string();
            let preapproved = verdict == "require_confirmation";
            self.audit(json!({"correlation_id": corr, "task_id": tid, "actor": subject, "node": "core-local",
                              "action": decision["request"]["action"]["tool"], "decision_id": decision["decision_id"],
                              "outcome": if verdict == "deny" { "denied" } else { "allowed" }}))?;
            let mut entry = json!({"index": index, "tool": tool, "decision": verdict, "decision_id": decision["decision_id"]});
            if preapproved {
                entry["preapproved"] = true.into();
            }
            if verdict == "deny" {
                let code = decision["reasons"][0]
                    .as_str()
                    .unwrap_or("policy.denied")
                    .to_string();
                entry["status"] = "denied".into();
                entry["summary"] = policy::explain(&code).into();
                entry["error_code"] = code.clone().into();
                first_problem.get_or_insert((code, "denied"));
                results.push(entry);
                continue;
            }
            let base = json!({"correlation_id": corr, "task_id": tid, "actor": {"kind": "module", "id": "core"}, "node": "core-local", "action": tool});
            match self.execute_tool(&tool, &arguments, preapproved) {
                Ok(output) => {
                    self.audit(merge(
                        &base,
                        json!({"outcome": "succeeded", "effect": "applied"}),
                    ))?;
                    entry["status"] = "succeeded".into();
                    entry["summary"] = automation::summarize(&output).into();
                }
                Err(e) => {
                    self.audit(merge(
                        &base,
                        json!({"outcome": "failed", "effect": "not_applied", "error_code": e.code}),
                    ))?;
                    entry["status"] = "failed".into();
                    entry["error_code"] = e.code.into();
                    entry["summary"] = e.message.chars().take(2000).collect::<String>().into();
                    first_problem.get_or_insert((e.code.to_string(), "failed"));
                }
            }
            results.push(entry);
        }

        let succeeded = results
            .iter()
            .filter(|r| r["status"] == "succeeded")
            .count();
        let mut run = json!({
            "contract": "jarvis.workflow-run/1",
            "run_id": uuid::Uuid::new_v4().to_string(),
            "workflow_id": doc["workflow_id"],
            "workflow_version": doc["version"],
            "task_id": tid,
            "trigger": {"kind": kind},
            "started_at": started,
            "finished_at": now(),
            "steps": results,
        });
        if let Some(s) = scheduled_for {
            run["trigger"]["scheduled_for"] = s.into();
        }
        let error = if !approved {
            Some(
                json!({"code": "security.approval_mismatch", "category": "forbidden", "message": "i passi non corrispondono a quelli approvati: automazione sospesa", "retryable": false, "effect": "not_applied"}),
            )
        } else if let Some((code, kind)) = &first_problem {
            if succeeded == 0 {
                let category = if *kind == "denied" {
                    "policy_denied"
                } else {
                    "internal"
                };
                Some(
                    json!({"code": code, "category": category, "message": format!("passo non riuscito: {code}"), "retryable": false, "effect": "not_applied"}),
                )
            } else {
                None
            }
        } else {
            None
        };
        let status = match (&error, &first_problem) {
            (Some(_), _) => "failed",
            (None, Some(_)) => "partial",
            (None, None) => "succeeded",
        };
        run["status"] = status.into();
        match &error {
            Some(e) => {
                run["error"] = e.clone();
                task.fail(e.clone(), "Automazione non riuscita");
            }
            None => task.transition(
                "succeeded",
                if status == "partial" {
                    "Esecuzione parziale"
                } else {
                    "Tutti i passi riusciti"
                },
            ),
        }
        let report = validate(Contract::WorkflowRun, &run);
        if !report.is_valid() {
            return Err(CoreError::Internal(format!(
                "esecuzione non conforme: {report:?}"
            )));
        }
        self.save(&task)?;
        self.store.run_put(&run).map_err(internal)?;
        self.notify_run(doc, &run, &task)?;
        if !approved {
            // Passi alterati rispetto all'approvazione: l'automazione viene sospesa.
            let mut disabled = doc.clone();
            disabled["enabled"] = false.into();
            disabled["updated_at"] = now().into();
            self.store.workflow_put(&disabled, None).map_err(internal)?;
        }
        Ok(run)
    }

    /// Esegue i workflow scaduti a `now`. Ogni workflow recupera al più una esecuzione persa.
    pub fn run_due(
        &mut self,
        now_t: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Value>, CoreError> {
        let tolerance = chrono::Duration::seconds(self.cfg.scheduler_tick_s as i64 * 2 + 60);
        let mut runs = Vec::new();
        for (doc, scheduled) in self
            .store
            .workflows_due(&automation::ts(now_t))
            .map_err(internal)?
        {
            if doc["enabled"] != true {
                continue;
            }
            let late = automation::parse_ts(&scheduled).is_some_and(|s| now_t - s > tolerance);
            let run = self.run_workflow(
                &doc,
                if late { "catch_up" } else { "schedule" },
                Some(&scheduled),
            )?;
            // Rilegge il documento: l'esecuzione può averlo sospeso.
            let current = self
                .store
                .workflow_find(doc["workflow_id"].as_str().unwrap_or_default())
                .map_err(internal)?;
            if let Some((mut current, _)) = current.into_iter().next() {
                let next = if current["enabled"] == true {
                    automation::next_run(&current["trigger"], now_t, &chrono::Local)
                } else {
                    None
                };
                if next.is_none() && current["trigger"]["kind"] == "once" {
                    current["enabled"] = false.into();
                    current["updated_at"] = now().into();
                }
                self.store
                    .workflow_put(&current, next.map(automation::ts).as_deref())
                    .map_err(internal)?;
            }
            runs.push(run);
        }
        Ok(runs)
    }

    // -------------------------------------------------------------- azioni dirette del proprietario

    pub fn workflows(&self) -> Result<Vec<Value>, CoreError> {
        self.workflow_views()
    }

    pub fn workflow_runs(&self, key: &str, limit: u32) -> Result<Vec<Value>, CoreError> {
        let (doc, _) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        self.store
            .runs(doc["workflow_id"].as_str().unwrap_or_default(), limit)
            .map_err(internal)
    }

    fn owner_audit(&mut self, action: &str) -> Result<(), CoreError> {
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": action, "outcome": "succeeded", "effect": "applied"}))?;
        Ok(())
    }

    /// Creazione dall'interfaccia: il proprietario autenticato approva direttamente i passi.
    pub fn create_workflow(&mut self, body: &Value) -> Result<Value, CoreError> {
        let doc = self
            .build_workflow(body, "owner_ui")
            .map_err(|e| CoreError::BadRequest(e.message))?;
        let out = self
            .store_new_workflow(&doc)
            .map_err(|e| CoreError::Internal(e.message))?;
        self.owner_audit("workflow.create")?;
        Ok(out)
    }

    pub fn set_workflow_enabled(&mut self, key: &str, enabled: bool) -> Result<Value, CoreError> {
        let (mut doc, _) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        let next = if enabled {
            automation::first_run(&doc["trigger"], chrono::Utc::now(), &chrono::Local)
        } else {
            None
        };
        if enabled && next.is_none() {
            return Err(CoreError::BadRequest(
                "l'automazione non ha più esecuzioni future".into(),
            ));
        }
        doc["enabled"] = enabled.into();
        doc["updated_at"] = now().into();
        self.store
            .workflow_put(&doc, next.map(automation::ts).as_deref())
            .map_err(internal)?;
        self.owner_audit(if enabled {
            "workflow.enable"
        } else {
            "workflow.disable"
        })?;
        Ok(json!({"enabled": enabled, "next_run_at": next.map(automation::ts)}))
    }

    pub fn run_workflow_now(&mut self, key: &str) -> Result<Value, CoreError> {
        let (doc, _) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        self.owner_audit("workflow.run")?;
        self.run_workflow(&doc, "manual", None)
    }

    pub fn delete_workflow(&mut self, key: &str) -> Result<(), CoreError> {
        let (doc, _) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        self.store
            .workflow_delete(doc["workflow_id"].as_str().unwrap_or_default())
            .map_err(internal)?;
        self.owner_audit("workflow.delete")
    }

    pub fn tool_catalog(&self) -> Vec<Value> {
        self.catalog
            .iter()
            .map(|t| {
                let d = &t.descriptor;
                json!({"name": d["name"], "summary": d["summary"], "risk": d["risk"], "confirmation": d["confirmation"],
                       "input_schema": d["input_schema"], "workflow_step": !t.name().starts_with("workflow.")})
            })
            .collect()
    }

    /// Solo per i test: altera i passi di un workflow senza aggiornare l'approvazione.
    #[doc(hidden)]
    pub fn tamper_workflow_for_tests(&mut self, key: &str, steps: Value) -> Result<(), CoreError> {
        let (mut doc, next) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        doc["steps"] = steps;
        self.store
            .workflow_put(&doc, next.as_deref())
            .map_err(internal)
    }
}

fn short(doc: &Value) -> String {
    doc["workflow_id"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(8)
        .collect()
}

// ---------------------------------------------------------------------- Notifiche e monitor (RF-13)

impl Jarvis {
    /// Crea una notifica `jarvis.notification/1`. Con `dedupe_key`, non duplica una notifica ancora non letta.
    pub fn notify(&mut self, fields: Value) -> Result<Option<Value>, CoreError> {
        if let Some(key) = fields["dedupe_key"].as_str() {
            if self
                .store
                .notification_unread_exists(key)
                .map_err(internal)?
            {
                return Ok(None);
            }
        }
        let mut doc = obj(fields);
        doc.insert("contract".into(), "jarvis.notification/1".into());
        doc.insert(
            "notification_id".into(),
            uuid::Uuid::new_v4().to_string().into(),
        );
        doc.insert("created_at".into(), now().into());
        if !doc.get("correlation_id").is_some_and(Value::is_string) {
            doc.insert(
                "correlation_id".into(),
                uuid::Uuid::new_v4().to_string().into(),
            );
        }
        doc.retain(|_, v| !v.is_null());
        if let Some(Value::String(body)) = doc.get_mut("body") {
            *body = body.chars().take(2000).collect();
        }
        let doc = Value::Object(doc);
        let report = validate(Contract::Notification, &doc);
        if !report.is_valid() {
            return Err(CoreError::Internal(format!(
                "notifica non conforme: {report:?}"
            )));
        }
        self.store.notification_put(&doc, 500).map_err(internal)?;
        Ok(Some(doc))
    }

    /// Esito di un'automazione: errori e parziali sempre (una notifica non letta per workflow), successi solo per i trigger singoli e manuali.
    fn notify_run(&mut self, doc: &Value, run: &Value, task: &Task) -> Result<(), CoreError> {
        let name = doc["name"].as_str().unwrap_or("automazione");
        let wid = doc["workflow_id"].as_str().unwrap_or_default();
        let status = run["status"].as_str().unwrap_or_default();
        let problems: Vec<String> = run["steps"]
            .as_array()
            .map(|s| {
                s.iter()
                    .filter(|x| x["status"] == "failed" || x["status"] == "denied")
                    .map(|x| {
                        format!(
                            "{} {} ({}): {}",
                            x["tool"].as_str().unwrap_or("?"),
                            x["status"].as_str().unwrap_or("?"),
                            x["error_code"].as_str().unwrap_or("errore"),
                            x["summary"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let (severity, title, body, dedupe) = match status {
            "failed" if run["error"]["code"] == "security.approval_mismatch" => (
                "error", format!("Automazione sospesa: {name}"),
                "I passi salvati non corrispondono a quelli che hai approvato: l'automazione è stata sospesa.".to_string(), None,
            ),
            "failed" => ("error", format!("Automazione non riuscita: {name}"), problems.join("; "), Some(format!("workflow-failure:{wid}"))),
            "partial" => ("warning", format!("Automazione parziale: {name}"), problems.join("; "), Some(format!("workflow-failure:{wid}"))),
            _ if doc["trigger"]["kind"] == "once" || run["trigger"]["kind"] == "manual" => {
                ("success", format!("Automazione completata: {name}"), format!("{} passi riusciti", run["steps"].as_array().map_or(0, Vec::len)), None)
            }
            _ => return Ok(()),
        };
        let category = if run["error"]["code"] == "security.approval_mismatch" {
            "security"
        } else {
            "automation"
        };
        self.notify(json!({
            "severity": severity, "category": category, "title": title, "body": body,
            "source": {"kind": "module", "id": "automation"}, "correlation_id": task.correlation(), "task_id": task.id(),
            "workflow_id": wid, "dedupe_key": dedupe, "link": {"view": "automations", "ref": wid},
        }))?;
        Ok(())
    }

    fn check_audit(&mut self) -> (Vec<String>, usize) {
        let events = self.audit.read_all().unwrap_or_default();
        let errors = jarvis_contracts::audit_chain_errors(&events);
        if !errors.is_empty() {
            let _ = self.notify(json!({
                "severity": "error", "category": "security", "title": "Registro di audit alterato",
                "body": format!("La catena di hash non è integra: {}", errors.join("; ")),
                "source": {"kind": "module", "id": "monitor"}, "dedupe_key": "audit-chain", "link": {"view": "audit"},
            }));
        }
        (errors, events.len())
    }

    /// Controlli periodici del monitor, eseguiti dal pianificatore anche senza interfaccia aperta.
    pub fn monitor(&mut self) -> Result<(), CoreError> {
        self.check_audit();
        if self.worker.as_mut().is_some_and(|w| w.has_exited()) {
            self.worker = None;
            self.notify(json!({
                "severity": "warning", "category": "system", "title": "Worker AI terminato",
                "body": "Il processo dell'agente si è chiuso; verrà riavviato alla prossima richiesta.",
                "source": {"kind": "module", "id": "monitor"}, "dedupe_key": "worker-exited",
            }))?;
        }
        Ok(())
    }

    pub fn notifications(
        &self,
        limit: u32,
        unread_only: bool,
        since: Option<&str>,
    ) -> Result<Value, CoreError> {
        let items = self
            .store
            .notifications(limit, unread_only, since)
            .map_err(internal)?;
        let unread = self.store.notifications_unread().map_err(internal)?;
        Ok(json!({"items": items, "unread": unread}))
    }

    pub fn mark_notification_read(&mut self, id: &str) -> Result<Value, CoreError> {
        if !self.store.notification_read(id, &now()).map_err(internal)? {
            return Err(CoreError::NotFound(
                "notifica inesistente o già letta".into(),
            ));
        }
        Ok(json!({"unread": self.store.notifications_unread().map_err(internal)?}))
    }

    pub fn mark_all_notifications_read(&mut self) -> Result<Value, CoreError> {
        let n = self
            .store
            .notifications_read_all(&now())
            .map_err(internal)?;
        Ok(json!({"marked": n, "unread": 0}))
    }

    /// Solo per i test: simula l'arresto improvviso del worker.
    #[doc(hidden)]
    pub fn kill_worker_for_tests(&mut self) {
        if let Some(w) = self.worker.as_mut() {
            w.kill_for_tests();
        }
    }
}

/// Provenienza delle evidenze Web: URL, titolo, istante di recupero e, per le pagine lette, SHA-256 del contenuto.
fn sources_of(tool: &str, output: &Value) -> Vec<Value> {
    match tool {
        "web.fetch" => vec![json!({
            "kind": "page", "title": output["title"], "url": output["final_url"], "requested_url": output["url"],
            "retrieved_at": output["retrieved_at"], "content_sha256": output["content_sha256"],
        })],
        "web.search" => output["results"]
            .as_array()
            .map(|r| {
                r.iter()
                    .map(|x| json!({"kind": "search_result", "rank": x["rank"], "title": x["title"], "url": x["url"],
                                    "provider": output["provider"], "retrieved_at": output["retrieved_at"]}))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------- Modifica delle automazioni

impl Jarvis {
    /// Nuova versione di un workflow: campi modificati, versione + 1, nuova approvazione legata ai passi.
    fn build_update(
        &self,
        current: &Value,
        args: &Value,
        method: &str,
    ) -> Result<(Value, Vec<String>), tools::ToolError> {
        let invalid = |m: String| tool_error("workflow.invalid", m);
        let mut doc = current.clone();
        let mut changes = Vec::new();
        if let Some(name) = args["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            let name: String = name.chars().take(200).collect();
            if doc["name"] != name.as_str() {
                changes.push(format!(
                    "nome: «{}» → «{name}»",
                    doc["name"].as_str().unwrap_or("")
                ));
                doc["name"] = name.into();
            }
        }
        if args["trigger"].is_object() && args["trigger"] != doc["trigger"] {
            changes.push(format!(
                "frequenza: {} → {}",
                automation::describe(&doc["trigger"]),
                automation::describe(&args["trigger"])
            ));
            doc["trigger"] = args["trigger"].clone();
        }
        if !args["steps"].is_null() {
            let steps = self.normalize_steps(&args["steps"])?;
            if steps != doc["steps"] {
                let list = |s: &Value| {
                    s.as_array()
                        .map(|a| {
                            a.iter()
                                .map(|x| x["tool"].as_str().unwrap_or("?").to_string())
                                .collect::<Vec<_>>()
                                .join(" → ")
                        })
                        .unwrap_or_default()
                };
                changes.push(format!("passi: {} → {}", list(&doc["steps"]), list(&steps)));
                doc["steps"] = steps;
            }
        }
        if let Some(policy) = args["on_failure"]
            .as_str()
            .filter(|p| matches!(*p, "stop" | "continue"))
        {
            if doc["on_failure"] != policy {
                changes.push(format!(
                    "in caso di errore: {} → {policy}",
                    doc["on_failure"].as_str().unwrap_or("stop")
                ));
                doc["on_failure"] = policy.into();
            }
        }
        if changes.is_empty() {
            return Err(invalid(
                "nessuna modifica rispetto alla versione attuale".into(),
            ));
        }
        let at = now();
        doc["version"] = (doc["version"].as_u64().unwrap_or(1) + 1).into();
        doc["updated_at"] = at.clone().into();
        doc["approval"] = json!({"approved_by": {"kind": "owner", "id": "owner"}, "method": method, "approved_at": at,
                                  "steps_digest": automation::steps_digest(&doc["steps"])});
        let report = validate(Contract::Workflow, &doc);
        if !report.is_valid() {
            return Err(invalid(format!(
                "automazione non conforme: {}",
                report.schema.join("; ")
            )));
        }
        if doc["trigger"] != current["trigger"]
            && automation::first_run(&doc["trigger"], chrono::Utc::now(), &chrono::Local).is_none()
        {
            return Err(invalid("la data indicata è già passata".into()));
        }
        Ok((doc, changes))
    }

    fn apply_update(
        &mut self,
        current: &Value,
        next: Option<String>,
        args: &Value,
        method: &str,
    ) -> Result<Value, tools::ToolError> {
        let (doc, changes) = self.build_update(current, args, method)?;
        // Una nuova frequenza riparte da adesso; altrimenti resta la prossima esecuzione prevista.
        let next = if doc["enabled"] != true {
            None
        } else if doc["trigger"] != current["trigger"] {
            automation::first_run(&doc["trigger"], chrono::Utc::now(), &chrono::Local)
                .map(automation::ts)
        } else {
            next
        };
        self.store
            .workflow_put(&doc, next.as_deref())
            .and_then(|()| self.store.workflow_version_put(&doc))
            .map_err(|e| tool_error("workflow.unavailable", e.to_string()))?;
        Ok(json!({
            "id": short(&doc), "workflow_id": doc["workflow_id"], "name": doc["name"], "version": doc["version"],
            "changes": changes, "trigger": automation::describe(&doc["trigger"]),
            "next_run": next.as_deref().map(automation::fmt_local),
        }))
    }

    /// Modifica dall'interfaccia: il proprietario autenticato approva direttamente la nuova versione.
    pub fn update_workflow(&mut self, key: &str, body: &Value) -> Result<Value, CoreError> {
        let (current, next) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        let out = self
            .apply_update(&current, next, body, "owner_ui")
            .map_err(|e| match e.code {
                "workflow.unavailable" => CoreError::Internal(e.message),
                _ => CoreError::BadRequest(e.message),
            })?;
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "workflow.update", "outcome": "succeeded", "effect": "applied"}))?;
        Ok(out)
    }

    pub fn workflow_versions(&self, key: &str) -> Result<Vec<Value>, CoreError> {
        let (doc, _) = self
            .find_workflow(key)
            .map_err(|e| CoreError::NotFound(e.message))?;
        self.store
            .workflow_versions(doc["workflow_id"].as_str().unwrap_or_default())
            .map_err(internal)
    }
}

fn merge(base: &Value, extra: Value) -> Value {
    let mut m = obj(base.clone());
    m.extend(obj(extra));
    Value::Object(m)
}
