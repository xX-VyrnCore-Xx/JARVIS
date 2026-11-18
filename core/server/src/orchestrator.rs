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
use crate::observability::{Observability, Timer};
use crate::store::Store;
use crate::{automation, now, policy, tools};

/// Memoria nel contesto dell'agente (ADR-0022).
const MEMORY_PROCEDURAL: usize = 20;
const MEMORY_SMALL: usize = 30;
const MEMORY_RELEVANT: usize = 8;
const HISTORY_MESSAGES: u32 = 10;
/// Budget del contesto in caratteri (circa 3,5 caratteri per token): cronologia e memoria (ADR-0024).
const CONTEXT_CHARS: usize = 9000;
const MEMORY_CHARS: usize = 3000;
/// Lunghezza massima dei turni più vecchi: la richiesta corrente resta intera.
const OLD_TURN_CHARS: usize = 700;
const MAX_MESSAGE: usize = 8000;

#[derive(Debug)]
pub enum CoreError {
    BadRequest(String),
    /// Capacità non disponibile su questo nodo (es. motori vocali non installati).
    Unavailable(String),
    NotFound(String),
    Gone(String),
    /// Credenziale o firma non valida (es. webhook dei connettori).
    Unauthorized(String),
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

/// Destinazione dei frammenti di risposta durante una chat in streaming (ADR-0025).
pub type StreamSink = Box<dyn Fn(&str) + Send>;

pub struct Jarvis {
    stream_sink: Option<StreamSink>,
    oauth: crate::oauth::Flows,
    cfg: Config,
    store: Store,
    audit: AuditLog,
    catalog: Catalog,
    worker: Option<Worker>,
    pending: HashMap<String, Pending>,
    obs: std::sync::Arc<Observability>,
    secrets: crate::secrets::SecretStore,
    /// Ultima istantanea delle metriche salvata (RNF-07).
    metrics_saved: Option<std::time::Instant>,
    /// Codici di associazione monouso in attesa (RF-11): codice -> (scadenza, permessi).
    pairings: HashMap<String, (std::time::Instant, Vec<String>)>,
    /// Nodi esecutori (RF-11, ADR-0017): assegnazioni e token, con un blocco proprio.
    nodes: std::sync::Arc<crate::nodes::NodeHub>,
    /// Configurazione guidata dei modelli (ADR-0019), con stato proprio fuori dal blocco del Core.
    setup: std::sync::Arc<crate::setup::Setup>,
    node_timeout: std::time::Duration,
    pair_failures: u32,
    /// Blocco esclusivo sulla cartella dati: un solo Core per volta (rilasciato alla chiusura).
    _lock: std::fs::File,
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
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(cfg.data_dir.join(".lock"))
            .map_err(|e| format!("blocco della cartella dati: {e}"))?;
        // Il blocco (flock) appartiene al file aperto: un Core appena chiuso, o un processo figlio nato da
        // un altro thread nell'istante fra fork ed exec, può trattenerlo per poco. Si riprova fino a 3 s.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while lock.try_lock().is_err() {
            if std::time::Instant::now() >= deadline {
                return Err(format!(
                    "un altro Core JARVIS sta già usando la cartella dati {}",
                    cfg.data_dir.display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Rollback di release: copia dei dati prima che una versione diversa apra (e migri) il database.
        let upgraded = crate::rollback::prepare(&cfg.data_dir)?;
        std::fs::create_dir_all(cfg.data_dir.join("audit")).map_err(|e| format!("audit: {e}"))?;
        let store = Store::open(&cfg.data_dir.join("jarvis.sqlite3"))
            .map_err(|e| format!("database: {e}"))?;
        let audit = AuditLog::open(
            &cfg.data_dir.join("audit/core-security.jsonl"),
            "core-security",
        )
        .map_err(|e| format!("audit: {e}"))?;
        let catalog = Catalog::load()?;
        let secrets = crate::secrets::SecretStore::open(&cfg.data_dir)?;
        let cfg_logs = cfg.data_dir.join("logs");
        let (cfg_setup_dir, cfg_setup_python, cfg_setup_ai) =
            (cfg.data_dir.clone(), cfg.python.clone(), cfg.ai_dir.clone());
        let mut jarvis = Self {
            stream_sink: None,
            oauth: crate::oauth::Flows::default(),
            cfg,
            store,
            audit,
            catalog,
            worker: None,
            pending: HashMap::new(),
            obs: std::sync::Arc::new(Observability::new(Some(&cfg_logs))),
            secrets,
            metrics_saved: None,
            pairings: HashMap::new(),
            nodes: std::sync::Arc::new(crate::nodes::NodeHub::default()),
            setup: std::sync::Arc::new(crate::setup::Setup::new(
                &cfg_setup_dir,
                &cfg_setup_python,
                &cfg_setup_ai,
            )),
            node_timeout: std::time::Duration::from_millis(
                std::env::var("JARVIS_NODE_TIMEOUT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30_000),
            ),
            pair_failures: 0,
            _lock: lock,
        };
        jarvis.register_authorized_connectors()?;
        jarvis.refresh_embedder();
        jarvis.enable_audit_signing()?;
        jarvis.register_nodes()?;
        crate::rollback::mark(&jarvis.cfg.data_dir)?;
        if let Some(up) = upgraded {
            jarvis.obs.log(
                "info",
                "release.upgrade",
                json!({"from": up["from"], "to": up["to"]}),
            );
            jarvis.notify(json!({"severity": "info", "category": "system",
                "title": format!("JARVIS aggiornato da {} a {}", up["from"].as_str().unwrap_or("?"), up["to"].as_str().unwrap_or("?")),
                "body": "I dati precedenti sono stati copiati per il rollback: con JARVIS chiuso, «./jarvis.sh rollback» li ripristina.",
                "source": {"kind": "module", "id": "release"}})).map_err(|e| format!("{e:?}"))?;
        }
        Ok(jarvis)
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn observability(&self) -> std::sync::Arc<Observability> {
        self.obs.clone()
    }

    // ------------------------------------------------------------------ worker

    fn worker(&mut self) -> Result<&mut Worker, Value> {
        if self.worker.as_mut().is_some_and(|w| w.has_exited()) {
            self.worker = None;
        }
        // Dopo la configurazione dei modelli il worker riparte con i nuovi percorsi (voce, Ollama).
        if self.setup.take_restart() {
            self.worker = None;
            self.refresh_embedder();
            self.obs
                .log("info", "worker.restart", json!({"reason": "setup"}));
        }
        if self.worker.is_none() {
            let mut cmd = Command::new(&self.cfg.python);
            crate::hide_console(&mut cmd);
            cmd.args(["-m", "jarvis_ai.worker", "--handler", "assistant"])
                .current_dir(&self.cfg.ai_dir);
            if let Some(backend) = &self.cfg.ai_backend {
                cmd.env("JARVIS_AI_BACKEND", backend);
            }
            if let Some(engine) = &self.cfg.voice_engine {
                cmd.env("JARVIS_VOICE_ENGINE", engine);
            }
            for (k, v) in self.setup.worker_env() {
                cmd.env(k, v);
            }
            for (k, v) in &self.cfg.worker_env {
                cmd.env(k, v);
            }
            let spawned = Worker::spawn(cmd).map_err(|e| e.to_contract());
            self.obs.count(if spawned.is_ok() {
                "worker.spawned"
            } else {
                "worker.spawn_failed"
            });
            self.obs.log(
                if spawned.is_ok() { "info" } else { "error" },
                "worker.spawn",
                json!({"ok": spawned.is_ok()}),
            );
            self.worker = Some(spawned?);
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
            "core": {"version": env!("CARGO_PKG_VERSION"), "status": "up", "secrets_store": self.secrets.location(), "memory_embeddings": self.embed_model()},
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
            "tools": self.delegated_tools().map(|t| json!({"name": t.name(), "version": t.version()})).collect::<Vec<_>>(),
            "limits": {"max_steps": 4, "max_subdelegations": 0, "allow_cloud": false},
            "timeout_ms": self.cfg.delegation_timeout_ms,
            "completion_conditions": ["Esito jarvis.agent-result/1 valido"],
            "issued_at": ts(issued),
            "expires_at": ts(expires),
            "inputs": self.with_dynamic_tools(inputs),
            "stream": self.stream_sink.is_some(),
        })
    }

    /// Strumenti delegati all'agente: prima il catalogo statico, poi al massimo 16 strumenti dei connettori
    /// (limite di 32 del contratto di delega).
    fn delegated_tools(&self) -> impl Iterator<Item = &crate::catalog::Tool> {
        fn dynamic(t: &&crate::catalog::Tool) -> bool {
            t.name().starts_with("connector.") || t.name().starts_with("node.")
        }
        self.catalog
            .iter()
            .filter(|t| !dynamic(t))
            .chain(self.catalog.iter().filter(dynamic).take(16))
            .take(32)
    }

    /// Aggiunge agli input i descrittori dei connettori autorizzati, che il catalogo statico dell'agente non conosce.
    fn with_dynamic_tools(&self, inputs: &Value) -> Value {
        let dynamic: Vec<Value> = self
            .delegated_tools()
            .filter(|t| t.name().starts_with("connector.") || t.name().starts_with("node."))
            .map(|t| json!({"name": t.name(), "version": t.version(), "summary": t.descriptor["summary"], "input_schema": t.descriptor["input_schema"]}))
            .collect();
        if dynamic.is_empty() || !inputs.is_object() {
            return inputs.clone();
        }
        merge(inputs, json!({"tool_descriptors": dynamic}))
    }

    /// Delega all'agente; restituisce l'esito `jarvis.agent-result/1` o un errore v1.
    fn call_agent(&mut self, task: &Task, inputs: &Value) -> Result<Value, Value> {
        let timer = Timer::start();
        let result = self.call_agent_inner(task, inputs);
        self.obs
            .observe("delegation general_assistant", timer.ms(), result.is_err());
        if let Ok(r) = &result {
            // Consumo di token riportato dal backend (ADR-0024): contatori senza contenuti.
            for (key, counter) in [
                ("prompt_tokens", "ai.tokens.prompt"),
                ("output_tokens", "ai.tokens.output"),
                ("tools_sent", "ai.tools.sent"),
            ] {
                if let Some(n) = r["usage"][key].as_u64() {
                    self.obs.add(counter, n);
                }
            }
        }
        if let Err(e) = &result {
            self.obs.count("delegation.failed");
            self.obs.log(
                "warn",
                "delegation.failed",
                json!({"task_id": task.id(), "correlation_id": task.correlation(),
                "code": e["code"], "duration_ms": timer.ms()}),
            );
        }
        result
    }

    fn call_agent_inner(&mut self, task: &Task, inputs: &Value) -> Result<Value, Value> {
        let delegation = self.delegation(task, inputs);
        let (id, corr) = (task.id(), task.correlation());
        let sink = self.stream_sink.take();
        let worker = match self.worker() {
            Ok(w) => w,
            Err(e) => {
                self.stream_sink = sink;
                return Err(e);
            }
        };
        let sent = match &sink {
            Some(f) => worker.delegate_streaming(&id, &corr, &delegation, &mut |d| f(d)),
            None => worker.delegate(&id, &corr, &delegation),
        };
        self.stream_sink = sink;
        match sent {
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
        let recent = self
            .store
            .messages_recent(HISTORY_MESSAGES)
            .map_err(internal)?;
        let messages = budget_history(&recent, context_budget());
        let query = messages
            .iter()
            .rev()
            .find(|m| m["role"] == "user")
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_string();
        let memory = budget_memory(self.memory_context(&query)?, MEMORY_CHARS);
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

    /// Chat con i frammenti della risposta inoltrati a `sink` mentre il modello li genera (ADR-0025).
    /// L'esito finale è lo stesso di `chat`: policy, conferme e audit non cambiano.
    pub fn chat_streaming(
        &mut self,
        message: &str,
        sink: StreamSink,
    ) -> Result<Outcome, CoreError> {
        self.stream_sink = Some(sink);
        let out = self.chat(message);
        self.stream_sink = None;
        out
    }

    pub fn chat(&mut self, message: &str) -> Result<Outcome, CoreError> {
        let timer = Timer::start();
        let out = self.chat_inner(message);
        self.record_task("chat", &timer, &out);
        out
    }

    /// Metriche e log di fine richiesta: identificativi, stato e durata, mai il contenuto.
    fn record_task(&self, kind: &str, timer: &Timer, out: &Result<Outcome, CoreError>) {
        let ms = timer.ms();
        match out {
            Ok(o) => {
                let state = o.task["state"].as_str().unwrap_or("unknown");
                self.obs
                    .observe(&format!("task {kind}"), ms, state == "failed");
                self.obs.log(if state == "failed" { "warn" } else { "info" }, "task.finished", json!({
                    "kind": kind, "task_id": o.task["task_id"], "correlation_id": o.task["correlation_id"], "state": state,
                    "steps": o.steps.len(), "error_code": o.task["error"]["code"], "duration_ms": (ms * 10.0).round() / 10.0,
                }));
            }
            Err(e) => {
                self.obs.observe(&format!("task {kind}"), ms, true);
                let kind_of = match e {
                    CoreError::BadRequest(_) => "bad_request",
                    CoreError::NotFound(_) => "not_found",
                    CoreError::Gone(_) => "gone",
                    CoreError::Unauthorized(_) => "unauthorized",
                    CoreError::Unavailable(_) => "unavailable",
                    CoreError::Internal(_) => "internal",
                };
                self.obs.log(
                    "warn",
                    "task.rejected",
                    json!({"kind": kind, "reason": kind_of}),
                );
            }
        }
    }

    fn chat_inner(&mut self, message: &str) -> Result<Outcome, CoreError> {
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
        let timer = Timer::start();
        let out = self.confirm_inner(confirmation_id, approve);
        self.record_task("confirm", &timer, &out);
        out
    }

    fn confirm_inner(
        &mut self,
        confirmation_id: &str,
        approve: bool,
    ) -> Result<Outcome, CoreError> {
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
                let effect = self.failure_effect(tool, e.code);
                self.audit(merge(
                    &base,
                    json!({"outcome": "failed", "effect": effect, "error_code": e.code}),
                ))?;
                out.steps.push(json!({"kind": "tool_result", "tool": tool, "status": "failed", "error": e.code, "effect": effect}));
                self.notify_connector_failure(task, tool, &e, effect)?;
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
        self.record_episode(&task, &out);
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

    /// Firma del registro di audit (ADR-0030): chiave Ed25519 generata al primo avvio e custodita
    /// nell'archivio dei segreti.
    fn enable_audit_signing(&mut self) -> Result<(), String> {
        let name = "audit:signing-key";
        let seed = match self.secrets.get(name)? {
            Some(hexkey) => hex::decode(hexkey)
                .map_err(|_| "chiave di firma dell'audit danneggiata".to_string())?,
            None => {
                let mut k = [0u8; 32];
                getrandom::getrandom(&mut k).map_err(|e| e.to_string())?;
                self.secrets.put(name, &hex::encode(k))?;
                k.to_vec()
            }
        };
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| "chiave di firma dell'audit danneggiata".to_string())?;
        self.audit.set_signer(&seed);
        Ok(())
    }

    /// Attiva gli embedding della memoria se è scelto un modello (ADR-0028). Senza Ollama la memoria usa
    /// l'indice locale; gli embedding mancanti vengono calcolati appena il servizio risponde.
    fn refresh_embedder(&mut self) {
        let embedder = self.setup.embed_model().map(|m| {
            std::sync::Arc::new(crate::memory::OllamaEmbedder::new(
                self.setup.ollama_url(),
                &m,
            )) as std::sync::Arc<dyn crate::memory::Embedder>
        });
        match self.store.set_embedder(embedder) {
            Ok(n) if n > 0 => self.obs.log("info", "memory.embedded", json!({"items": n})),
            Ok(_) => {}
            Err(e) => self.obs.log(
                "warn",
                "memory.embed_failed",
                json!({"error": e.to_string()}),
            ),
        }
    }

    /// Modello di embedding attivo, per lo stato del sistema.
    pub fn embed_model(&self) -> Option<String> {
        self.store.embedder.as_ref().map(|e| e.model().to_string())
    }

    /// Memoria data all'agente (ADR-0022): tutte le preferenze, poi i fatti e gli episodi pertinenti alla
    /// richiesta. Con una memoria piccola i fatti sono inclusi tutti, come prima.
    fn memory_context(&self, query: &str) -> Result<Vec<Value>, CoreError> {
        let _ = self.store.memory_purge_expired(&now());
        let mut out: Vec<Value> = vec![];
        let mut seen = std::collections::HashSet::new();
        for m in self
            .store
            .memory_list_kind(Some("procedural"))
            .map_err(internal)?
            .into_iter()
            .take(MEMORY_PROCEDURAL)
        {
            seen.insert(m["id"].as_str().unwrap_or_default().to_string());
            out.push(format!("[preferenza] {}", m["text"].as_str().unwrap_or_default()).into());
        }
        let semantic = self
            .store
            .memory_list_kind(Some("semantic"))
            .map_err(internal)?;
        let relevant = if semantic.len() <= MEMORY_SMALL {
            semantic
        } else {
            vec![]
        };
        let found = if query.trim().is_empty() {
            vec![]
        } else {
            self.store
                .memory_search(query, None, MEMORY_RELEVANT)
                .map_err(internal)?
        };
        for m in relevant.into_iter().chain(found) {
            let id = m["id"].as_str().unwrap_or_default().to_string();
            if !seen.insert(id) {
                continue;
            }
            let text = m["text"].as_str().unwrap_or_default();
            out.push(
                match m["kind"].as_str() {
                    Some("episodic") => format!(
                        "[episodio del {}] {text}",
                        m["created_at"]
                            .as_str()
                            .unwrap_or_default()
                            .get(..10)
                            .unwrap_or_default()
                    ),
                    Some("procedural") => format!("[preferenza] {text}"),
                    _ => text.to_string(),
                }
                .into(),
            );
        }
        Ok(out)
    }

    /// Memoria episodica: un riassunto per ogni attività conclusa dalla chat, legato al task e con scadenza
    /// (`JARVIS_EPISODIC_DAYS`, 30 predefiniti, 0 disattiva). Le operazioni sulla memoria non generano episodi,
    /// così un fatto cancellato non sopravvive in un riassunto.
    fn record_episode(&self, task: &Task, out: &Outcome) {
        let days: i64 = std::env::var("JARVIS_EPISODIC_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        if days <= 0 {
            return;
        }
        let tool = out
            .steps
            .iter()
            .find(|s| s["kind"] == "tool_call")
            .and_then(|s| s["tool"].as_str());
        if tool.is_some_and(|t| t.starts_with("memory.")) {
            return;
        }
        let objective: String = task.doc["objective"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(160)
            .collect();
        if objective.trim().is_empty() {
            return;
        }
        let state = task.doc["state"].as_str().unwrap_or("unknown");
        let text = match tool {
            Some(t) => format!("Richiesta «{objective}»: {t}, esito {state}"),
            None => format!("Richiesta «{objective}»: risposta diretta, esito {state}"),
        };
        let at = now();
        let expires = (chrono::Utc::now() + chrono::Duration::days(days))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        let _ = self.store.memory_insert(
            &crate::memory::NewItem {
                id: &id,
                text: &text,
                kind: "episodic",
                source: json!({"kind": "task", "task_id": task.id()}),
                expires_at: Some(expires),
            },
            &at,
        );
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

    /// Elenco filtrato per tipo o, con `query`, ricerca ibrida ordinata per pertinenza.
    pub fn memory_query(
        &self,
        kind: Option<&str>,
        query: Option<&str>,
    ) -> Result<Vec<Value>, CoreError> {
        if let Some(k) = kind {
            if !crate::memory::KINDS.contains(&k) {
                return Err(CoreError::BadRequest(format!(
                    "tipo di memoria sconosciuto: {k}"
                )));
            }
        }
        let _ = self.store.memory_purge_expired(&now());
        match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => self.store.memory_search(q, kind, 20).map_err(internal),
            None => self.store.memory_list_kind(kind).map_err(internal),
        }
    }

    /// Correzione diretta del proprietario: aggiorna testo e indici derivati, con audit.
    pub fn memory_correct(&mut self, id: &str, text: &str) -> Result<(), CoreError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > 1000 {
            return Err(CoreError::BadRequest(
                "testo vuoto o oltre 1000 caratteri".into(),
            ));
        }
        if !self
            .store
            .memory_update(id, text, &now())
            .map_err(internal)?
        {
            return Err(CoreError::NotFound(format!(
                "nessun elemento di memoria con id {id}"
            )));
        }
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "memory.correct", "outcome": "succeeded", "effect": "applied"}))?;
        Ok(())
    }

    pub fn memory_graph(&self) -> Result<Value, CoreError> {
        self.store.memory_graph().map_err(internal)
    }

    /// Ricostruzione degli indici derivati dalla tabella sorgente.
    pub fn memory_reindex(&mut self) -> Result<usize, CoreError> {
        let n = self.store.memory_reindex().map_err(internal)?;
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "memory.reindex", "outcome": "succeeded", "effect": "applied"}))?;
        Ok(n)
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
        let signatures = self.audit.verify_signatures().map_err(internal)?;
        Ok(
            json!({"total": total, "chain_valid": errors.is_empty(), "chain_errors": errors, "events": recent, "signatures": signatures}),
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
        let timer = Timer::start();
        let result = self.execute_tool_inner(tool, arguments, confirmed);
        self.obs
            .observe(&format!("tool {tool}"), timer.ms(), result.is_err());
        if let Err(e) = &result {
            self.obs.log("warn", "tool.failed", json!({"tool": tool, "code": e.code, "duration_ms": (timer.ms() * 10.0).round() / 10.0}));
        }
        result
    }

