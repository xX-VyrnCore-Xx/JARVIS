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
use crate::{now, policy, tools};

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
    fn new(message: &str) -> Self {
        let at = now();
        let objective: String = message.chars().take(4000).collect();
        Self {
            doc: json!({
                "contract": "jarvis.task/1",
                "task_id": uuid::Uuid::new_v4().to_string(),
                "correlation_id": uuid::Uuid::new_v4().to_string(),
                "requester": {"kind": "owner", "id": "owner"},
                "objective": objective,
                "success_conditions": ["Risposta fornita al proprietario", "Ogni strumento eseguito ha superato la policy"],
                "dependencies": [],
                "scope": {"nodes": ["core-local"]},
                "state": "pending",
                "history": [{"to": "pending", "at": at, "by": {"kind": "owner", "id": "owner"}, "reason": "Messaggio ricevuto"}],
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
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        json!({
            "task": self.task,
            "reply": self.reply,
            "confirmation": self.confirmation,
            "steps": self.steps,
            "backend": self.backend,
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
        let errors = self.audit.verify().unwrap_or_else(|e| vec![e.to_string()]);
        let events = self.audit.read_all().map(|v| v.len()).unwrap_or(0);
        json!({
            "core": {"version": env!("CARGO_PKG_VERSION"), "status": "up"},
            "ai": ai,
            "audit": {"events": events, "chain_valid": errors.is_empty()},
            "pending_confirmations": self.pending.len(),
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
        let mut task = Task::new(message);
        self.store
            .message_add("user", message, &task.id(), &now())
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
            &policy::Request {
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
                    .message_add("assistant", &text, &task.id(), &now())
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
                let (status, output) = self.run_tool(&task, &tool, &arguments, &mut out)?;
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
            let (status, output) = self.run_tool(&task, &tool, &arguments, &mut out)?;
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
        out: &mut Outcome,
    ) -> Result<(&'static str, Value), CoreError> {
        let workspace = self.cfg.workspace();
        let result = tools::execute(
            tool,
            arguments,
            &tools::Context {
                workspace: &workspace,
                store: &self.store,
            },
        );
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
            .message_add("assistant", &text, &task.id(), &now())
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
            },
        )
        .map_err(|e| CoreError::Internal(e.message))
    }
}

fn merge(base: &Value, extra: Value) -> Value {
    let mut m = obj(base.clone());
    m.extend(obj(extra));
    Value::Object(m)
}
