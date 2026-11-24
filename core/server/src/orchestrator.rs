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

mod initiatives;
mod mission_runner;
pub use mission_runner::{MissionRun, Next};

/// Memoria nel contesto dell'agente (ADR-0022).
const MEMORY_PROCEDURAL: usize = 20;
const MEMORY_SMALL: usize = 30;
const MEMORY_RELEVANT: usize = 8;
const HISTORY_MESSAGES: u32 = 10;
/// Budget del contesto in caratteri (circa 3,5 caratteri per token): cronologia e memoria (ADR-0024).
const CONTEXT_CHARS: usize = 15_000;
const MEMORY_CHARS: usize = 3000;
/// Lunghezza massima dei turni più vecchi: la richiesta corrente resta intera.
const OLD_TURN_CHARS: usize = 1200;
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
    /// Passi già eseguiti nel turno sospeso (ADR-0037).
    history: Vec<Value>,
}

/// Passi massimi del ciclo dell'agente per una richiesta (limite `max_steps` della delega).
const MAX_STEPS: usize = 4;
/// Agenti temporanei per richiesta e passi di ciascuno (ADR-0040).
const MAX_SUBAGENTS: usize = 2;
const SUBAGENT_STEPS: usize = 3;
/// Strumenti per ciascuna esecuzione di una missione (ADR-0096).
const MISSION_STEPS: usize = 5;

/// Agente isolato in esecuzione (sotto-agente o missione): ruolo, strumenti concessi e passi consentiti.
struct TempAgent {
    role: String,
    tools: Vec<String>,
    steps: usize,
    /// Identità nell'audit e nella policy: `temporary` o `mission`.
    actor: &'static str,
}
/// Strumenti predefiniti di un agente temporaneo: sola lettura.
const SUBAGENT_DEFAULT_TOOLS: [&str; 7] = [
    "time.now",
    "system.info",
    "fs.list",
    "fs.read",
    "memory.search",
    "web.search",
    "web.fetch",
];
/// Caratteri conservati degli esiti precedenti all'ultimo nel contesto dell'agente.
const EARLIER_OUTPUT_CHARS: usize = 1500;
const LATEST_OUTPUT_CHARS: usize = 6000;
const CLOUD_CONVERSATION_TOKENS: usize = 16_000;

/// Identificativi W3C Trace Context (ADR-0039): traccia = correlation_id del task (32 esadecimali), intervallo
/// radice = prime 16 cifre dell'id del task, intervalli figli casuali.
fn trace_ids(task: &Task) -> (String, String) {
    let hex = |s: String| s.replace('-', "");
    let root: String = hex(task.id()).chars().take(16).collect();
    (hex(task.correlation()), root)
}

/// Registro di un passo: strumento, argomenti, impronta (per riconoscere le ripetizioni) ed esito.
fn step_record(tool: &str, arguments: &Value, digest: &str, status: &str, output: Value) -> Value {
    json!({"tool": tool, "arguments": arguments, "digest": digest, "status": status, "output": output})
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
    updates: crate::updates::Updates,
    oauth: crate::oauth::Flows,
    cloud_limiter: std::sync::Arc<std::sync::Mutex<crate::cloud::Limiter>>,
    n8n: std::sync::Arc<crate::n8n::Manager>,
    cfg: Config,
    store: Store,
    audit: AuditLog,
    catalog: Catalog,
    worker: Option<Worker>,
    pending: HashMap<String, Pending>,
    /// Porta TLS dei nodi e impronta del certificato del Core, se l'mTLS è attivo (ADR-0041).
    node_tls: Option<(u16, String)>,
    /// Agenti temporanei in esecuzione (ADR-0040): task figlio → ruolo e strumenti concessi.
    temporary: HashMap<String, TempAgent>,
    /// Canale delle notifiche appena create, per i client collegati in tempo reale (ADR-0098).
    events: tokio::sync::broadcast::Sender<Value>,
    /// Ultima valutazione delle iniziative proattive (ADR-0097).
    proactive_checked: Option<std::time::Instant>,
    obs: std::sync::Arc<Observability>,
    secrets: crate::secrets::SecretStore,
    push: crate::push::PushSubscriptions,
    /// Ultima istantanea delle metriche salvata (RNF-07).
    metrics_saved: Option<std::time::Instant>,
    /// Ultima manutenzione del database (ADR-0037): al massimo ogni 6 ore.
    maintained: Option<std::time::Instant>,
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
    _legacy_lock: std::fs::File,
}

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