    fn execute_tool_inner(
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
            t if t.starts_with("connector.") => self.call_connector(t, arguments, confirmed),
            t if t.starts_with("node.") => self.call_node(t, arguments, confirmed),
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
        let timer = Timer::start();
        let run = self.run_workflow_inner(doc, kind, scheduled_for);
        let status = run
            .as_ref()
            .map(|r| r["status"].as_str().unwrap_or("unknown").to_string())
            .unwrap_or_else(|_| "error".into());
        self.obs.observe(
            &format!("workflow {kind}"),
            timer.ms(),
            status != "succeeded",
        );
        self.obs.log(if status == "succeeded" { "info" } else { "warn" }, "workflow.run", json!({
            "workflow_id": doc["workflow_id"], "version": doc["version"], "trigger": kind, "status": status,
            "task_id": run.as_ref().ok().map(|r| r["task_id"].clone()), "duration_ms": (timer.ms() * 10.0).round() / 10.0,
        }));
        run
    }

    fn run_workflow_inner(
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
                    let effect = self.failure_effect(&tool, e.code);
                    self.audit(merge(
                        &base,
                        json!({"outcome": "failed", "effect": effect, "error_code": e.code}),
                    ))?;
                    self.notify_connector_failure(&task, &tool, &e, effect)?;
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
        self.obs.count(&format!(
            "notification.{}",
            doc["severity"].as_str().unwrap_or("info")
        ));
        self.obs.log("info", "notification", json!({"severity": doc["severity"], "category": doc["category"],
            "task_id": doc["task_id"], "workflow_id": doc["workflow_id"], "correlation_id": doc["correlation_id"]}));
        self.notify_external(&doc);
        Ok(Some(doc))
    }

    /// Inoltro ai canali esterni configurati (ADR-0026), in background e fuori dal blocco del Core.
    fn notify_external(&self, doc: &Value) {
        let Ok(channels) = self.store.channels() else {
            return;
        };
        let targets: Vec<(Value, Option<String>)> = channels
            .into_iter()
            .filter(|c| crate::channels::matches(c, doc))
            .map(|c| {
                let secret = self
                    .secrets
                    .get(&format!(
                        "channel:{}",
                        c["channel_id"].as_str().unwrap_or_default()
                    ))
                    .ok()
                    .flatten();
                (c, secret)
            })
            .collect();
        crate::channels::dispatch(targets, doc.clone(), self.obs.clone());
    }

    // ------------------------------------------------------------------ canali di notifica (ADR-0026)

    /// Canali configurati, senza segreti: `has_secret` dice solo se un token è salvato.
    pub fn channels(&self) -> Result<Vec<Value>, CoreError> {
        Ok(self
            .store
            .channels()
            .map_err(internal)?
            .into_iter()
            .map(|mut c| {
                let id = c["channel_id"].as_str().unwrap_or_default().to_string();
                c["has_secret"] = self
                    .secrets
                    .get(&format!("channel:{id}"))
                    .ok()
                    .flatten()
                    .is_some()
                    .into();
                c
            })
            .collect())
    }