impl Jarvis {
    pub fn new(cfg: Config) -> Result<Self, String> {
        let wait = std::time::Duration::from_secs(3);
        let lock = crate::config::data_lock(&cfg.data_dir, wait)?;
        match crate::backup::private_dir(&cfg.data_dir) {
            Ok(()) => (),
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists && cfg.data_dir.is_dir() => {}
            Err(error) => return Err(format!("cartella dati: {error}")),
        }
        crate::backup::tighten_dir(&cfg.data_dir);
        std::fs::create_dir_all(cfg.workspace()).map_err(|e| format!("workspace: {e}"))?;
        // Compatibilità con le release che usano ancora il blocco interno.
        let legacy_lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(cfg.data_dir.join(".lock"))
            .map_err(|e| format!("blocco della cartella dati: {e}"))?;
        crate::config::wait_lock(&legacy_lock, &cfg.data_dir, wait)?;
        // Rollback di release: copia dei dati prima che una versione diversa apra (e migri) il database.
        let upgraded = crate::rollback::prepare(&cfg.data_dir)?;
        std::fs::create_dir_all(cfg.data_dir.join("audit")).map_err(|e| format!("audit: {e}"))?;
        let mut store = Store::open(&cfg.data_dir.join("jarvis.sqlite3"))
            .map_err(|e| format!("database: {e}"))?;
        let audit = AuditLog::open(
            &cfg.data_dir.join("audit/core-security.jsonl"),
            "core-security",
        )
        .map_err(|e| format!("audit: {e}"))?;
        let catalog = Catalog::load()?;
        let secrets = crate::secrets::SecretStore::open(&cfg.data_dir)?;
        store
            .protect_chat(secrets.chat_cipher.clone())
            .map_err(|e| format!("cifratura della conversazione: {e}"))?;
        let push = crate::push::PushSubscriptions::open(&cfg.data_dir.join("jarvis.sqlite3"))?;
        let cfg_logs = cfg.data_dir.join("logs");
        let (cfg_setup_dir, cfg_setup_python, cfg_setup_ai) =
            (cfg.data_dir.clone(), cfg.python.clone(), cfg.ai_dir.clone());
        let mut jarvis = Self {
            stream_sink: None,
            updates: crate::updates::Updates::new(&cfg.data_dir),
            oauth: crate::oauth::Flows::default(),
            cloud_limiter: Default::default(),
            n8n: std::sync::Arc::new(crate::n8n::Manager::new(cfg.data_dir.clone())),
            cfg,
            store,
            audit,
            catalog,
            worker: None,
            pending: HashMap::new(),
            temporary: HashMap::new(),
            events: tokio::sync::broadcast::channel(64).0,
            proactive_checked: None,
            node_tls: None,
            obs: std::sync::Arc::new(Observability::new(Some(&cfg_logs))),
            secrets,
            push,
            metrics_saved: None,
            maintained: None,
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
            _legacy_lock: legacy_lock,
        };
        jarvis.register_authorized_connectors()?;
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
        // Dopo la configurazione della voce il worker riparte con i nuovi percorsi.
        if self.setup.take_restart() {
            self.worker = None;
            self.obs
                .log("info", "worker.restart", json!({"reason": "setup"}));
        }
        if self.worker.is_none() {
            let mut cmd = Command::new(&self.cfg.python);
            crate::hide_console(&mut cmd);
            cmd.args(["-m", "jarvis_ai.worker", "--handler", "assistant"])
                .current_dir(&self.cfg.ai_dir);
            for (k, v) in self.setup.worker_env() {
                cmd.env(k, v);
            }
            for (k, v) in &self.cfg.worker_env {
                cmd.env(k, v);
            }
            // L'unico motore vocale del Core ammesso è quello sintetico delle verifiche.
            cmd.env(
                "JARVIS_VOICE_ENGINE",
                if self.cfg.voice_engine.as_deref() == Some("fake") {
                    "fake"
                } else {
                    "off"
                },
            );
            cmd.env_remove("JARVIS_TOKEN");
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
            "core": {"version": env!("CARGO_PKG_VERSION"), "status": "up", "secrets_store": self.secrets.location(), "memory_embeddings": null,
                     "database": {"schema_version": self.store.schema_version().ok()}},
            "web": self.cfg.web.describe(),
            "ai": ai,
            "inference": {"location": "cloud", "history_default": false, "voice_available": false},
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
        let objective = task.doc["objective"].as_str().unwrap_or_default();
        let tools = if inputs["response_only"] == true {
            Vec::new()
        } else {
            self.delegated_tools(objective, self.task_scope(task))
        };
        json!({
            "contract": "jarvis.delegation/1",
            "delegation_id": uuid::Uuid::new_v4().to_string(),
            "task_id": task.id(),
            "objective": task.doc["objective"],
            "delegate": match (self.temporary.get(&task.id()), crate::agents::route(task.doc["objective"].as_str().unwrap_or_default())) {
                (Some(agent), _) => json!({"agent": "temporary", "temporary": true, "role": agent.role}),
                (None, Some(a)) => json!({"agent": a.name, "temporary": false, "role": a.role}),
                (None, None) => json!({"agent": crate::agents::GENERAL, "temporary": false}),
            },
            "context": {"refs": [format!("jarvis://task/{}", task.id())]},
            "tools": tools.iter().map(|t| json!({"name": t.name(), "version": t.version()})).collect::<Vec<_>>(),
            "limits": if let Some(agent) = self.temporary.get(&task.id()) {
                json!({"max_steps": agent.steps, "max_subdelegations": 0, "allow_cloud": false})
            } else {
                json!({"max_steps": MAX_STEPS, "max_subdelegations": MAX_SUBAGENTS, "allow_cloud": false})
            },
            "timeout_ms": self.cfg.delegation_timeout_ms,
            "completion_conditions": ["Esito jarvis.agent-result/1 valido"],
            "issued_at": ts(issued),
            "expires_at": ts(expires),
            "inputs": Self::with_dynamic_tools(inputs, &tools),
            "stream": self.stream_sink.is_some(),
        })
    }

    /// Gli strumenti dinamici citati nella richiesta hanno precedenza dentro il budget di delega.
    fn delegated_tools(
        &self,
        objective: &str,
        in_scope: impl Fn(&str) -> bool,
    ) -> Vec<&crate::catalog::Tool> {
        let dynamic = |t: &&crate::catalog::Tool| {
            t.name().starts_with("connector.") || t.name().starts_with("node.")
        };
        let objective = objective.to_ascii_lowercase();
        let explicit = explicit_connector_tool(&objective);
        let words: std::collections::HashSet<_> = objective
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .collect();
        let mut ranked: Vec<_> = self
            .catalog
            .iter()
            .filter(|t| in_scope(t.name()))
            .filter(dynamic)
            .collect();
        ranked.sort_by_cached_key(|t| {
            let mut parts = t.name().splitn(3, '.').skip(1);
            let namespace = parts.next().unwrap_or_default();
            let operation = parts.next().unwrap_or_default();
            let named = words.contains(namespace);
            let specific = named && operation.split('.').all(|part| words.contains(part));
            std::cmp::Reverse((explicit.as_deref() == Some(t.name()), specific, named))
        });
        self.catalog
            .iter()
            .filter(|t| {
                in_scope(t.name())
                    && t.name() != crate::desktop::TOOL
                    && !dynamic(t)
                    && (t.name() != "n8n.create" || self.n8n.enabled())
            })
            .chain(ranked.into_iter().take(16))
            .take(64)
            .collect()
    }

    /// Registra un intervallo nella traccia del task (ADR-0039): nome, tipo, inizio, durata, esito e attributi
    /// senza contenuti dell'utente. Un errore di scrittura non interrompe la richiesta.
    fn span(&self, task: &Task, name: &str, kind: &str, ms: f64, status: &str, attrs: Value) {
        let (trace_id, root) = trace_ids(task);
        // Un agente temporaneo (ADR-0040) condivide la traccia del task principale: la sua radice è figlia di
        // quella del principale e i suoi intervalli sono archiviati con il task principale.
        let parent_id = task.doc["parent_task_id"].as_str().map(str::to_string);
        let parent_root = parent_id
            .as_ref()
            .map(|p| p.replace('-', "").chars().take(16).collect::<String>());
        let end = chrono::Utc::now();
        let start = end - chrono::Duration::microseconds((ms * 1000.0) as i64);
        let (span_id, parent) = if kind == "task" {
            (root, parent_root.map_or(Value::Null, Value::String))
        } else {
            (
                uuid::Uuid::new_v4().simple().to_string()[..16].to_string(),
                Value::String(root),
            )
        };
        let doc = json!({
            "trace_id": trace_id, "span_id": span_id, "parent_span_id": parent, "name": name, "kind": kind,
            "start": start.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "duration_ms": (ms * 10.0).round() / 10.0, "status": status, "attributes": attrs,
        });
        let key = parent_id.unwrap_or_else(|| task.id());
        let _ = self.store.span_add(&key, &now(), &doc);
    }

    /// Traccia di un task: intervalli in ordine e durata complessiva.
    pub fn task_trace(&self, task_id: &str) -> Result<Value, CoreError> {
        let spans = self.store.spans(task_id).map_err(internal)?;
        if spans.is_empty() {
            return Err(CoreError::NotFound(
                "nessuna traccia per questo task".into(),
            ));
        }
        let total = spans
            .iter()
            .find(|s| s["kind"] == "task" && s["parent_span_id"].is_null())
            .and_then(|s| s["duration_ms"].as_f64());
        Ok(
            json!({"task_id": task_id, "trace_id": spans[0]["trace_id"], "total_ms": total, "spans": spans}),
        )
    }

    /// Strumento concesso all'agente del task: la specializzazione scelta dalla richiesta (ADR-0038) limita
    /// gli strumenti al proprio ambito; il generalista li riceve tutti.
    fn task_allows(&self, task: &Task, tool: &str) -> bool {
        self.task_scope(task)(tool)
    }

    /// Ambito degli strumenti del task, calcolato una volta (le competenze attive richiedono una query).
    fn task_scope(&self, task: &Task) -> impl Fn(&str) -> bool + '_ {
        let temporary = self
            .temporary
            .get(&task.id())
            .map(|agent| agent.tools.clone());
        let objective = task.doc["objective"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let specialist = crate::agents::route(&objective);
        let explicit = explicit_connector_tool(&objective.to_ascii_lowercase());
        // Una competenza attiva rende disponibili i suoi strumenti anche fuori dall'ambito dello specialista
        // (ADR-0048); policy, conferme e audit restano quelli di ogni strumento.
        let skill_tools: Vec<String> = if temporary.is_some() || specialist.is_none() {
            Vec::new()
        } else {
            match crate::skills::active(&self.store, &objective) {
                Ok(skills) => skills
                    .iter()
                    .flat_map(|s| s["tools"].as_array().cloned().unwrap_or_default())
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                Err(error) => {
                    eprintln!(
                        "[competenze] impossibile caricare l'ambito degli strumenti: {error}"
                    );
                    Vec::new()
                }
            }
        };
        let explicit_desktop = crate::desktop::Target::from_request(&objective).is_some();
        move |tool: &str| match &temporary {
            _ if tool == crate::desktop::TOOL => temporary.is_none() && explicit_desktop,
            Some(tools) => tools.iter().any(|t| t == tool),
            None => {
                explicit.as_deref() == Some(tool)
                    || specialist.is_none_or(|a| crate::agents::allows(a, tool))
                    || skill_tools.iter().any(|t| t == tool)
            }
        }
    }

    /// Include i descrittori dinamici della stessa lista inviata nel campo `tools`.
    fn with_dynamic_tools(inputs: &Value, tools: &[&crate::catalog::Tool]) -> Value {
        let dynamic: Vec<Value> = tools
            .iter()
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
        let agent = crate::agents::route(task.doc["objective"].as_str().unwrap_or_default())
            .map_or(crate::agents::GENERAL, |a| a.name);
        let attrs = match &result {
            Ok(r) => json!({"agent": agent, "backend": r["backend"]["name"], "result": r["kind"],
                            "prompt_tokens": r["usage"]["prompt_tokens"], "output_tokens": r["usage"]["output_tokens"],
                            "tools_sent": r["usage"]["tools_sent"], "steps_left": inputs["steps_left"]}),
            Err(e) => json!({"agent": agent, "error_code": e["code"]}),
        };
        self.span(
            task,
            agent,
            "agent",
            timer.ms(),
            if result.is_ok() { "ok" } else { "error" },
            attrs,
        );
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
        // Competenze attivate dalla richiesta (ADR-0048).
        let skills: Vec<Value> = crate::skills::active(&self.store, &query)
            .map_err(internal)?
            .into_iter()
            .map(|s| json!({"id": s["id"], "name": s["name"], "instructions": s["instructions"], "tools": s["tools"]}))
            .collect();
        if skills.is_empty() {
            return Ok(json!({"messages": messages, "memory": memory}));
        }
        Ok(json!({"messages": messages, "memory": memory, "skills": skills}))
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

    pub fn record_owner_action(&mut self, action: &str) -> Result<(), CoreError> {
        self.audit(json!({
            "correlation_id": uuid::Uuid::new_v4().to_string(),
            "actor": {"kind": "owner", "id": "owner"}, "node": "core-local",
            "action": action, "outcome": "succeeded", "effect": "applied",
        }))?;
        Ok(())
    }

    pub fn cloud_status(&self) -> Result<Value, CoreError> {
        let limiter = self.cloud_limiter.lock().unwrap_or_else(|e| e.into_inner());
        let settings = crate::cloud::settings(&self.cfg.data_dir);
        let providers = crate::cloud::Provider::ALL
            .into_iter()
            .map(|provider| {
                let configured = self
                    .secrets
                    .get(&provider.secret())
                    .map_err(internal)?
                    .is_some();
                let mut view = provider.view(configured, &limiter);
                view["model_override"] = settings[provider.id()].clone();
                view["preferred"] =
                    json!(configured && settings["default_provider"] == provider.id());
                view["feedback_profiles"] = json!(self
                    .store
                    .cloud_feedback_profiles(provider.id())
                    .map_err(internal)?);
                Ok(view)
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        Ok(json!({"providers": providers}))
    }

    pub fn configure_cloud(&mut self, body: &Value) -> Result<Value, CoreError> {
        let provider = crate::cloud::Provider::parse(&body["provider"])
            .ok_or_else(|| CoreError::BadRequest("provider sconosciuto".into()))?;
        if body["remove_key"] == true {
            self.secrets.delete(&provider.secret()).map_err(internal)?;
        } else {
            let model = body
                .get("model")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|model| crate::cloud::valid_model(model))
                        .ok_or_else(|| {
                            CoreError::BadRequest("identificativo del modello non valido".into())
                        })
                })
                .transpose()?;
            if let Some(value) = body.get("key") {
                let key = value
                    .as_str()
                    .map(str::trim)
                    .ok_or_else(|| CoreError::BadRequest("chiave API non valida".into()))?;
                if key.is_empty() || key.len() > 1024 || key.bytes().any(|b| !b.is_ascii_graphic())
                {
                    return Err(CoreError::BadRequest("chiave API non valida".into()));
                }
                self.secrets
                    .put(&provider.secret(), key)
                    .map_err(internal)?;
            }
            if body["use_for_chat"] == true
                && self
                    .secrets
                    .get(&provider.secret())
                    .map_err(internal)?
                    .is_none()
            {
                return Err(CoreError::BadRequest(
                    "Salva prima la chiave API del provider.".into(),
                ));
            }
            if model.is_some() || body["use_for_chat"] == true || body["clear_model"] == true {
                let mut settings = crate::cloud::settings(&self.cfg.data_dir);
                if let Some(model) = model {
                    settings[provider.id()] = json!(model);
                } else if body["clear_model"] == true {
                    settings
                        .as_object_mut()
                        .expect("configurazione JSON")
                        .remove(provider.id());
                }
                if body["use_for_chat"] == true {
                    settings["default_provider"] = json!(provider.id());
                }
                crate::backup::atomic_write(
                    &self.cfg.data_dir.join("cloud.json"),
                    settings.to_string().as_bytes(),
                )
                .map_err(internal)?;
            }
        }
        if body["remove_key"] == true
            || body.get("key").is_some()
            || body.get("model").is_some()
            || body["clear_model"] == true
        {
            self.cloud_limiter
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reset_model_availability(provider);
        }
        let action = if body["remove_key"] == true {
            "cloud.remove_key"
        } else {
            "cloud.configure"
        };
        self.record_owner_action(action)?;
        self.cloud_status()
    }

    pub fn rate_cloud_reply(&self, input: &Value) -> Result<Value, CoreError> {
        let task = input["task_id"]
            .as_str()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .ok_or_else(|| CoreError::BadRequest("ID della risposta non valido.".into()))?;
        let rating = input["rating"]
            .as_str()
            .filter(|r| matches!(*r, "helpful" | "concise" | "detailed" | "precise"))
            .ok_or_else(|| CoreError::BadRequest("Valutazione non valida.".into()))?;
        if !self
            .store
            .rate_cloud_reply(task, rating)
            .map_err(internal)?
        {
            return Err(CoreError::NotFound(
                "Risposta cloud assente o troppo vecchia.".into(),
            ));
        }
        Ok(json!({"saved": true}))
    }

    pub fn prepare_cloud_chat(&self, body: &Value) -> Result<crate::cloud::Request, CoreError> {
        use crate::cloud::{Function, Provider, Request};

        let providers = if body["provider"] == "auto" {
            Provider::ALL.to_vec()
        } else {
            vec![Provider::parse(&body["provider"])
                .ok_or_else(|| CoreError::BadRequest("provider sconosciuto".into()))?]
        };
        let message = body["message"]
            .as_str()
            .map(str::trim)
            .filter(|message| !message.is_empty() && message.chars().count() <= MAX_MESSAGE)
            .ok_or_else(|| CoreError::BadRequest("messaggio mancante o troppo lungo".into()))?;
        if crate::cloud::contains_credentials(message) {
            return Err(CoreError::BadRequest(
                "La richiesta contiene una credenziale. Rimuovila prima dell’invio al provider."
                    .into(),
            ));
        }
        let recent = if body["use_context"] == true {
            self.store.recent_cloud_turns().map_err(internal)?
        } else {
            Vec::new()
        };
        let settings = crate::cloud::settings(&self.cfg.data_dir);
        let mut targets = Vec::new();
        for provider in providers {
            if let Some(key) = self.secrets.get(&provider.secret()).map_err(internal)? {
                let mut budget = CLOUD_CONVERSATION_TOKENS
                    .saturating_sub(crate::cloud::estimated_tokens(message));
                let mut history = Vec::new();
                for turn in recent.iter().take_while(|turn| {
                    turn.cloud_provider.as_deref() == Some(provider.id())
                        || turn.assistant.is_none()
                }) {
                    let Some(assistant) = turn.assistant.as_deref() else {
                        continue;
                    };
                    if crate::cloud::contains_credentials(&turn.user)
                        || crate::cloud::contains_credentials(assistant)
                    {
                        break;
                    }
                    let user = crate::cloud::clip_tokens(&turn.user, 2200);
                    let assistant = crate::cloud::clip_tokens(assistant, 2800);
                    let cost = crate::cloud::estimated_tokens(&user)
                        + crate::cloud::estimated_tokens(&assistant);
                    if cost > budget {
                        break;
                    }
                    budget -= cost;
                    history.push(crate::cloud::Turn { user, assistant });
                }
                history.reverse();
                targets.push(provider.target(key, message, history, &settings));
            }
        }
        // Il messaggio resta con un solo provider, anche in caso di errore.
        targets.sort_by_key(|target| {
            (
                settings["default_provider"] != target.provider.id(),
                target.history.is_empty(),
            )
        });
        let target = targets.into_iter().next().ok_or_else(|| {
            CoreError::BadRequest("Configura la chiave API in Modelli AI.".into())
        })?;
        let task = Task::new(message, json!({"kind": "owner", "id": "owner"}));
        let allows = self.task_scope(&task);
        let defensive = crate::agents::route(message).filter(|agent| agent.name == "monitoring");
        let tools = crate::cloud::TOOLS
            .iter()
            .filter(|_| crate::cloud::may_need_calculation(message))
            .filter(|tool| allows(tool))
            .filter(|tool| defensive.is_none_or(|agent| crate::agents::allows(agent, tool)))
            .filter_map(|tool| self.catalog.get(tool))
            .map(Function::from)
            .collect();
        let preferences = [
            self.store
                .cloud_quality(target.provider.id(), &target.model)
                .map_err(internal)?,
            self.store
                .cloud_quality(target.provider.id(), &target.alternative)
                .map_err(internal)?,
        ];
        Ok(Request {
            message: message.to_string(),
            tools,
            target,
            preferences,
            limiter: self.cloud_limiter.clone(),
        })
    }

    pub fn finish_cloud_chat(
        &mut self,
        message: String,
        completed: crate::cloud::Completed,
        timer: Timer,
    ) -> Result<Outcome, CoreError> {
        use crate::cloud::{Answer, Failure};

        self.expire_pending()?;
        let mut task = Task::new(&message, json!({"kind": "owner", "id": "owner"}));
        task.transition("planned", "Modello remoto selezionato per l’attività");
        task.transition("running", "Inferenza remota");
        let crate::cloud::Completed {
            provider,
            model,
            answer,
        } = completed;
        let response = match answer {
            Ok(Answer::Call(call))
                if !call["tool"]
                    .as_str()
                    .is_some_and(|name| crate::cloud::TOOLS.contains(&name)) =>
            {
                Err(Failure::InvalidTool)
            }
            other => other,
        };
        let mut event = json!({
            "correlation_id": task.correlation(), "task_id": task.id(),
            "actor": {"kind": "owner", "id": "owner"}, "node": "core-local",
            "action": format!("cloud.{}", provider.id()), "outcome": "succeeded",
        });
        if let Err(error) = &response {
            event["outcome"] = json!("failed");
            event["error_code"] = json!(error.code());
            event["effect"] = json!("not_applied");
        }
        self.audit(event)?;
        self.store
            .message_add("user", &message, &task.id(), &now(), None, None)
            .map_err(internal)?;
        let backend = json!({"name": provider.id(), "location": "cloud", "model": model});
        let out = Outcome {
            steps: vec![json!({"kind": "delegation", "backend": backend})],
            backend: Some(backend),
            ..Outcome::default()
        };
        let result = match response {
            Ok(Answer::Text(text)) => {
                task.transition("succeeded", "Risposta del modello");
                let id = task.id();
                let mut result = self.finish_reply(task, out, text, Some(provider.id()))?;
                match self
                    .store
                    .cloud_feedback_candidate(&id, provider.id(), &model)
                {
                    Ok(()) => {
                        if let Some(backend) = &mut result.backend {
                            backend["feedback_available"] = json!(true);
                        }
                    }
                    Err(_) => self.obs.count("cloud.feedback.storage_failed"),
                }
                Ok(result)
            }
            Ok(Answer::Call(call)) => {
                self.save(&task)?;
                let inputs = json!({
                    "messages": [{"role": "user", "content": message}],
                    "memory": [], "cloud_backend": out.backend,
                });
                self.drive(
                    task,
                    inputs,
                    json!({"kind": "tool_call", "tool_call": call}),
                    out,
                    Vec::new(),
                )
            }
            Err(error) => {
                let code = error.code();
                let (text, retryable) = error.user_message(provider);
                task.fail(
                    json!({"code": code, "category": "unavailable", "message": text,
                    "retryable": retryable, "effect": "not_applied"}),
                    "Inferenza remota non riuscita",
                );
                self.finish(task, out, text)
            }
        };
        self.record_task("cloud", &timer, &result);
        result
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

    /// Prova il parser deterministico senza salvare né inviare al cloud i comandi riconosciuti.
    pub(crate) fn local_command(&mut self, message: &str) -> Result<Option<Outcome>, CoreError> {
        let message = message.trim();
        if message.is_empty() || message.chars().count() > MAX_MESSAGE {
            return Err(CoreError::BadRequest(
                "messaggio mancante o troppo lungo".into(),
            ));
        }
        if is_source_followup(message)
            || is_local_fact_request(message)
            || crate::desktop::Target::from_request(message).is_some()
        {
            return self.chat(message).map(Some);
        }
        let mut task = Task::new(message, json!({"kind": "owner", "id": "owner"}));
        let inputs = json!({"messages": [{"role": "user", "content": message}], "memory": []});
        let proposal = self.call_agent_inner(&task, &inputs).map_err(|_| {
            CoreError::Unavailable("Il parser dei comandi non è disponibile.".into())
        })?;
        if proposal["kind"] != "tool_call" {
            return Ok(None);
        }
        self.expire_pending()?;
        task.transition("planned", "Comando locale riconosciuto");
        task.transition("running", "Esecuzione del comando");
        self.store
            .message_add("user", message, &task.id(), &now(), None, None)
            .map_err(internal)?;
        self.save(&task)?;
        let out = Outcome {
            backend: Some(proposal["backend"].clone()),
            ..Outcome::default()
        };
        self.drive(task, inputs, proposal, out, Vec::new())
            .map(Some)
    }

    fn chat_inner(&mut self, message: &str) -> Result<Outcome, CoreError> {
        let message = message.trim();
        if message.is_empty() || message.chars().count() > MAX_MESSAGE {
            return Err(CoreError::BadRequest(format!(
                "il messaggio deve avere da 1 a {MAX_MESSAGE} caratteri"
            )));
        }
        self.expire_pending()?;
        let previous_sources = if is_source_followup(message) {
            Some(self.store.last_turn_sources().map_err(internal)?)
        } else {
            None
        };
        let mut task = Task::new(message, json!({"kind": "owner", "id": "owner"}));
        self.store
            .message_add("user", message, &task.id(), &now(), None, None)
            .map_err(internal)?;
        if let Some(sources) = previous_sources {
            let text = if sources.is_empty() {
                "La risposta precedente non ha fonti verificate. Quale informazione vuoi controllare?"
            } else {
                "Qui sotto trovi le fonti associate alla risposta precedente."
            };
            task.transition("planned", "Riferimento all’ultimo turno");
            task.transition("running", "Recupero delle fonti locali");
            task.transition("succeeded", "Risposta sulle fonti");
            return self.finish(
                task,
                Outcome {
                    sources,
                    ..Outcome::default()
                },
                text.into(),
            );
        }
        task.transition("planned", "Delega all'agente");
        task.transition("running", "Agente in esecuzione");
        self.save(&task)?;

        if let Some(target) = crate::desktop::Target::from_request(message) {
            let result = json!({"kind":"tool_call", "tool_call":{
                "tool":crate::desktop::TOOL, "version":"1.0.0", "arguments":{"target":target.id()}
            }});
            return self.drive(task, json!({}), result, Outcome::default(), Vec::new());
        }

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
        let agent = crate::agents::route(task.doc["objective"].as_str().unwrap_or_default())
            .map_or(crate::agents::GENERAL, |a| a.name);
        out.steps
            .push(json!({"kind": "delegation", "agent": agent, "backend": result["backend"]}));
        for s in inputs["skills"].as_array().into_iter().flatten() {
            out.steps
                .push(json!({"kind": "skill", "skill": s["id"], "name": s["name"]}));
        }
        self.drive(task, inputs, result, out, Vec::new())
    }

    /// Ciclo dell'agente (ADR-0037): ogni proposta di strumento passa da policy, audit ed eventuale conferma;
    /// dopo ogni risultato l'agente può chiedere un altro strumento, fino a `MAX_STEPS` passi per richiesta.
    /// Una proposta identica a una già eseguita in questo turno chiude il ciclo (niente ripetizioni a vuoto).
    fn drive(
        &mut self,
        mut task: Task,
        inputs: Value,
        mut result: Value,
        mut out: Outcome,
        mut history: Vec<Value>,
    ) -> Result<Outcome, CoreError> {
        loop {
            if result["kind"] == "reply" {
                let reason = if history.is_empty() {
                    "Risposta dell'agente"
                } else {
                    "Strumenti eseguiti e risposta composta"
                };
                let text = result["text"].as_str().unwrap_or_default().to_string();
                let last = history.last().unwrap_or(&Value::Null);
                let output = &last["output"];
                if output["effect"] == "unknown" {
                    task.fail(
                        json!({"code": "tool.uncertain_effect", "category": "unavailable",
                               "message": "Effetto della scrittura non verificabile", "retryable": false, "effect": "unknown"}),
                        "Esito dello strumento incerto",
                    );
                } else if matches!(last["status"].as_str(), Some("failed" | "denied")) {
                    let category = match (last["status"].as_str(), output["code"].as_str()) {
                        (_, Some("policy.invalid_arguments")) => "validation",
                        (Some("denied"), _) => "policy_denied",
                        _ => "unavailable",
                    };
                    task.fail(json!({
                        "code": output["code"].as_str().unwrap_or("tool.failed"),
                        "category": category,
                        "message": clip(output["message"].as_str().unwrap_or("Strumento non riuscito"), 2000),
                        "retryable": false,
                        "effect": output["effect"].as_str().unwrap_or("not_applied"),
                    }), "Strumento non riuscito");
                } else {
                    task.transition("succeeded", reason);
                }
                return self.finish(task, out, text);
            }
            let call = &result["tool_call"];
            let tool = call["tool"].as_str().unwrap_or_default().to_string();
            let version = call["version"].as_str().unwrap_or_default().to_string();
            let arguments = call["arguments"].clone();
            let digest = canonical_digest(
                &json!({"tool": tool, "version": version, "arguments": arguments}),
            );
            if history.iter().any(|h| h["digest"] == digest.as_str()) {
                out.steps.push(json!({"kind": "loop_guard", "tool": tool}));
                self.obs.count("agent.loop_guard");
                result = self.next_step(&task, &inputs, &history, true, &mut out);
                continue;
            }
            out.steps.push(json!({"kind": "tool_call", "tool": tool, "arguments": arguments, "rationale": call["rationale"]}));
            // Ambito della specializzazione (ADR-0038): uno strumento non concesso non arriva alla policy.
            if !self.task_allows(&task, &tool) {
                out.steps
                    .push(json!({"kind": "scope", "tool": tool, "allowed": false}));
                self.obs.count("agent.out_of_scope");
                let output = json!({"code": "policy.not_delegated", "message": policy::explain("policy.not_delegated")});
                history.push(step_record(&tool, &arguments, &digest, "denied", output));
                result = self.next_step(&task, &inputs, &history, true, &mut out);
                continue;
            }
            let (tid, corr) = (task.id(), task.correlation());
            let policy_timer = Timer::start();
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
            self.span(
                &task,
                &tool,
                "policy",
                policy_timer.ms(),
                decision["decision"].as_str().unwrap_or("unknown"),
                json!({"tool": tool, "risk": decision["risk"], "reasons": decision["reasons"]}),
            );
            let outcome_name = if decision["decision"] == "deny" {
                "denied"
            } else {
                "allowed"
            };
            self.audit(json!({
                "correlation_id": corr, "task_id": tid, "actor": {"kind": "agent", "id": "general_assistant"}, "node": "core-local",
                "action": decision["request"]["action"]["tool"], "decision_id": decision["decision_id"], "outcome": outcome_name,
            }))?;

            let (status, output) = match decision["decision"].as_str() {
                Some("deny") => {
                    let code = decision["reasons"][0].as_str().unwrap_or("policy.denied");
                    let mut output = json!({"code": code, "message": policy::explain(code)});
                    // Argomenti non validi (ADR-0045): l'agente riceve i difetti precisi e lo schema atteso
                    // e può correggere la chiamata nel passo successivo invece di chiudere il turno.
                    if code == "policy.invalid_arguments" {
                        if let Some(t) = self.catalog.get(&tool) {
                            let missing = missing_tool_parameters(t, &arguments);
                            if !missing.is_empty() {
                                let text = format!(
                                    "Per usare {tool}, mi servono questi parametri: {}. Puoi indicarli?",
                                    missing.join(", ")
                                );
                                out.steps.push(json!({"kind": "clarification", "tool": tool, "missing": missing}));
                                task.transition("succeeded", "Richiesti parametri al proprietario");
                                return self.finish(task, out, text);
                            }
                            let details: Vec<String> = t
                                .input
                                .iter_errors(&arguments)
                                .take(4)
                                .map(|e| {
                                    let at = e.instance_path.to_string();
                                    if at.is_empty() {
                                        e.to_string()
                                    } else {
                                        format!("{at}: {e}")
                                    }
                                })
                                .collect();
                            output["details"] = json!(details);
                            output["expected"] = t.descriptor["input_schema"].clone();
                        }
                    }
                    ("denied", output)
                }
                Some("require_confirmation") => {
                    return self.await_confirmation(
                        task, decision, tool, version, arguments, inputs, history, out,
                    );
                }
                _ if tool == "agent.delegate" => self.run_subagent(&task, &arguments, &mut out)?,
                _ => self.run_tool(&task, &tool, &arguments, false, &mut out)?,
            };
            if tool == "code.check_python" && output["succeeded"] == true {
                out.steps.push(json!({
                    "kind": "next_action",
                    "reason": "Sintassi verificata. Valuta un test con gli input previsti."
                }));
            }
            let retryable = output["code"] == "policy.invalid_arguments";
            history.push(step_record(&tool, &arguments, &digest, status, output));
            let last = history.len() >= MAX_STEPS || (status == "denied" && !retryable);
            result = self.next_step(&task, &inputs, &history, last, &mut out);
        }
    }

    /// Sospende il ciclo in attesa della conferma del proprietario e conserva il contesto per riprenderlo.
    #[allow(clippy::too_many_arguments)]
    fn await_confirmation(
        &mut self,
        mut task: Task,
        decision: Value,
        tool: String,
        version: String,
        arguments: Value,
        inputs: Value,
        history: Vec<Value>,
        mut out: Outcome,
    ) -> Result<Outcome, CoreError> {
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
            .message_add("assistant", &text, &task.id(), &now(), None, None)
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
                history,
            },
        );
        Ok(out)
    }