    pub fn channel_add(&mut self, input: &Value) -> Result<Value, CoreError> {
        let mut doc = crate::channels::normalize(input).map_err(CoreError::BadRequest)?;
        let secret = input["secret"].as_str().map(str::trim).unwrap_or_default();
        if doc["kind"] == "telegram" && secret.is_empty() {
            return Err(CoreError::BadRequest(
                "serve il token del bot Telegram".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().simple().to_string()[..10].to_string();
        doc["channel_id"] = id.clone().into();
        doc["created_at"] = now().into();
        if !secret.is_empty() {
            self.secrets
                .put(&format!("channel:{id}"), secret)
                .map_err(CoreError::Internal)?;
        }
        self.store.channel_put(&doc).map_err(internal)?;
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "notify.channel.add", "outcome": "succeeded", "effect": "applied"}))?;
        doc["has_secret"] = (!secret.is_empty()).into();
        Ok(doc)
    }

    pub fn channel_delete(&mut self, id: &str) -> Result<(), CoreError> {
        if !self.store.channel_delete(id).map_err(internal)? {
            return Err(CoreError::NotFound(format!("nessun canale con id {id}")));
        }
        let _ = self.secrets.delete(&format!("channel:{id}"));
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "notify.channel.delete", "outcome": "succeeded", "effect": "applied"}))?;
        Ok(())
    }

    /// Canale e segreto per una prova d'invio eseguita dall'API fuori dal blocco del Core.
    pub fn channel_for_test(&self, id: &str) -> Result<(Value, Option<String>), CoreError> {
        let channel = self
            .store
            .channels()
            .map_err(internal)?
            .into_iter()
            .find(|c| c["channel_id"] == id)
            .ok_or_else(|| CoreError::NotFound(format!("nessun canale con id {id}")))?;
        let secret = self.secrets.get(&format!("channel:{id}")).ok().flatten();
        Ok((channel, secret))
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
        self.persist_metrics(false)?;
        if self.worker.as_mut().is_some_and(|w| w.has_exited()) {
            self.worker = None;
            self.obs.count("worker.crashed");
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

// ---------------------------------------------------------------------- Voce (RF-09)

impl Jarvis {
    fn voice_call(&mut self, kind: &str, payload: Value) -> Result<Value, CoreError> {
        let (t, c) = (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        let worker = self.worker().map_err(|e| {
            CoreError::Unavailable(
                e["message"]
                    .as_str()
                    .unwrap_or("worker non disponibile")
                    .into(),
            )
        })?;
        let env = worker.request(
            &t,
            &c,
            json!({"kind": "module", "id": "ai-worker"}),
            kind,
            &payload,
            Duration::from_secs(120),
        );
        match env {
            Ok(env) if env["status"] == "succeeded" => Ok(env["payload"].clone()),
            Ok(env) => {
                let message = env["error"]["message"]
                    .as_str()
                    .unwrap_or("errore vocale")
                    .to_string();
                Err(match env["error"]["code"].as_str() {
                    Some("voice.unavailable") => CoreError::Unavailable(message),
                    Some("voice.invalid_result") => CoreError::Internal(message),
                    _ => CoreError::BadRequest(message),
                })
            }
            Err(e) => {
                self.worker = None;
                Err(CoreError::Unavailable(e.message))
            }
        }
    }

    /// Trascrive un WAV registrato dal client. L'audio non viene salvato: resta solo il testo restituito.
    pub fn voice_transcribe(&mut self, wav: &[u8], wake_word: bool) -> Result<Value, CoreError> {
        use base64::Engine as _;
        if wav.is_empty() {
            return Err(CoreError::BadRequest("audio vuoto".into()));
        }
        let mode = if wake_word {
            "wake_word"
        } else {
            "push_to_talk"
        };
        let out = self.voice_call("voice.transcribe", json!({"audio_b64": base64::engine::general_purpose::STANDARD.encode(wav), "mode": mode}))?;
        if !validate(Contract::VoiceTranscription, &out).is_valid() {
            return Err(CoreError::Internal("trascrizione non conforme".into()));
        }
        Ok(out)
    }

    /// Sintetizza un testo; restituisce l'audio WAV e i metadati (senza audio).
    pub fn voice_speak(&mut self, text: &str) -> Result<(Vec<u8>, Value), CoreError> {
        use base64::Engine as _;
        let text: String = text.trim().chars().take(4000).collect();
        if text.is_empty() {
            return Err(CoreError::BadRequest("testo vuoto".into()));
        }
        let mut out = self.voice_call("voice.synthesize", json!({"text": text}))?;
        if !validate(Contract::VoiceSynthesis, &out).is_valid() {
            return Err(CoreError::Internal("sintesi non conforme".into()));
        }
        let audio = base64::engine::general_purpose::STANDARD
            .decode(out["audio_b64"].as_str().unwrap_or_default())
            .map_err(|_| CoreError::Internal("audio non decodificabile".into()))?;
        if let Some(map) = out.as_object_mut() {
            map.remove("audio_b64");
        }
        Ok((audio, out))
    }
}

// ---------------------------------------------------------------------- Metriche persistenti (RNF-07)

/// Intervallo fra due istantanee salvate e periodo di conservazione.
const METRICS_EVERY: std::time::Duration = std::time::Duration::from_secs(300);
const METRICS_KEEP_DAYS: i64 = 7;

impl Jarvis {
    /// Salva un'istantanea compatta delle metriche (conteggi, errori, p95 per serie) al massimo ogni
    /// 5 minuti, o subito con `force`. Le istantanee non contengono contenuti dell'utente.
    pub fn persist_metrics(&mut self, force: bool) -> Result<Option<Value>, CoreError> {
        if !force
            && self
                .metrics_saved
                .is_some_and(|t| t.elapsed() < METRICS_EVERY)
        {
            return Ok(None);
        }
        let snap = self.obs.snapshot();
        let at = now();
        let doc = json!({"at": at, "uptime_s": snap["uptime_s"], "series": snap["series"], "counters": snap["counters"]});
        let keep = (chrono::Utc::now() - chrono::Duration::days(METRICS_KEEP_DAYS))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.store.metrics_put(&at, &doc, &keep).map_err(internal)?;
        self.metrics_saved = Some(std::time::Instant::now());
        Ok(Some(doc))
    }

    /// Istantanee delle ultime `hours` ore (al massimo 7 giorni), anche di sessioni precedenti.
    pub fn metrics_history(&self, hours: u32) -> Result<Vec<Value>, CoreError> {
        let since = (chrono::Utc::now()
            - chrono::Duration::hours(i64::from(hours.clamp(1, 24 * 7))))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.store.metrics_since(&since).map_err(internal)
    }
}

// ---------------------------------------------------------------------- Dispositivi associati (RF-11)

const PAIRING_TTL: std::time::Duration = std::time::Duration::from_secs(600);
const PAIRING_MAX_FAILURES: u32 = 5;
/// Alfabeto senza caratteri ambigui (0/O, 1/I/L): 8 caratteri = 40 bit.
const PAIRING_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

fn sha256_hex(s: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(s.as_bytes()))
}

impl Jarvis {
    fn device_audit(
        &mut self,
        action: &str,
        device_id: &str,
        outcome: &str,
    ) -> Result<(), CoreError> {
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": action, "resource": format!("jarvis://device/{device_id}"),
                          "outcome": outcome, "effect": "applied"}))?;
        Ok(())
    }

    /// Crea un codice monouso valido 10 minuti. Solo il proprietario, dal computer del Core.
    pub fn create_pairing(&mut self, scopes: &[String]) -> Result<Value, CoreError> {
        let mut scopes: Vec<String> = scopes
            .iter()
            .filter(|s| matches!(s.as_str(), "chat" | "confirm" | "node"))
            .cloned()
            .collect();
        if scopes.iter().any(|s| s == "node") {
            // Un nodo esecutore non chatta e non conferma: riceve solo assegnazioni.
            scopes = vec!["node".into()];
        } else if !scopes.iter().any(|s| s == "chat") {
            scopes.insert(0, "chat".into());
        }
        scopes.dedup();
        let now_i = std::time::Instant::now();
        self.pairings.retain(|_, (exp, _)| *exp > now_i);
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).map_err(internal)?;
        let code: String = bytes
            .iter()
            .map(|b| PAIRING_ALPHABET[*b as usize % PAIRING_ALPHABET.len()] as char)
            .collect();
        self.pairings
            .insert(code.clone(), (now_i + PAIRING_TTL, scopes.clone()));
        self.pair_failures = 0;
        let expires = chrono::Utc::now() + chrono::Duration::seconds(PAIRING_TTL.as_secs() as i64);
        Ok(
            json!({"code": code, "scopes": scopes, "expires_at": expires.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)}),
        )
    }

    /// Associa un dispositivo con un codice valido. Il token è restituito una sola volta; il Core ne
    /// conserva solo l'impronta SHA-256. Dopo 5 codici errati tutti i codici in attesa vengono annullati.
    pub fn pair_device(
        &mut self,
        code: &str,
        name: &str,
        platform: &str,
        capabilities: &[String],
    ) -> Result<Value, CoreError> {
        let code = code.trim().to_ascii_uppercase().replace(['-', ' '], "");
        let now_i = std::time::Instant::now();
        let entry = self.pairings.remove(&code).filter(|(exp, _)| *exp > now_i);
        let Some((_, scopes)) = entry else {
            self.pair_failures += 1;
            self.obs.count("device.pair_rejected");
            if self.pair_failures >= PAIRING_MAX_FAILURES {
                self.pairings.clear();
                self.pair_failures = 0;
                self.notify(json!({"severity": "warning", "category": "security", "title": "Associazione bloccata",
                    "body": "Troppi codici di associazione errati: i codici in attesa sono stati annullati.",
                    "source": {"kind": "module", "id": "devices"}, "dedupe_key": "pairing-blocked"}))?;
            }
            return Err(CoreError::Unauthorized(
                "codice di associazione errato o scaduto".into(),
            ));
        };
        let name: String = name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect();
        let platform = if matches!(platform, "android" | "ios" | "desktop" | "web") {
            platform
        } else {
            "other"
        };
        let mut raw = [0u8; 32];
        getrandom::getrandom(&mut raw).map_err(internal)?;
        let token = hex::encode(raw);
        let mut doc = json!({
            "contract": "jarvis.device/1", "device_id": uuid::Uuid::new_v4().to_string(),
            "name": if name.is_empty() { "Dispositivo".to_string() } else { name },
            "platform": platform, "scopes": scopes, "enrolled_at": now(),
        });
        let is_node = scopes.iter().any(|s| s == "node");
        if is_node {
            let taken: Vec<String> = self
                .store
                .devices()
                .map_err(internal)?
                .iter()
                .filter(|d| d.get("revoked_at").is_none())
                .filter_map(|d| d["slug"].as_str().map(str::to_string))
                .collect();
            let base = crate::nodes::slug(doc["name"].as_str().unwrap_or("nodo"));
            let mut slug = base.clone();
            let mut n = 2;
            while taken.contains(&slug) {
                slug = format!("{base}-{n}");
                n += 1;
            }
            let mut caps: Vec<&str> = crate::nodes::NODE_TOOLS
                .iter()
                .copied()
                .filter(|t| capabilities.iter().any(|c| c == t))
                .collect();
            if caps.is_empty() {
                caps = vec!["system.info", "time.now"];
            }
            doc["slug"] = slug.into();
            doc["capabilities"] = json!(caps);
        }
        let report = validate(Contract::Device, &doc);
        if !report.is_valid() {
            return Err(CoreError::Internal(format!(
                "dispositivo non conforme: {report:?}"
            )));
        }
        self.store
            .device_put(&doc, &sha256_hex(&token))
            .map_err(internal)?;
        let id = doc["device_id"].as_str().unwrap_or_default().to_string();
        if is_node {
            self.register_node(&doc, &sha256_hex(&token))
                .map_err(CoreError::Internal)?;
        }
        self.device_audit("device.pair", &id, "succeeded")?;
        self.notify(json!({"severity": "info", "category": "security", "title": format!("Nuovo dispositivo associato: {}", doc["name"].as_str().unwrap_or("")),
            "body": format!("Permessi: {}. Puoi revocarlo dalla sezione Dispositivi.", scopes.join(", ")),
            "source": {"kind": "module", "id": "devices"}}))?;
        Ok(json!({"device": doc, "token": token}))
    }

    /// Dispositivo attivo per un token (None se sconosciuto o revocato); aggiorna `last_seen_at` al massimo ogni minuto.
    pub fn device_for_token(&mut self, token: &str) -> Result<Option<Value>, CoreError> {
        if token.len() != 64 {
            return Ok(None);
        }
        let hash = sha256_hex(token);
        let Some(mut doc) = self.store.device_by_token(&hash).map_err(internal)? else {
            return Ok(None);
        };
        if doc.get("revoked_at").is_some() {
            return Ok(None);
        }
        let stale = doc["last_seen_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_none_or(|t| {
                chrono::Utc::now().signed_duration_since(t) > chrono::Duration::seconds(60)
            });
        if stale {
            doc["last_seen_at"] = now().into();
            self.store.device_put(&doc, &hash).map_err(internal)?;
        }
        Ok(Some(doc))
    }

    pub fn devices(&self) -> Result<Vec<Value>, CoreError> {
        self.store.devices().map_err(internal)
    }

    /// Revoca immediata: il token smette di funzionare alla richiesta successiva.
    pub fn revoke_device(&mut self, id: &str) -> Result<Value, CoreError> {
        let mut doc = self
            .store
            .devices()
            .map_err(internal)?
            .into_iter()
            .find(|d| d["device_id"] == id)
            .ok_or_else(|| CoreError::NotFound(format!("nessun dispositivo {id}")))?;
        if doc.get("revoked_at").is_none() {
            doc["revoked_at"] = now().into();
            self.store.device_put(&doc, "").map_err(internal)?;
            // L'impronta viene cancellata: il vecchio token non trova più alcun dispositivo.
            self.store.device_retire_token(id).map_err(internal)?;
            if let Some(slug) = doc["slug"].as_str() {
                // Nodo: token, coda e assegnazioni in attesa annullati, strumenti tolti dal catalogo.
                self.nodes.revoke(id);
                self.catalog.remove_prefix(&format!("node.{slug}."));
            }
            self.device_audit("device.revoke", id, "revoked")?;
        }
        Ok(doc)
    }
}

// ---------------------------------------------------------------------- Nodi esecutori (RF-11)

impl Jarvis {
    pub fn setup(&self) -> std::sync::Arc<crate::setup::Setup> {
        self.setup.clone()
    }

    pub fn node_hub(&self) -> std::sync::Arc<crate::nodes::NodeHub> {
        self.nodes.clone()
    }

    fn register_node(&mut self, doc: &Value, hash: &str) -> Result<(), String> {
        let id = doc["device_id"].as_str().unwrap_or_default();
        let slug = doc["slug"].as_str().unwrap_or_default();
        let name = doc["name"].as_str().unwrap_or(slug);
        self.nodes.register(id, hash);
        self.catalog.remove_prefix(&format!("node.{slug}."));
        for cap in doc["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let base = self
                .catalog
                .get(cap)
                .ok_or_else(|| format!("strumento di base sconosciuto: {cap}"))?
                .descriptor
                .clone();
            self.catalog
                .insert_dynamic(crate::nodes::tool_descriptor(&base, slug, name))?;
        }
        Ok(())
    }

    fn register_nodes(&mut self) -> Result<(), String> {
        for (doc, hash) in self.store.devices_with_hash().map_err(|e| e.to_string())? {
            if doc.get("revoked_at").is_none()
                && doc["scopes"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|x| x == "node"))
            {
                self.register_node(&doc, &hash)?;
            }
        }
        Ok(())
    }

    /// Nodi associati con lo stato di connessione (in linea se hanno chiesto lavoro nell'ultimo minuto).
    pub fn nodes(&self) -> Result<Vec<Value>, CoreError> {
        Ok(self
            .store
            .devices()
            .map_err(internal)?
            .into_iter()
            .filter(|d| {
                d["scopes"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|x| x == "node"))
            })
            .map(|mut d| {
                let id = d["device_id"].as_str().unwrap_or_default().to_string();
                d["online"] = (d.get("revoked_at").is_none()
                    && self.nodes.online(&id, std::time::Duration::from_secs(60)))
                .into();
                d
            })
            .collect())
    }

    /// Assegna lo strumento al nodo e attende l'esito. Policy e conferma sono già state applicate dal
    /// Core come per gli strumenti locali; qui si ricontrolla la conferma per le scritture.
    fn call_node(
        &mut self,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
    ) -> Result<Value, tools::ToolError> {
        let rest = tool.strip_prefix("node.").unwrap_or_default();
        let (slug, base) = rest.split_once('.').unwrap_or((rest, ""));
        let descriptor = self
            .catalog
            .get(tool)
            .ok_or_else(|| tool_error("node.revoked", "nodo non associato o revocato"))?;
        if descriptor.needs_confirmation() && !confirmed {
            return Err(tool_error(
                "node.not_confirmed",
                "l'operazione sul nodo richiede la conferma del proprietario",
            ));
        }
        let node = self
            .store
            .devices()
            .map_err(|e| tool_error("node.unavailable", e.to_string()))?
            .into_iter()
            .find(|d| d["slug"] == slug && d.get("revoked_at").is_none())
            .ok_or_else(|| tool_error("node.revoked", "nodo non associato o revocato"))?;
        let id = node["device_id"].as_str().unwrap_or_default().to_string();
        if !self.nodes.online(&id, std::time::Duration::from_secs(60)) {
            return Err(tool_error(
                "node.offline",
                format!(
                    "il nodo {} non è in linea",
                    node["name"].as_str().unwrap_or(slug)
                ),
            ));
        }
        let timer = Timer::start();
        let assignment = self.nodes.enqueue(&id, base, arguments);
        let outcome = self.nodes.wait_result(&assignment, self.node_timeout);
        self.obs
            .observe(&format!("node {slug}"), timer.ms(), outcome.is_err());
        self.obs.log("info", "node.assignment", json!({"node": slug, "tool": base, "ok": outcome.is_ok(), "duration_ms": (timer.ms() * 10.0).round() / 10.0}));
        outcome.map_err(|(code, message)| {
            // I codici del nodo passano solo se ben formati; il messaggio resta testo.
            const PASSTHROUGH: [&str; 10] = [
                "node.timeout",
                "node.revoked",
                "fs.outside_workspace",
                "fs.not_found",
                "fs.not_a_directory",
                "fs.not_a_file",
                "fs.too_large",
                "fs.not_text",
                "fs.is_directory",
                "fs.unavailable",
            ];
            let known: &'static str = PASSTHROUGH
                .iter()
                .find(|c| **c == code)
                .copied()
                .unwrap_or("node.tool_failed");
            tool_error(
                known,
                format!(
                    "{}: {}",
                    node["name"].as_str().unwrap_or(slug),
                    message.chars().take(500).collect::<String>()
                ),
            )
        })
    }
}