    /// Agente temporaneo (RF-03, ADR-0040): un task figlio con ruolo, obiettivo e pochi strumenti senza conferma,
    /// stessa traccia del principale, al massimo `SUBAGENT_STEPS` strumenti. Ogni strumento passa dalla policy
    /// e dall'audit; ciò che richiederebbe una conferma viene negato e lasciato all'agente principale.
    fn run_subagent(
        &mut self,
        parent: &Task,
        args: &Value,
        out: &mut Outcome,
    ) -> Result<(&'static str, Value), CoreError> {
        let fail =
            |code: &str, message: String| ("failed", json!({"code": code, "message": message}));
        if self.temporary.contains_key(&parent.id()) {
            return Ok(fail(
                "agent.limit",
                "un agente temporaneo non può crearne altri".into(),
            ));
        }
        if out.steps.iter().filter(|s| s["kind"] == "subagent").count() >= MAX_SUBAGENTS {
            return Ok(fail(
                "agent.limit",
                format!("al massimo {MAX_SUBAGENTS} agenti temporanei per richiesta"),
            ));
        }
        let role = args["role"].as_str().unwrap_or_default().to_string();
        let objective = args["objective"].as_str().unwrap_or_default().to_string();
        let default_tools =
            if crate::agents::route(&objective).is_some_and(|agent| agent.name == "coding") {
                &[
                    "fs.list",
                    "fs.read",
                    "fs.search",
                    "code.check_python",
                    "code.validate_json",
                    "math.calc",
                ][..]
            } else {
                &SUBAGENT_DEFAULT_TOOLS[..]
            };
        let requested: Vec<String> = match args["tools"].as_array() {
            Some(list) if !list.is_empty() => list
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect(),
            _ => default_tools.iter().map(|t| t.to_string()).collect(),
        };
        let invalid: Vec<&String> = requested
            .iter()
            .filter(|t| {
                t.as_str() == "agent.delegate"
                    || self.catalog.get(t).is_none_or(|d| {
                        !matches!(d.descriptor["effects"].as_str(), Some("read" | "none"))
                            || d.needs_confirmation()
                            || matches!(d.risk(), "high" | "critical")
                    })
            })
            .collect();
        if !invalid.is_empty() {
            return Ok(fail(
                "agent.invalid_tools",
                format!(
                    "strumenti non concedibili a un agente temporaneo: {}",
                    invalid
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        let timer = Timer::start();
        let mut sub = Task::new(
            &objective,
            json!({"kind": "agent", "id": "general_assistant"}),
        );
        sub.doc["correlation_id"] = parent.doc["correlation_id"].clone();
        sub.doc["parent_task_id"] = parent.id().into();
        sub.doc["success_conditions"] = json!([
            "Risposta fornita all'agente principale",
            "Ogni strumento eseguito ha superato la policy"
        ]);
        sub.transition(
            "planned",
            &format!(
                "Agente temporaneo: {}",
                role.chars().take(120).collect::<String>()
            ),
        );
        sub.transition("running", "Agente temporaneo in esecuzione");
        self.save(&sub)?;
        self.temporary.insert(
            sub.id(),
            TempAgent {
                role: role.clone(),
                tools: requested.clone(),
                steps: SUBAGENT_STEPS,
                actor: "temporary",
            },
        );
        let inputs = json!({"messages": [{"role": "user", "content": objective}], "memory": []});
        let (reply, history) = self.run_agent_loop(&sub, &inputs)?;
        self.temporary.remove(&sub.id());
        let tools_used: Vec<Value> = history
            .iter()
            .map(|h| json!({"tool": h["tool"], "status": h["status"]}))
            .collect();
        let (status, output) = match reply {
            Ok(text) => {
                sub.transition("succeeded", "Risposta dell'agente temporaneo");
                (
                    "succeeded",
                    json!({"agent_task_id": sub.id(), "role": role, "reply": text, "tools_used": tools_used}),
                )
            }
            Err(e) => {
                sub.fail(e.clone(), "Agente temporaneo non riuscito");
                (
                    "failed",
                    json!({"code": "agent.unavailable", "message": e["message"], "agent_task_id": sub.id()}),
                )
            }
        };
        self.save(&sub)?;
        self.span(
            &sub,
            "agente temporaneo",
            "task",
            timer.ms(),
            sub.doc["state"].as_str().unwrap_or("unknown"),
            json!({"role": role, "tools": tools_used.len()}),
        );
        out.steps.push(json!({"kind": "subagent", "task_id": sub.id(), "role": role, "status": status, "tools": tools_used}));
        Ok((status, output))
    }

    /// Esegue una chiamata proposta da un agente isolato: ambito concesso, ripetizioni, policy e audit
    /// (ADR-0040, ADR-0096). Restituisce impronta, esito e risultato; un rifiuto non è un errore del Core.
    fn isolated_call(
        &mut self,
        sub: &Task,
        actor: &str,
        call: (&str, &str, &Value),
        history: &[Value],
        out: &mut Outcome,
    ) -> Result<(String, &'static str, Value), CoreError> {
        let (tool, version, arguments) = call;
        let digest =
            canonical_digest(&json!({"tool": tool, "version": version, "arguments": arguments}));
        if !self.task_allows(sub, tool) || history.iter().any(|h| h["digest"] == digest.as_str()) {
            return Ok((
                digest,
                "denied",
                json!({"code": "policy.not_delegated", "message": policy::explain("policy.not_delegated")}),
            ));
        }
        let (sid, corr) = (sub.id(), sub.correlation());
        let decision = policy::decide(
            &self.catalog,
            &self.cfg.web,
            &policy::Request {
                subject: json!({"kind": "agent", "id": actor}),
                task_id: &sid,
                correlation_id: &corr,
                tool,
                version,
                arguments,
                digest: &digest,
            },
        );
        let allowed = decision["decision"] == "allow";
        self.audit(json!({
            "correlation_id": corr, "task_id": sid, "actor": {"kind": "agent", "id": actor}, "node": "core-local",
            "action": decision["request"]["action"]["tool"], "decision_id": decision["decision_id"],
            "outcome": if allowed { "allowed" } else { "denied" },
        }))?;
        let (status, output) = if allowed {
            self.run_tool(sub, tool, arguments, false, out)?
        } else {
            let code = decision["reasons"][0].as_str().unwrap_or("policy.denied");
            (
                "denied",
                json!({"code": code, "message": policy::explain(code)}),
            )
        };
        Ok((digest, status, output))
    }

    /// Ciclo di un agente isolato (ADR-0040, ADR-0096): propone strumenti uno alla volta, ciascuno passa dalla
    /// policy e dall'ambito concesso, finché risponde o esaurisce i passi. Restituisce risposta (o errore) e storico.
    fn run_agent_loop(
        &mut self,
        sub: &Task,
        inputs: &Value,
    ) -> Result<(Result<String, Value>, Vec<Value>), CoreError> {
        let (steps, actor) = self
            .temporary
            .get(&sub.id())
            .map_or((SUBAGENT_STEPS, "temporary"), |a| (a.steps, a.actor));
        let mut sub_out = Outcome::default();
        let mut history: Vec<Value> = Vec::new();
        let mut first = inputs.clone();
        first["steps_left"] = json!(steps);
        let mut result = self.call_agent(sub, &first);
        let reply = loop {
            let r = match result {
                Ok(r) => r,
                Err(e) => break Err(e),
            };
            if r["kind"] == "reply" {
                break Ok(r["text"].as_str().unwrap_or_default().to_string());
            }
            let call = &r["tool_call"];
            let tool = call["tool"].as_str().unwrap_or_default().to_string();
            let version = call["version"].as_str().unwrap_or_default().to_string();
            let arguments = call["arguments"].clone();
            let (digest, status, output) = self.isolated_call(
                sub,
                actor,
                (&tool, &version, &arguments),
                &history,
                &mut sub_out,
            )?;
            history.push(step_record(&tool, &arguments, &digest, status, output));
            let last = history.len() >= steps;
            result = Ok(self.next_step(sub, inputs, &history, last, &mut sub_out));
        };
        Ok((reply, history))
    }

    /// Passo successivo dell'agente con i risultati raccolti finora. Con `last` l'agente non riceve strumenti
    /// e deve rispondere. Se l'agente non è raggiungibile si usa un testo locale con l'ultimo esito.
    fn next_step(
        &mut self,
        task: &Task,
        inputs: &Value,
        history: &[Value],
        last: bool,
        out: &mut Outcome,
    ) -> Value {
        if let Some(step) = history
            .last()
            .filter(|step| step["tool"] == crate::desktop::TOOL)
        {
            let output = &step["output"];
            let text = match step["status"].as_str() {
                Some("succeeded") => format!(
                    "Ho richiesto di aprire {}.",
                    output["label"].as_str().unwrap_or("questo strumento")
                ),
                Some("cancelled") => "Apertura annullata.".to_string(),
                _ => output["message"]
                    .as_str()
                    .unwrap_or("Apertura non riuscita.")
                    .to_string(),
            };
            return json!({"kind":"reply", "text":text});
        }
        let last = last || inputs["cloud_backend"].is_object();
        if let Some(uncertain) = history
            .last()
            .filter(|h| h["output"]["effect"] == "unknown")
        {
            let tool = uncertain["tool"].as_str().unwrap_or("lo strumento");
            return json!({"kind": "reply", "text": format!(
                "Esito incerto per {tool}. La scrittura potrebbe essere arrivata a destinazione. Verifica lo stato prima di ripetere l’operazione."
            )});
        }
        let mut inputs = inputs.clone();
        // I passi precedenti all'ultimo viaggiano ridotti: bastano all'agente per orientarsi e tengono la
        // delega entro il budget di token (ADR-0024).
        let visible: Vec<Value> = history
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let raw = h["output"].to_string();
                let limit = if i + 1 == history.len() { LATEST_OUTPUT_CHARS } else { EARLIER_OUTPUT_CHARS };
                let output = if raw.chars().count() > limit {
                    json!({"truncated": true, "excerpt": clip(&raw, limit)})
                } else {
                    h["output"].clone()
                };
                json!({"tool": h["tool"], "status": h["status"], "output": output, "arguments": h["arguments"]})
            })
            .collect();
        if let Some(latest) = history.last() {
            inputs["tool_result"] = json!({"tool": latest["tool"], "status": latest["status"], "output": latest["output"]});
        }
        inputs["tool_history"] = Value::Array(visible);
        inputs["steps_left"] = json!(if last {
            0
        } else {
            MAX_STEPS.saturating_sub(history.len())
        });
        match self.call_agent(task, &inputs) {
            Ok(r) if r["kind"] == "reply" || (!last && r["kind"] == "tool_call") => {
                if !inputs["cloud_backend"].is_object() {
                    out.backend = Some(r["backend"].clone());
                }
                r
            }
            _ => {
                let latest = history.last().cloned().unwrap_or(Value::Null);
                let raw = latest["output"].to_string();
                let short: String = raw.chars().take(2000).collect();
                json!({"kind": "reply", "text": format!(
                    "Esito di {} ({}): {short}",
                    latest["tool"].as_str().unwrap_or("strumento"),
                    latest["status"].as_str().unwrap_or("sconosciuto")
                )})
            }
        }
    }

    fn owner_only_confirmation(pending: &Pending) -> bool {
        pending.tool == crate::desktop::TOOL
            || (std::env::var("JARVIS_MACOS_SESSION").as_deref() == Ok("1")
                && matches!(pending.decision["risk"].as_str(), Some("high" | "critical")))
    }

    pub(crate) fn requires_owner_confirmation(&self, id: &str) -> bool {
        self.pending
            .get(id)
            .is_some_and(Self::owner_only_confirmation)
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
            mut history,
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
        let mut out = Outcome {
            backend: inputs.get("cloud_backend").cloned(),
            ..Outcome::default()
        };
        let waited = task.doc["history"]
            .as_array()
            .and_then(|h| h.iter().rev().find(|x| x["to"] == "waiting_confirmation"))
            .and_then(|x| x["at"].as_str())
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                    .num_microseconds()
                    .unwrap_or(0) as f64
                    / 1000.0
            })
            .unwrap_or(0.0);
        self.span(
            &task,
            "conferma del proprietario",
            "confirmation",
            waited,
            if approve { "approved" } else { "rejected" },
            json!({"tool": tool}),
        );
        if approve {
            self.audit(merge(&base, json!({"outcome": "confirmed"})))?;
            out.steps
                .push(json!({"kind": "confirmation", "approved": true}));
            task.transition("running", "Confermato dal proprietario");
            let (status, output) = self.run_tool(&task, &tool, &arguments, true, &mut out)?;
            history.push(step_record(&tool, &arguments, &digest, status, output));
            let last = history.len() >= MAX_STEPS;
            let result = self.next_step(&task, &inputs, &history, last, &mut out);
            self.drive(task, inputs, result, out, history)
        } else {
            self.audit(merge(
                &base,
                json!({"outcome": "cancelled", "effect": "not_applied"}),
            ))?;
            out.steps
                .push(json!({"kind": "confirmation", "approved": false}));
            // Dopo un rifiuto l'agente risponde soltanto: nessun altro strumento in questo turno.
            history.push(step_record(
                &tool,
                &arguments,
                &digest,
                "cancelled",
                json!({"message": "rifiutato dal proprietario"}),
            ));
            let result = self.next_step(&task, &inputs, &history, true, &mut out);
            let text = result["text"].as_str().unwrap_or_default().to_string();
            task.transition("cancelled", "Rifiutato dal proprietario");
            self.finish(task, out, text)
        }
    }