// ---------------------------------------------------------------------- Connettori (RF-14)

fn token_name(id: &str) -> String {
    format!("connector:{id}:credential")
}

fn oauth_name(id: &str) -> String {
    format!("connector:{id}:oauth")
}

fn webhook_name(id: &str) -> String {
    format!("connector:{id}:webhook")
}

fn random_secret() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("generatore casuale del sistema");
    hex::encode(b)
}

impl Jarvis {
    fn register_authorized_connectors(&mut self) -> Result<(), String> {
        for (manifest, _, authorized) in self.store.connectors().map_err(|e| e.to_string())? {
            if authorized.is_some() {
                for d in crate::connectors::tool_descriptors(&manifest) {
                    self.catalog.insert_dynamic(d)?;
                }
            }
        }
        Ok(())
    }

    fn connector_manifest(&self, id: &str) -> Result<(Value, Option<String>), CoreError> {
        self.store
            .connectors()
            .map_err(internal)?
            .into_iter()
            .find(|(m, _, _)| m["id"] == id)
            .map(|(m, _, a)| (m, a))
            .ok_or_else(|| CoreError::NotFound(format!("nessun connettore {id}")))
    }

    fn connector_audit(&mut self, action: &str, id: &str, outcome: &str) -> Result<(), CoreError> {
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": action, "resource": format!("jarvis://connector/{id}"),
                          "outcome": outcome, "effect": "applied"}))?;
        Ok(())
    }

    fn connector_view(&self, manifest: &Value, installed: &str, authorized: Option<&str>) -> Value {
        let id = manifest["id"].as_str().unwrap_or_default();
        let has = |n: String| self.secrets.get(&n).ok().flatten().is_some();
        json!({
            "id": id, "name": manifest["name"], "version": manifest["version"], "description": manifest["description"],
            "base_url": manifest["base_url"], "auth": manifest["auth"]["type"],
            "installed_at": installed, "authorized_at": authorized, "authorized": authorized.is_some(),
            "has_credential": has(token_name(id)),
            "webhook": manifest["webhook"]["enabled"] == true, "webhook_path": format!("/api/hooks/{id}"),
            "operations": manifest["operations"].as_array().map(|ops| ops.iter().map(|o| json!({
                "tool": crate::connectors::tool_name(id, o["name"].as_str().unwrap_or_default()),
                "summary": o["summary"], "method": o["method"], "effects": o["effects"], "risk": o["risk"],
            })).collect::<Vec<_>>()),
        })
    }

    pub fn connectors(&self) -> Result<Vec<Value>, CoreError> {
        Ok(self
            .store
            .connectors()
            .map_err(internal)?
            .iter()
            .map(|(m, i, a)| self.connector_view(m, i, a.as_deref()))
            .collect())
    }

    /// Installa (o aggiorna) un connettore dal manifest. Gli strumenti diventano disponibili solo dopo
    /// l'autorizzazione; un connettore senza autenticazione è autorizzato dall'installazione stessa,
    /// che è un'azione esplicita del proprietario. Il segreto del webhook è mostrato una sola volta.
    pub fn install_connector(&mut self, manifest: &Value) -> Result<Value, CoreError> {
        let normalized = crate::connectors::validate_manifest(manifest, &self.cfg.web)
            .map_err(CoreError::BadRequest)?;
        let id = normalized["id"].as_str().unwrap_or_default().to_string();
        for d in crate::connectors::tool_descriptors(&normalized) {
            if validate(Contract::ToolDescriptor, &d).is_valid() {
                continue;
            }
            return Err(CoreError::BadRequest(format!(
                "operazione con nome non ammesso: {}",
                d["name"]
            )));
        }
        let at = now();
        self.catalog.remove_connector(&id);
        self.store
            .connector_put(&normalized, &at)
            .map_err(internal)?;
        let open = normalized["auth"]["type"] == "none";
        self.store
            .connector_set_authorized(&id, open.then_some(at.as_str()))
            .map_err(internal)?;
        if !open {
            self.secrets
                .delete(&token_name(&id))
                .map_err(CoreError::Internal)?;
            self.secrets
                .delete(&oauth_name(&id))
                .map_err(CoreError::Internal)?;
        }
        if open {
            for d in crate::connectors::tool_descriptors(&normalized) {
                self.catalog
                    .insert_dynamic(d)
                    .map_err(CoreError::BadRequest)?;
            }
        }
        let webhook_secret = if normalized["webhook"]["enabled"] == true {
            let s = random_secret();
            self.secrets
                .put(&webhook_name(&id), &s)
                .map_err(CoreError::Internal)?;
            Some(s)
        } else {
            self.secrets
                .delete(&webhook_name(&id))
                .map_err(CoreError::Internal)?;
            None
        };
        self.connector_audit("connector.install", &id, "succeeded")?;
        self.obs
            .log("info", "connector.install", json!({"connector": id}));
        let (m, a) = self.connector_manifest(&id)?;
        let mut view = self.connector_view(&m, &at, a.as_deref());
        if let Some(s) = webhook_secret {
            view["webhook_secret"] = s.into();
        }
        Ok(view)
    }

    /// Salva la credenziale cifrata e registra gli strumenti del connettore.
    pub fn authorize_connector(&mut self, id: &str, credential: &str) -> Result<Value, CoreError> {
        let (manifest, _) = self.connector_manifest(id)?;
        if manifest["auth"]["type"] == "oauth2" {
            return Err(CoreError::BadRequest(
                "questo connettore si autorizza con il pulsante «Accedi» (OAuth)".into(),
            ));
        }
        let credential = credential.trim();
        if manifest["auth"]["type"] != "none" {
            if credential.is_empty()
                || credential.len() > 4096
                || credential.chars().any(char::is_control)
            {
                return Err(CoreError::BadRequest(
                    "credenziale vuota o non valida".into(),
                ));
            }
            self.secrets
                .put(&token_name(id), credential)
                .map_err(CoreError::Internal)?;
        }
        let at = now();
        self.store
            .connector_set_authorized(id, Some(&at))
            .map_err(internal)?;
        self.catalog.remove_connector(id);
        for d in crate::connectors::tool_descriptors(&manifest) {
            self.catalog
                .insert_dynamic(d)
                .map_err(CoreError::BadRequest)?;
        }
        self.connector_audit("connector.authorize", id, "succeeded")?;
        self.obs
            .log("info", "connector.authorize", json!({"connector": id}));
        let (m, a) = self.connector_manifest(id)?;
        let installed = self
            .store
            .connectors()
            .map_err(internal)?
            .into_iter()
            .find(|(x, _, _)| x["id"] == id)
            .map(|(_, i, _)| i)
            .unwrap_or_default();
        Ok(self.connector_view(&m, &installed, a.as_deref()))
    }

    /// Revoca: credenziale cancellata e strumenti rimossi dal catalogo (la policy li nega da subito).
    /// Con `uninstall` elimina anche manifest e segreto del webhook.
    pub fn revoke_connector(&mut self, id: &str, uninstall: bool) -> Result<Value, CoreError> {
        self.connector_manifest(id)?;
        self.secrets
            .delete(&token_name(id))
            .map_err(CoreError::Internal)?;
        self.secrets
            .delete(&oauth_name(id))
            .map_err(CoreError::Internal)?;
        let removed = self.catalog.remove_connector(id);
        self.store
            .connector_set_authorized(id, None)
            .map_err(internal)?;
        if uninstall {
            self.secrets
                .delete(&webhook_name(id))
                .map_err(CoreError::Internal)?;
            self.store.connector_delete(id).map_err(internal)?;
        }
        self.connector_audit(
            if uninstall {
                "connector.uninstall"
            } else {
                "connector.revoke"
            },
            id,
            "revoked",
        )?;
        self.obs.log(
            "info",
            "connector.revoke",
            json!({"connector": id, "uninstall": uninstall}),
        );
        Ok(json!({"id": id, "tools_removed": removed, "uninstalled": uninstall}))
    }

    /// Avvia OAuth (ADR-0027): restituisce la pagina di consenso da aprire nel browser del proprietario.
    pub fn oauth_start(
        &mut self,
        id: &str,
        client_secret: Option<String>,
    ) -> Result<Value, CoreError> {
        let (manifest, _) = self.connector_manifest(id)?;
        if manifest["auth"]["type"] != "oauth2" {
            return Err(CoreError::BadRequest("il connettore non usa OAuth".into()));
        }
        let secret = client_secret
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control));
        let url = self
            .oauth
            .start(&manifest, self.cfg.port, secret)
            .map_err(CoreError::BadRequest)?;
        self.connector_audit("connector.oauth.start", id, "succeeded")?;
        Ok(json!({"authorize_url": url}))
    }

    /// Ritorno dal servizio: `state` monouso, scambio del codice, token cifrati, strumenti registrati.
    pub fn oauth_callback(
        &mut self,
        state: &str,
        code: Option<&str>,
        denied: Option<&str>,
    ) -> Result<String, CoreError> {
        let pending = self.oauth.take(state).ok_or_else(|| {
            CoreError::BadRequest("richiesta scaduta o già usata: riprova da JARVIS".into())
        })?;
        let id = pending.connector.clone();
        if let Some(reason) = denied {
            self.connector_audit("connector.oauth.denied", &id, "cancelled")?;
            return Err(CoreError::BadRequest(format!(
                "il servizio non ha concesso l'accesso ({})",
                reason.chars().take(60).collect::<String>()
            )));
        }
        let code =
            code.ok_or_else(|| CoreError::BadRequest("codice di autorizzazione mancante".into()))?;
        let (manifest, _) = self.connector_manifest(&id)?;
        let tokens = match crate::oauth::exchange(&manifest, &self.cfg.web, code, &pending) {
            Ok(t) => t,
            Err(e) => {
                self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                                  "node": "core-local", "action": "connector.oauth.exchange", "resource": format!("jarvis://connector/{id}"),
                                  "outcome": "failed", "error_code": "connector.oauth_failed", "effect": "not_applied"}))?;
                return Err(CoreError::BadRequest(e));
            }
        };
        self.save_oauth_tokens(&id, &tokens)?;
        self.store
            .connector_set_authorized(&id, Some(&now()))
            .map_err(internal)?;
        self.catalog.remove_connector(&id);
        for d in crate::connectors::tool_descriptors(&manifest) {
            self.catalog
                .insert_dynamic(d)
                .map_err(CoreError::BadRequest)?;
        }
        self.connector_audit("connector.authorize", &id, "succeeded")?;
        self.obs.log(
            "info",
            "connector.authorize",
            json!({"connector": id, "method": "oauth2"}),
        );
        Ok(manifest["name"].as_str().unwrap_or(&id).to_string())
    }

    fn save_oauth_tokens(&self, id: &str, tokens: &Value) -> Result<(), CoreError> {
        self.secrets
            .put(
                &token_name(id),
                tokens["access_token"].as_str().unwrap_or_default(),
            )
            .map_err(CoreError::Internal)?;
        self.secrets
            .put(&oauth_name(id), &tokens.to_string())
            .map_err(CoreError::Internal)
    }

    /// Rinnovo automatico dell'access token a 60 secondi dalla scadenza.
    fn oauth_refresh_if_needed(&self, id: &str, manifest: &Value) -> Result<(), String> {
        let state: Value = self
            .secrets
            .get(&oauth_name(id))?
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or("connettore non autorizzato: premi «Accedi»")?;
        if !crate::oauth::expiring(&state) {
            return Ok(());
        }
        let renewed = crate::oauth::refresh(manifest, &self.cfg.web, &state);
        self.obs.count(if renewed.is_ok() {
            "connector.oauth.refreshed"
        } else {
            "connector.oauth.refresh_failed"
        });
        let tokens = renewed?;
        self.save_oauth_tokens(id, &tokens)
            .map_err(|e| format!("{e:?}"))
    }

    fn call_connector(
        &mut self,
        tool: &str,
        arguments: &Value,
        confirmed: bool,
    ) -> Result<Value, tools::ToolError> {
        let mut parts = tool.splitn(3, '.');
        let (_, id, op) = (
            parts.next(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        let descriptor = self
            .catalog
            .get(tool)
            .ok_or_else(|| tool_error("connector.not_authorized", "connettore non autorizzato"))?;
        if descriptor.needs_confirmation() && !confirmed {
            return Err(tool_error(
                "connector.not_confirmed",
                "la scrittura verso l'esterno richiede la conferma del proprietario",
            ));
        }
        let (manifest, authorized) = self
            .connector_manifest(id)
            .map_err(|_| tool_error("connector.not_authorized", "connettore non installato"))?;
        if authorized.is_none() {
            return Err(tool_error(
                "connector.not_authorized",
                "connettore revocato",
            ));
        }
        if manifest["auth"]["type"] == "oauth2" {
            self.oauth_refresh_if_needed(id, &manifest)
                .map_err(|e| tool_error("connector.not_authorized", e))?;
        }
        let secret = self
            .secrets
            .get(&token_name(id))
            .map_err(|e| tool_error("connector.unavailable", e))?;
        // Le letture seguono la politica di ripetizione del descrittore (`safe`, 2 tentativi) sugli errori
        // transitori; le scritture non vengono mai ripetute.
        let attempts = if descriptor.descriptor["retry"]["mode"] == "safe" {
            descriptor.descriptor["retry"]["max_attempts"]
                .as_u64()
                .unwrap_or(1)
                .clamp(1, 3)
        } else {
            1
        };
        let mut attempt = 0;
        loop {
            attempt += 1;
            let timer = Timer::start();
            let result =
                crate::connectors::call(&manifest, op, arguments, secret.as_deref(), &self.cfg.web);
            self.obs
                .observe(&format!("connector {id}"), timer.ms(), result.is_err());
            match result {
                Err(e) if attempt < attempts && crate::connectors::transient(e.code) => {
                    self.obs.count("connector.retry");
                    self.obs.log(
                        "warn",
                        "connector.retry",
                        json!({"connector": id, "code": e.code, "attempt": attempt}),
                    );
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                other => return other,
            }
        }
    }

    /// Effetto di uno strumento fallito: una scrittura esterna interrotta da timeout o rete può essere
    /// arrivata al servizio, quindi l'esito è ignoto e va verificato prima di ripeterla.
    fn failure_effect(&self, tool: &str, code: &str) -> &'static str {
        let write_external = self
            .catalog
            .get(tool)
            .is_some_and(|t| t.descriptor["effects"] == "write_external");
        let node_write = tool.starts_with("node.")
            && self.catalog.get(tool).is_some_and(|t| {
                t.descriptor["effects"] != "read" && t.descriptor["effects"] != "none"
            });
        if (write_external && matches!(code, "web.timeout" | "web.unreachable"))
            || (node_write && code == "node.timeout")
        {
            "unknown"
        } else {
            "not_applied"
        }
    }

    fn notify_connector_failure(
        &mut self,
        task: &Task,
        tool: &str,
        e: &tools::ToolError,
        effect: &str,
    ) -> Result<(), CoreError> {
        let Some(id) = tool
            .strip_prefix("connector.")
            .and_then(|r| r.split('.').next())
        else {
            return Ok(());
        };
        let body = if effect == "unknown" {
            format!("{tool}: {} — la scrittura potrebbe essere arrivata al servizio: verifica prima di ripeterla.", e.message)
        } else {
            format!("{tool}: {}", e.message)
        };
        self.notify(json!({
            "severity": "warning", "category": "integration", "title": format!("Connettore {id} non riuscito"),
            "body": body, "source": {"kind": "module", "id": "integration"},
            "correlation_id": task.correlation(), "task_id": task.id(),
            "dedupe_key": format!("connector-failure:{id}"), "link": {"view": "connectors", "ref": id},
        }))?;
        Ok(())
    }

    /// Webhook in ingresso: firma HMAC-SHA256 obbligatoria. Il contenuto è trattato come non attendibile e
    /// diventa solo una notifica; non avvia strumenti né automazioni.
    pub fn connector_webhook(
        &mut self,
        id: &str,
        body: &[u8],
        signature: Option<&str>,
    ) -> Result<Value, CoreError> {
        let secret = self
            .secrets
            .get(&webhook_name(id))
            .map_err(CoreError::Internal)?
            .filter(|_| {
                self.store
                    .connectors()
                    .map(|c| c.iter().any(|(m, _, _)| m["id"] == id))
                    .unwrap_or(false)
            });
        let Some(secret) = secret else {
            return Err(CoreError::NotFound(format!(
                "nessun webhook attivo per {id}"
            )));
        };
        if !crate::connectors::verify_signature(&secret, body, signature) {
            self.obs.count("connector.webhook_rejected");
            self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "plugin", "id": format!("connector.{id}")},
                              "node": "core-local", "action": "connector.webhook", "resource": format!("jarvis://connector/{id}"),
                              "outcome": "failed", "error_code": "connector.bad_signature", "effect": "not_applied"}))?;
            return Err(CoreError::Unauthorized(
                "firma del webhook non valida".into(),
            ));
        }
        let (manifest, _) = self.connector_manifest(id)?;
        let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let text = parsed["text"]
            .as_str()
            .or(parsed["message"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(500).collect());
        let note = self.notify(json!({
            "severity": "info", "category": "integration",
            "title": format!("Evento da {}", manifest["name"].as_str().unwrap_or(id)),
            "body": text.chars().take(500).collect::<String>(),
            "source": {"kind": "plugin", "id": format!("connector.{id}")}, "link": {"view": "connectors", "ref": id},
        }))?;
        self.obs.count("connector.webhook_accepted");
        Ok(json!({"accepted": true, "notification_id": note.map(|n| n["notification_id"].clone())}))
    }
}

// ---------------------------------------------------------------------- Backup (RNF-08)

impl Jarvis {
    /// Backup cifrato con la passphrase del proprietario; istantanea coerente presa sotto il blocco del Core.
    pub fn create_backup(&mut self, passphrase: &str) -> Result<Value, CoreError> {
        let timer = Timer::start();
        let out_dir = self.cfg.backups_dir();
        let store = &self.store;
        let summary = crate::backup::create(&self.cfg.data_dir, &out_dir, passphrase, |p| {
            store.snapshot_to(p).map_err(|e| e.to_string())
        })
        .map_err(|e| {
            if e.contains("passphrase") {
                CoreError::BadRequest(e)
            } else {
                CoreError::Internal(e)
            }
        })?;
        self.obs.observe("backup create", timer.ms(), false);
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "backup.create", "outcome": "succeeded", "effect": "applied"}))?;
        self.notify(json!({"severity": "success", "category": "system", "title": "Backup creato",
            "body": format!("{} · {} file cifrati", summary["file"].as_str().unwrap_or(""), summary["files"]), "source": {"kind": "module", "id": "backup"}}))?;
        let mut view = summary;
        if let Some(m) = view.as_object_mut() {
            m.remove("path");
        }
        Ok(view)
    }

    pub fn backups(&self) -> Value {
        json!({"dir": self.cfg.backups_dir().to_string_lossy(), "items": crate::backup::list(&self.cfg.backups_dir())})
    }
}