    pub fn pending_confirmations(&self) -> Vec<Value> {
        self.pending_confirmations_for(false)
    }

    pub(crate) fn pending_confirmations_for(&self, remote: bool) -> Vec<Value> {
        self.pending
            .values()
            .filter(|p| !remote || !Self::owner_only_confirmation(p))
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
        let tool_timer = Timer::start();
        let result = self.execute_tool(tool, arguments, confirmed);
        let kind = match tool {
            t if t.starts_with("connector.") => "connector",
            t if t.starts_with("node.") => "node",
            "shell.run" | "sandbox.run" => "terminal",
            t if t.starts_with("web.") => "web",
            _ => "tool",
        };
        let attrs = match &result {
            Ok(_) => json!({"tool": tool, "confirmed": confirmed}),
            Err(e) => {
                json!({"tool": tool, "confirmed": confirmed, "error_code": e.code, "effect": self.failure_effect(tool, e.code)})
            }
        };
        self.span(
            task,
            tool,
            kind,
            tool_timer.ms(),
            if result.is_ok() { "ok" } else { "error" },
            attrs,
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
                Ok((
                    "failed",
                    json!({"code": e.code, "message": e.message, "effect": effect}),
                ))
            }
        }
    }

    fn finish(&mut self, task: Task, out: Outcome, text: String) -> Result<Outcome, CoreError> {
        self.finish_reply(task, out, text, None)
    }

    fn finish_reply(
        &mut self,
        task: Task,
        mut out: Outcome,
        text: String,
        cloud_provider: Option<&str>,
    ) -> Result<Outcome, CoreError> {
        self.save(&task)?;
        let created = task.doc["created_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                    .num_microseconds()
                    .unwrap_or(0) as f64
                    / 1000.0
            })
            .unwrap_or(0.0);
        let tools = out
            .steps
            .iter()
            .filter(|s| s["kind"] == "tool_call")
            .count();
        self.span(
            &task,
            "richiesta",
            "task",
            created,
            task.doc["state"].as_str().unwrap_or("unknown"),
            json!({"steps": out.steps.len(), "tools": tools}),
        );
        if out
            .backend
            .as_ref()
            .is_none_or(|backend| backend["location"] != "cloud")
        {
            self.record_episode(&task, &out);
        }
        self.store
            .message_add(
                "assistant",
                &text,
                &task.id(),
                &now(),
                Some(&Value::Array(out.sources.clone())),
                cloud_provider,
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
            .filter(|m| !crate::cloud::contains_credentials(m["text"].as_str().unwrap_or_default()))
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
            if crate::cloud::contains_credentials(text) {
                continue;
            }
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
        let touches_memory = out.steps.iter().any(|s| {
            s["kind"] == "tool_call" && s["tool"].as_str().is_some_and(|t| t.starts_with("memory."))
        });
        if touches_memory {
            return;
        }
        let full_objective = task.doc["objective"].as_str().unwrap_or_default();
        if crate::cloud::contains_credentials(full_objective) {
            return;
        }
        let objective: String = full_objective.chars().take(160).collect();
        if objective.trim().is_empty() {
            return;
        }
        let state = task.doc["state"].as_str().unwrap_or("unknown");
        let tools: Vec<&str> = out
            .steps
            .iter()
            .filter(|s| s["kind"] == "tool_call")
            .filter_map(|s| s["tool"].as_str())
            .collect();
        let text = if tools.is_empty() {
            format!("Richiesta «{objective}»: risposta diretta, esito {state}")
        } else {
            format!(
                "Richiesta «{objective}»: {}, esito {state}",
                tools.join(" → ")
            )
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

    pub(crate) fn owner_conversation(&self, limit: u32) -> Result<Vec<Value>, CoreError> {
        self.store
            .conversation_recent(limit, true)
            .map_err(internal)
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
        if crate::cloud::contains_credentials(text) {
            return Err(CoreError::BadRequest(
                "La memoria non deve contenere credenziali.".into(),
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

    pub fn n8n(&self) -> std::sync::Arc<crate::n8n::Manager> {
        self.n8n.clone()
    }

    pub fn n8n_notices(&mut self, completed: &[Value]) -> Result<(), CoreError> {
        for job in completed {
            let (severity, body) = if job["kind"] == "audit" {
                match self.security_status() {
                    Ok(report)
                        if report["audit_chain_valid"] == true
                            && report["signatures"]["valid"] == true =>
                    {
                        ("success", "Audit integro e firme valide.")
                    }
                    _ => (
                        "warning",
                        "Verifica l’integrità del registro nel Centro sicurezza.",
                    ),
                }
            } else {
                ("info", "Promemoria pianificato da n8n.")
            };
            self.notify(json!({
                "severity": severity, "category": "integration", "title": job["name"], "body": body,
                "source": {"kind": "module", "id": "n8n"}, "link": {"view": "automations"},
                "dedupe_key": format!("n8n:{}:{}", job["id"].as_str().unwrap_or_default(),
                                      job["last_run"].as_u64().unwrap_or_default()),
            }))?;
        }
        Ok(())
    }

    pub fn security_status(&self) -> Result<Value, CoreError> {
        let audit = self.audit_log(0)?;
        let devices = self.devices()?;
        let active = devices.iter().filter(|d| d["revoked_at"].is_null()).count();
        Ok(json!({
            "audit_chain_valid": audit["chain_valid"], "audit_events": audit["total"],
            "signatures": {"enabled": audit["signatures"]["enabled"], "valid": audit["signatures"]["valid"]},
            "secrets_store": self.secrets.location(), "devices_active": active,
            "pending_confirmations": self.pending.len(), "web_enabled": self.cfg.web.enabled,
            "web_private_addresses": self.cfg.web.allow_private, "shell_enabled": !policy::shell_disabled(),
        }))
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
            "n8n.create" if confirmed => self
                .n8n
                .add(arguments)
                .map_err(|reason| tool_error("n8n.unavailable", reason)),
            "n8n.create" => Err(tool_error(
                "n8n.unavailable",
                "la routine richiede conferma",
            )),
            "security.status" => self.security_status().map_err(|_| {
                tool_error(
                    "security.unavailable",
                    "controllo della postura non riuscito",
                )
            }),
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
            crate::desktop::TOOL if confirmed => {
                let target =
                    crate::desktop::Target::parse(arguments["target"].as_str().unwrap_or_default())
                        .ok_or_else(|| {
                            tool_error("pc.unavailable", "Destinazione desktop non disponibile")
                        })?;
                crate::desktop::open(target, &self.cfg.workspace())
            }
            crate::desktop::TOOL => Err(tool_error(
                "pc.unavailable",
                "È richiesta una conferma locale",
            )),
            "code.run_python" if confirmed => {
                crate::sandbox::run_python(arguments["source"].as_str().unwrap_or_default())
            }
            "code.run_python" => Err(tool_error(
                "code.not_confirmed",
                "L’esecuzione richiede la conferma del proprietario",
            )),
            // Terminale sandbox (ADR-0105): isolato, quindi senza conferma per comando.
            "sandbox.run" => crate::sandbox::terminal_run(&self.cfg.data_dir, arguments),
            "sandbox.status" => Ok(crate::sandbox::terminal_status(&self.cfg.data_dir)),
            "sandbox.reset" if confirmed => {
                crate::sandbox::terminal_reset(&self.cfg.data_dir, arguments)
            }
            "sandbox.reset" => Err(tool_error(
                "sandbox.not_confirmed",
                "L’azzeramento richiede la conferma del proprietario",
            )),
            "shell.run" => {
                if !confirmed {
                    return Err(tool_error(
                        "shell.not_confirmed",
                        "ogni comando del terminale richiede la conferma del proprietario",
                    ));
                }
                let workspace = self.cfg.workspace();
                tools::shell_run(
                    &workspace,
                    arguments["command"].as_str().unwrap_or_default(),
                )
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
            if let Some(reason) = workflow_tool_restriction(name) {
                return Err(invalid(format!("passo {}: {reason}", i + 1)));
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
        self.proactive_poll(now_t);
        // Cose da fare scadute (ADR-0046): una notifica per ciascuna, una sola volta.
        let now_s = now_t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        for t in crate::toolbox::todos_due(&self.store, &now_s) {
            self.notify(json!({
                "severity": if t["priority"] == "alta" { "warning" } else { "info" }, "category": "reminder",
                "title": format!("Da fare: {}", t["text"].as_str().unwrap_or_default().chars().take(80).collect::<String>()),
                "body": "Scadenza raggiunta. Scrivi «fatto <id>» quando l'hai completata.",
                "source": {"kind": "module", "id": "todo"}, "dedupe_key": format!("todo:{}", t["id"].as_str().unwrap_or_default()),
                "link": {"view": "chat"},
            }))?;
        }
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
                       "input_schema": d["input_schema"], "workflow_step": workflow_tool_restriction(t.name()).is_none()})
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
        // Nessun client in ascolto non è un errore.
        let _ = self.events.send(doc.clone());
        self.obs.count(&format!(
            "notification.{}",
            doc["severity"].as_str().unwrap_or("info")
        ));
        self.obs.log("info", "notification", json!({"severity": doc["severity"], "category": doc["category"],
            "task_id": doc["task_id"], "workflow_id": doc["workflow_id"], "correlation_id": doc["correlation_id"]}));
        self.notify_external(&doc);
        Ok(Some(doc))
    }

    /// Riceve le notifiche appena create (flusso in tempo reale, ADR-0098).
    pub fn subscribe_notifications(&self) -> tokio::sync::broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// Stato della consegna push senza token: configurazione, registrazioni e ultimo invio riuscito.
    pub fn push_status(&self) -> Value {
        self.push.status()
    }

    /// Inoltro ai canali esterni configurati (ADR-0026), in background e fuori dal blocco del Core.
    fn notify_external(&self, doc: &Value) {
        if let Ok(channels) = self.store.channels() {
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
        self.push.dispatch(&self.secrets, doc, self.obs.clone());
    }

    /// Registra un token FCM senza mai restituirlo. Principal: proprietario o dispositivo associato.
    pub fn push_register(
        &self,
        principal: &str,
        token: &str,
        platform: crate::push::Platform,
    ) -> Result<Value, CoreError> {
        self.push
            .register(&self.secrets, principal, token, platform)
            .map_err(CoreError::BadRequest)?;
        Ok(json!({
            "registered": true,
            "delivery_status": crate::push::delivery_config_status(&self.cfg.data_dir)
        }))
    }

    /// Rimuove tutte le registrazioni FCM del principal corrente.
    pub fn push_remove(&self, principal: &str) -> Result<Value, CoreError> {
        let removed = self
            .push
            .remove_for_owner(&self.secrets, principal)
            .map_err(internal)?;
        Ok(json!({"removed": removed}))
    }

    // ------------------------------------------------------------------ aggiornamenti (ADR-0034)

    /// Stato del database (integrità rapida, schema, dimensioni) per la sezione Sistema.
    pub fn database_health(&self) -> Result<Value, CoreError> {
        self.store.health().map_err(internal)
    }

    pub fn data_dir(&self) -> std::path::PathBuf {
        self.cfg.data_dir.clone()
    }

    pub fn updates_state(&self) -> Result<Value, CoreError> {
        let mut state = self.updates.state();
        state["github_token_configured"] = self
            .secrets
            .get("updates:github_token")
            .map_err(internal)?
            .is_some()
            .into();
        Ok(state)
    }

    pub fn updates_configure(
        &mut self,
        enabled: bool,
        channel: &str,
        github_token: Option<&str>,
        remove_github_token: bool,
    ) -> Result<Value, CoreError> {
        if !crate::updates::CHANNELS.contains(&channel) {
            return Err(CoreError::BadRequest(format!(
                "canale sconosciuto: {channel} (stable o beta)"
            )));
        }
        if github_token.is_some() && remove_github_token {
            return Err(CoreError::BadRequest(
                "non puoi impostare e rimuovere il token nella stessa richiesta".into(),
            ));
        }
        if let Some(token) = github_token {
            if token.is_empty()
                || token.len() > 512
                || token.bytes().any(|byte| !byte.is_ascii_graphic())
            {
                return Err(CoreError::BadRequest("token GitHub non valido".into()));
            }
            self.secrets
                .put("updates:github_token", token)
                .map_err(internal)?;
            self.updates.reset_check().map_err(CoreError::Internal)?;
        } else if remove_github_token {
            self.secrets
                .delete("updates:github_token")
                .map_err(internal)?;
            self.updates.reset_check().map_err(CoreError::Internal)?;
        }
        self.updates
            .configure(enabled, channel)
            .map_err(CoreError::BadRequest)?;
        let action = if github_token.is_some() {
            "updates.github_token.configure"
        } else if remove_github_token {
            "updates.github_token.remove"
        } else {
            "updates.configure"
        };
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": action, "outcome": "succeeded", "effect": "applied"}))?;
        self.updates_state()
    }

    pub fn updates_fetch_config(&self) -> Result<(std::path::PathBuf, Option<String>), CoreError> {
        Ok((
            self.cfg.data_dir.clone(),
            self.secrets.get("updates:github_token").map_err(internal)?,
        ))
    }

    /// URL dell'elenco delle release se il controllo automatico è dovuto (la richiesta va fuori dal blocco).
    pub fn updates_due(&self) -> bool {
        self.updates.due(chrono::Utc::now())
    }

    /// Registra l'esito e notifica una versione più recente, una sola volta per versione.
    pub fn updates_record(
        &mut self,
        result: Result<Vec<Value>, String>,
    ) -> Result<Value, CoreError> {
        let (_, newer) = self.updates.record(result).map_err(CoreError::Internal)?;
        if let Some(r) = newer {
            let tag = r["tag"].as_str().unwrap_or_default().to_string();
            self.notify(json!({
                "severity": "info", "category": "system", "title": format!("Aggiornamento disponibile: JARVIS {tag}"),
                "body": format!("Hai la versione {}. Scarica {tag} dalla pagina della release e verifica la firma: {}",
                                env!("CARGO_PKG_VERSION"), r["url"].as_str().unwrap_or_default()),
                "source": {"kind": "module", "id": "updates"}, "dedupe_key": format!("update:{tag}"),
                "link": {"view": "system"},
            }))?;
        }
        self.updates_state()
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

    /// Arresto ordinato (ADR-0103): ferma il worker AI senza attendere il drop del Core e porta il WAL del
    /// database nel file principale, così l'uscita immediata del processo non lascia nulla in sospeso.
    pub fn shutdown(&mut self) {
        self.worker = None;
        let _ = self.store.maintenance();
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
        "web.browse" | "web.form" => vec![json!({
            "kind": "page", "title": output["title"], "url": output["url"], "requested_url": output["requested_url"],
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
/// Giorni di conservazione delle tracce dei task (ADR-0039).
const TRACE_KEEP_DAYS: i64 = 30;
const DB_MAINTENANCE_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

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
        if self
            .maintained
            .is_none_or(|t| t.elapsed() >= DB_MAINTENANCE_EVERY)
        {
            self.maintained = Some(std::time::Instant::now());
            let timer = Timer::start();
            let done = self.store.maintenance();
            self.obs
                .observe("db maintenance", timer.ms(), done.is_err());
            let _ = self.store.memory_purge_expired(&now());
            let keep = (chrono::Utc::now() - chrono::Duration::days(TRACE_KEEP_DAYS))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let _ = self.store.spans_prune(&keep);
            let done = (chrono::Utc::now() - chrono::Duration::days(90))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let _ = self.store.history_prune(RUNS_KEEP_PER_WORKFLOW, &done);
        }
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

/// Esecuzioni conservate per ogni automazione.
pub const RUNS_KEEP_PER_WORKFLOW: u32 = 200;

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

    pub(crate) fn cancel_pairing(&mut self, code: &str) -> bool {
        self.pairings.remove(code).is_some()
    }

    /// Associa un dispositivo con un codice valido. Il token è restituito una sola volta; il Core ne
    /// conserva solo l'impronta SHA-256. Dopo 5 codici errati tutti i codici in attesa vengono annullati.
    /// Registra una passkey verificata per un dispositivo attivo (ADR-0042), con voce di audit.
    pub fn passkey_register(
        &mut self,
        device_id: &str,
        reg: &crate::passkeys::Registered,
    ) -> Result<Value, CoreError> {
        let id = crate::passkeys::b64url(&reg.credential_id);
        self.store
            .passkey_add(&id, device_id, &reg.public_key, reg.sign_count, &now())
            .map_err(internal)?;
        self.device_audit("passkey.register", device_id, "succeeded")?;
        Ok(json!({"credential_id": id}))
    }

    /// Passkey di un dispositivo, senza chiavi: id, creazione e ultimo uso.
    pub fn passkeys(&self, device_id: &str) -> Result<Vec<Value>, CoreError> {
        Ok(self
            .store
            .passkeys_for(device_id)
            .map_err(internal)?
            .into_iter()
            .map(|(id, _, _, created, used)| json!({"credential_id": id, "created_at": created, "last_used_at": used}))
            .collect())
    }

    /// Verifica un'asserzione per una conferma: la passkey deve appartenere al dispositivo.
    #[allow(clippy::too_many_arguments)]
    pub fn passkey_check(
        &mut self,
        device_id: &str,
        credential_id: &str,
        client_data: &[u8],
        authenticator_data: &[u8],
        signature: &[u8],
        challenge: &str,
        origins: &[String],
        rp_id: &str,
    ) -> Result<(), CoreError> {
        let keys = self.store.passkeys_for(device_id).map_err(internal)?;
        let Some((_, public, count, _, _)) = keys.iter().find(|k| k.0 == credential_id) else {
            self.passkey_refused(device_id)?;
            return Err(CoreError::Unauthorized(
                "passkey non registrata per questo dispositivo".into(),
            ));
        };
        match crate::passkeys::verify_assertion(
            client_data,
            authenticator_data,
            signature,
            public,
            *count,
            challenge,
            origins,
            rp_id,
        ) {
            Ok(n) => {
                self.store
                    .passkey_used(credential_id, n, &now())
                    .map_err(internal)?;
                self.device_audit("passkey.assert", device_id, "succeeded")?;
                Ok(())
            }
            Err(e) => {
                self.passkey_refused(device_id)?;
                Err(CoreError::Unauthorized(e))
            }
        }
    }

    /// Asserzione rifiutata: nessun effetto, voce di audit per chi controlla gli accessi.
    fn passkey_refused(&mut self, device_id: &str) -> Result<(), CoreError> {
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "client", "id": "device"},
                          "node": "core-local", "action": "passkey.assert", "resource": format!("jarvis://device/{device_id}"),
                          "outcome": "cancelled", "effect": "not_applied"}))?;
        Ok(())
    }

    /// Elimina una passkey (solo dal computer del proprietario).
    pub fn passkey_delete(&mut self, credential_id: &str) -> Result<(), CoreError> {
        if !self.store.passkey_delete(credential_id).map_err(internal)? {
            return Err(CoreError::NotFound("passkey inesistente".into()));
        }
        self.owner_audit("passkey.delete")
    }

    /// Attiva l'mTLS dei nodi sulla porta indicata (ADR-0041): identità del Core in `<dati>/tls`, creata al
    /// primo avvio. I nodi associati da qui in poi si legano al proprio certificato.
    pub fn enable_node_tls(&mut self, port: u16) -> Result<crate::tls::Identity, String> {
        let id =
            crate::tls::Identity::load_or_create(&self.cfg.data_dir.join("tls"), "jarvis-core")?;
        self.node_tls = Some((port, id.fingerprint()?));
        Ok(id)
    }

    pub fn pair_device(
        &mut self,
        code: &str,
        name: &str,
        platform: &str,
        capabilities: &[String],
        cert_pem: Option<&str>,
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
        let platform = if matches!(platform, "android" | "desktop") {
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
            // mTLS (ADR-0041): con la porta dei nodi attiva il nodo si lega al certificato inviato.
            if let (Some(_), Some(pem)) = (&self.node_tls, cert_pem) {
                let cert = crate::tls::cert_from_pem(pem).map_err(CoreError::BadRequest)?;
                doc["cert_sha256"] = crate::tls::fingerprint(cert.as_ref()).into();
            }
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
        let mtls = match (&self.node_tls, doc.get("cert_sha256")) {
            (Some((port, fp)), Some(_)) => json!({"port": port, "core_cert_sha256": fp}),
            _ => Value::Null,
        };
        Ok(json!({"device": doc, "token": token, "mtls": mtls}))
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
            if !self.store.device_touch(&doc, &hash).map_err(internal)? {
                return Ok(None);
            }
        }
        Ok(Some(doc))
    }

    pub fn devices(&self) -> Result<Vec<Value>, CoreError> {
        self.store.devices().map_err(internal)
    }

    /// Aggiorna catalogo e audit dopo il commit della revoca.
    pub(crate) fn record_device_revocation(
        &mut self,
        id: &str,
        doc: &Value,
    ) -> Result<(), CoreError> {
        let push_cleanup = self.push.remove_for_owner(&self.secrets, id);
        if let Some(slug) = doc["slug"].as_str() {
            // Un nuovo nodo può aver già riutilizzato lo slug mentre l'API aspettava il lock del Core.
            if !self.node_devices().iter().any(|d| d["slug"] == slug) {
                self.catalog.remove_prefix(&format!("node.{slug}."));
            }
            self.refresh_auto_node_tools();
        }
        let audit = self.device_audit("device.revoke", id, "revoked");
        push_cleanup.map_err(internal)?;
        audit
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
        if let Some(fp) = doc["cert_sha256"].as_str() {
            self.nodes.bind_cert(id, fp);
        }
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
            // Scelta automatica (ADR-0031): lo strumento esiste finché almeno un nodo lo offre.
            self.catalog.insert_dynamic(crate::nodes::tool_descriptor(
                &base,
                "auto",
                "scelto automaticamente",
            ))?;
        }
        Ok(())
    }

    /// Strumenti `node.auto.*` coerenti con i nodi rimasti dopo una revoca.
    fn refresh_auto_node_tools(&mut self) {
        self.catalog.remove_prefix("node.auto.");
        let docs = self.node_devices();
        for doc in docs {
            for cap in doc["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if let Some(base) = self.catalog.get(cap).map(|t| t.descriptor.clone()) {
                    let _ = self.catalog.insert_dynamic(crate::nodes::tool_descriptor(
                        &base,
                        "auto",
                        "scelto automaticamente",
                    ));
                }
            }
        }
    }

    /// Nodi associati e non revocati.
    fn node_devices(&self) -> Vec<Value> {
        self.store
            .devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| {
                d.get("revoked_at").is_none()
                    && d["scopes"]
                        .as_array()
                        .is_some_and(|s| s.iter().any(|x| x == "node"))
            })
            .collect()
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
        let node = if slug == "auto" {
            // Scelta automatica (ADR-0031): il nodo in linea più libero fra quelli che offrono lo strumento.
            let offering: Vec<Value> = self
                .node_devices()
                .into_iter()
                .filter(|d| {
                    d["capabilities"]
                        .as_array()
                        .is_some_and(|c| c.iter().any(|x| x == base))
                })
                .collect();
            let ids: Vec<String> = offering
                .iter()
                .filter_map(|d| d["device_id"].as_str().map(str::to_string))
                .collect();
            let chosen = self
                .nodes
                .pick(&ids, std::time::Duration::from_secs(60))
                .ok_or_else(|| {
                    tool_error("node.offline", format!("nessun nodo in linea offre {base}"))
                })?;
            offering
                .into_iter()
                .find(|d| d["device_id"] == chosen.as_str())
                .unwrap_or_default()
        } else {
            self.store
                .devices()
                .map_err(|e| tool_error("node.unavailable", e.to_string()))?
                .into_iter()
                .find(|d| d["slug"] == slug && d.get("revoked_at").is_none())
                .ok_or_else(|| tool_error("node.revoked", "nodo non associato o revocato"))?
        };
        let slug = node["slug"].as_str().unwrap_or(slug).to_string();
        let slug = slug.as_str();
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
        let assignment = self
            .nodes
            .enqueue(&id, base, arguments)
            .ok_or_else(|| tool_error("node.revoked", "nodo non associato o revocato"))?;
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

    /// Competenze del proprietario (ADR-0048).
    pub fn skills(&self) -> Result<Value, CoreError> {
        Ok(
            json!({"items": crate::skills::list(&self.store).map_err(internal)?, "max_active": crate::skills::MAX_ACTIVE}),
        )
    }

    /// Crea (`id` assente) o aggiorna una competenza; gli strumenti devono esistere nel catalogo.
    pub fn skill_save(&mut self, id: Option<&str>, input: &Value) -> Result<Value, CoreError> {
        if let Some(id) = id {
            let exists = crate::skills::list(&self.store)
                .map_err(internal)?
                .iter()
                .any(|s| s["id"] == id);
            if !exists {
                return Err(CoreError::NotFound("competenza inesistente".into()));
            }
        }
        let skill = crate::skills::validate(input, |t| self.catalog.get(t).is_some())
            .map_err(CoreError::BadRequest)?;
        let id = crate::skills::save(&self.store, id, &skill).map_err(internal)?;
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "skill.save", "resource": format!("jarvis://skill/{id}"),
                          "outcome": "succeeded", "effect": "applied"}))?;
        Ok(merge(&skill, json!({"id": id})))
    }

    pub fn skill_delete(&mut self, id: &str) -> Result<Value, CoreError> {
        let builtin = crate::skills::list(&self.store)
            .map_err(internal)?
            .iter()
            .any(|s| s["id"] == id && s["builtin"] == true);
        if builtin {
            return Err(CoreError::BadRequest(
                "le competenze incluse non si eliminano: disattivala".into(),
            ));
        }
        if !crate::skills::delete(&self.store, id).map_err(internal)? {
            return Err(CoreError::NotFound("competenza inesistente".into()));
        }
        self.audit(json!({"correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "owner", "id": "owner"},
                          "node": "core-local", "action": "skill.delete", "resource": format!("jarvis://skill/{id}"),
                          "outcome": "succeeded", "effect": "applied"}))?;
        Ok(json!({"deleted": id}))
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
            "has_credential": has(if manifest["auth"]["type"] == "oauth2" { oauth_name(id) } else { token_name(id) }),
            "webhook": manifest["webhook"]["enabled"] == true, "webhook_path": format!("/api/hooks/{id}"),
            "operations": manifest["operations"].as_array().map(|ops| ops.iter().map(|o| json!({
                "tool": crate::connectors::tool_name(id, o["name"].as_str().unwrap_or_default()),
                "summary": o["summary"], "method": o["method"], "effects": o["effects"], "risk": o["risk"],
                "input_schema": o["input_schema"],
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

    pub fn install_new_connector(&mut self, manifest: &Value) -> Result<Value, CoreError> {
        if self
            .store
            .connectors()
            .map_err(internal)?
            .iter()
            .any(|(item, _, _)| item["id"] == manifest["id"])
        {
            return Err(CoreError::BadRequest("Connettore già installato".into()));
        }
        self.install_connector(manifest)
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
        self.oauth.cancel(&id);
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
        let (manifest, _) = self.connector_manifest(id)?;
        self.oauth.cancel(id);
        // OAuth (ADR-0027): prima della cancellazione locale si chiede al servizio di invalidare i token.
        let remote =
            if manifest["auth"]["type"] == "oauth2" && manifest["auth"]["revoke_url"].is_string() {
                let state: Value = self
                    .secrets
                    .get(&oauth_name(id))
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Null);
                let r = crate::oauth::revoke(
                    &manifest,
                    &self.cfg.web,
                    &state,
                    state["access_token"].as_str(),
                );
                self.obs.count(if r.is_ok() {
                    "connector.oauth.revoked_remote"
                } else {
                    "connector.oauth.revoke_failed"
                });
                Some(r)
            } else {
                None
            };
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
        Ok(
            json!({"id": id, "tools_removed": removed, "uninstalled": uninstall,
                  "remote_revoked": remote.as_ref().map(Result::is_ok),
                  "remote_error": remote.and_then(Result::err)}),
        )
    }

    pub fn google_oauth_start(&mut self, id: &str, file: &Value) -> Result<Value, CoreError> {
        if !matches!(id, "gmail" | "google_calendar") {
            return Err(CoreError::BadRequest(
                "Importazione disponibile per Gmail e Google Calendar.".into(),
            ));
        }
        let (client_id, secret) =
            crate::connector_catalog::google_credentials(file).map_err(CoreError::BadRequest)?;
        let manifest = crate::connector_catalog::manifest(id, Some(client_id))
            .map_err(CoreError::BadRequest)?;
        let normalized = crate::connectors::validate_manifest(&manifest, &self.cfg.web)
            .map_err(CoreError::BadRequest)?;
        match self.connector_manifest(id) {
            Ok((existing, _)) if existing == normalized => {}
            Ok(_) => return Err(CoreError::BadRequest(
                "Il connettore è installato con un altro client OAuth. Rimuovilo prima di sostituirlo.".into(),
            )),
            Err(CoreError::NotFound(_)) => { self.install_new_connector(&manifest)?; }
            Err(error) => return Err(error),
        }
        self.oauth_start(id, Some(secret.into()))
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
            .filter(|s| !s.is_empty());
        if secret
            .as_ref()
            .is_some_and(|s| s.len() > 4096 || !s.bytes().all(|b| b.is_ascii_graphic()))
        {
            return Err(CoreError::BadRequest("client secret non valido".into()));
        }
        let secret = match secret {
            Some(secret) => Some(secret),
            None => self
                .secrets
                .get(&oauth_name(id))
                .map_err(internal)?
                .and_then(|state| serde_json::from_str::<Value>(&state).ok())
                .and_then(|state| state["client_secret"].as_str().map(str::to_owned)),
        };
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
        if denied.is_some() {
            self.connector_audit("connector.oauth.denied", &id, "cancelled")?;
            return Err(CoreError::BadRequest(
                "il servizio non ha concesso l'accesso".into(),
            ));
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
            .put(&oauth_name(id), &tokens.to_string())
            .map_err(CoreError::Internal)
    }

    /// Rinnovo automatico dell'access token a 60 secondi dalla scadenza, o subito con `force` (token
    /// rifiutato dal servizio prima della scadenza dichiarata).
    fn oauth_access_token(
        &self,
        id: &str,
        manifest: &Value,
        force: bool,
    ) -> Result<String, String> {
        let mut state: Value = self
            .secrets
            .get(&oauth_name(id))?
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or("connettore non autorizzato: premi «Accedi»")?;
        if force || crate::oauth::expiring(&state) {
            let renewed = crate::oauth::refresh(manifest, &self.cfg.web, &state);
            self.obs.count(if renewed.is_ok() {
                "connector.oauth.refreshed"
            } else {
                "connector.oauth.refresh_failed"
            });
            state = renewed?;
            self.save_oauth_tokens(id, &state)
                .map_err(|e| format!("{e:?}"))?;
        }
        state["access_token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "access token mancante: autorizza di nuovo il connettore".into())
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
        let mut secret = if manifest["auth"]["type"] == "oauth2" {
            Some(
                self.oauth_access_token(id, &manifest, false)
                    .map_err(|e| tool_error("connector.not_authorized", e))?,
            )
        } else {
            self.secrets
                .get(&token_name(id))
                .map_err(|e| tool_error("connector.unavailable", e))?
        };
        let mut renewed = false;
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
                // Un 401 con OAuth: rinnovo immediato e una sola nuova chiamata (anche per le scritture, perché
                // il servizio ha rifiutato la richiesta senza eseguirla).
                Err(e)
                    if e.code == "connector.token_rejected"
                        && !renewed
                        && manifest["auth"]["type"] == "oauth2" =>
                {
                    renewed = true;
                    secret = Some(
                        self.oauth_access_token(id, &manifest, true)
                            .map_err(|e| tool_error("connector.not_authorized", e))?,
                    );
                    attempt -= 1;
                }
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
        if (write_external
            && matches!(
                code,
                "web.timeout" | "web.unreachable" | "connector.server_error"
            ))
            || (node_write && matches!(code, "node.timeout" | "node.revoked"))
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

    pub fn backup_directory(&self) -> std::path::PathBuf {
        self.cfg.backups_dir()
    }

    pub fn backups(&self) -> Value {
        json!({"dir": self.cfg.backups_dir().to_string_lossy(), "items": crate::backup::list(&self.cfg.backups_dir()),
               "rollback": crate::rollback::list(&self.cfg.data_dir).iter().rev().filter_map(|p| p.file_name().map(|name| name.to_string_lossy())).collect::<Vec<_>>()})
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

fn workflow_tool_restriction(tool: &str) -> Option<&'static str> {
    match tool {
        name if name.starts_with("workflow.") => {
            Some("un'automazione non può gestire altre automazioni")
        }
        crate::desktop::TOOL => Some("le azioni sul PC richiedono una richiesta diretta"),
        "shell.run" | "code.run_python" | "sandbox.run" | "sandbox.reset" => {
            Some("i comandi del terminale e del sandbox richiedono una conferma diretta")
        }
        _ => None,
    }
}

fn missing_tool_parameters(tool: &crate::catalog::Tool, arguments: &Value) -> Vec<String> {
    tool.input
        .iter_errors(arguments)
        .filter_map(|error| {
            let jsonschema::error::ValidationErrorKind::Required { property } = error.kind else {
                return None;
            };
            let name = property.as_str()?;
            let path = error.instance_path.to_string();
            Some(if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            })
        })
        .take(8)
        .collect()
}

fn explicit_connector_tool(objective: &str) -> Option<String> {
    let mut command = objective.split_whitespace();
    match (command.next(), command.next(), command.next()) {
        (Some("connettore"), Some(id), Some(operation)) => {
            Some(crate::connectors::tool_name(id, operation))
        }
        _ => None,
    }
}

/// I comandi deterministici si risolvono nel Core anche se è stata scelta la chat cloud.
pub(crate) fn is_local_fact_request(message: &str) -> bool {
    let message = message.trim().to_lowercase();
    matches!(
        message.trim_end_matches(['?', '.', '!']),
        "che ore sono" | "info sistema" | "postura di sicurezza" | "centro sicurezza"
    ) || [
        "conta le parole:",
        "valida il json:",
        "valida json:",
        "verifica il json:",
        "verifica json:",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
}

pub(crate) fn is_source_followup(message: &str) -> bool {
    let normalized = message
        .trim()
        .trim_end_matches(['?', '!', '.'])
        .to_lowercase();
    matches!(
        normalized.as_str(),
        "fonte"
            | "fonti"
            | "fonte della risposta"
            | "fonti della risposta"
            | "qual è la fonte"
            | "quali sono le fonti"
            | "mi dai la fonte"
            | "mi dai le fonti"
            | "link della fonte"
            | "link delle fonti"
            | "mostra le fonti"
            | "e le fonti"
    )
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