fn merge(base: &Value, extra: Value) -> Value {
    let mut m = obj(base.clone());
    m.extend(obj(extra));
    Value::Object(m)
}

/// Budget della cronologia: `JARVIS_CONTEXT_CHARS` o il valore predefinito, mai sotto 1000 caratteri.
fn context_budget() -> usize {
    std::env::var("JARVIS_CONTEXT_CHARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CONTEXT_CHARS)
        .max(1000)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Cronologia entro il budget (ADR-0024): l'ultimo messaggio resta intero (fino a MAX_MESSAGE), i turni
/// precedenti sono accorciati e i più vecchi esclusi quando il budget finisce. L'ordine è conservato.
pub(crate) fn budget_history(recent: &[Value], budget: usize) -> Vec<Value> {
    let mut kept: Vec<Value> = vec![];
    let mut used = 0usize;
    for (i, m) in recent.iter().rev().enumerate() {
        let content = m["content"].as_str().unwrap_or_default();
        let text = if i == 0 {
            clip(content, MAX_MESSAGE)
        } else {
            clip(content, OLD_TURN_CHARS)
        };
        let len = text.chars().count();
        if i > 0 && used + len > budget {
            break;
        }
        used += len;
        kept.push(json!({"role": m["role"], "content": text}));
    }
    kept.reverse();
    kept
}

/// Memoria entro il budget: gli elementi arrivano già in ordine di priorità (preferenze, poi pertinenti).
pub(crate) fn budget_memory(items: Vec<Value>, budget: usize) -> Vec<Value> {
    let mut used = 0usize;
    items
        .into_iter()
        .take_while(|m| {
            used += m.as_str().map(|s| s.chars().count()).unwrap_or(0);
            used <= budget
        })
        .collect()
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn history_keeps_latest_message_whole_and_respects_budget() {
        let long = "x".repeat(5000);
        let recent: Vec<Value> = (0..10)
            .map(|i| json!({"role": if i % 2 == 0 { "user" } else { "assistant" }, "content": format!("{i}:{long}")}))
            .collect();
        let out = budget_history(&recent, 2500);
        let last = out.last().unwrap()["content"].as_str().unwrap();
        assert!(
            last.starts_with("9:") && last.chars().count() == 5002,
            "ultimo turno intero"
        );
        let older: usize = out[..out.len() - 1]
            .iter()
            .map(|m| m["content"].as_str().unwrap().chars().count())
            .sum();
        assert!(
            older + 5002 <= 2500 + 5002 && out.len() < 10,
            "turni vecchi esclusi oltre il budget"
        );
        assert!(out[..out.len() - 1].iter().all(|m| m["content"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= OLD_TURN_CHARS));
        let short: Vec<Value> = vec![json!({"role": "user", "content": "ciao"})];
        assert_eq!(budget_history(&short, 2500), short);
    }

    #[test]
    fn memory_stops_at_budget_in_priority_order() {
        let items: Vec<Value> = vec![
            "a".repeat(1500).into(),
            "b".repeat(1000).into(),
            "c".repeat(1000).into(),
        ];
        let out = budget_memory(items, 3000);
        assert_eq!(out.len(), 2);
    }
}
