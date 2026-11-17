//! Contratti v1 di JARVIS per il Core Rust.
//!
//! Gli schemi sono incorporati a compile time da `contracts/v1/`: il binario e
//! la versione dei contratti coincidono. Gli invarianti semantici replicano
//! `semantic_errors` di `tests/contracts/test_contracts_v1.py`; i test
//! d'integrazione usano le stesse fixture per provare la compatibilità
//! Rust–Python (RNF-01).

use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use jsonschema::{Resource, Validator};
use serde_json::Value;
use sha2::{Digest, Sha256};

const BASE: &str = "https://jarvis.local/contracts/v1/";

macro_rules! schema_file {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/v1/",
            $name
        ))
    };
}

const COMMON: &str = schema_file!("common.schema.json");
const ERROR: &str = schema_file!("error.schema.json");
const TRANSITIONS: &str = schema_file!("task-transitions.json");

/// Hash di partenza della catena di audit.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Contratti v1 validabili.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contract {
    AuditEvent,
    Delegation,
    Envelope,
    Error,
    PolicyDecision,
    Task,
    ToolDescriptor,
}

impl Contract {
    pub const ALL: [Contract; 7] = [
        Contract::AuditEvent,
        Contract::Delegation,
        Contract::Envelope,
        Contract::Error,
        Contract::PolicyDecision,
        Contract::Task,
        Contract::ToolDescriptor,
    ];

    /// Nome usato nei file `<nome>.schema.json` e nelle fixture.
    pub fn name(self) -> &'static str {
        match self {
            Contract::AuditEvent => "audit-event",
            Contract::Delegation => "delegation",
            Contract::Envelope => "envelope",
            Contract::Error => "error",
            Contract::PolicyDecision => "policy-decision",
            Contract::Task => "task",
            Contract::ToolDescriptor => "tool-descriptor",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    fn source(self) -> &'static str {
        match self {
            Contract::AuditEvent => schema_file!("audit-event.schema.json"),
            Contract::Delegation => schema_file!("delegation.schema.json"),
            Contract::Envelope => schema_file!("envelope.schema.json"),
            Contract::Error => ERROR,
            Contract::PolicyDecision => schema_file!("policy-decision.schema.json"),
            Contract::Task => schema_file!("task.schema.json"),
            Contract::ToolDescriptor => schema_file!("tool-descriptor.schema.json"),
        }
    }

    fn validator(self) -> &'static Validator {
        static CELLS: [OnceLock<Validator>; 7] = [const { OnceLock::new() }; 7];
        let index = Self::ALL
            .iter()
            .position(|c| *c == self)
            .expect("contratto noto");
        CELLS[index].get_or_init(|| build_validator(self))
    }
}

fn parse(source: &str) -> Value {
    serde_json::from_str(source).expect("schema incorporato non valido")
}

fn build_validator(contract: Contract) -> Validator {
    let resources = [("common.schema.json", COMMON), ("error.schema.json", ERROR)]
        .into_iter()
        .map(|(file, src)| {
            let resource = Resource::from_contents(parse(src)).expect("risorsa dello schema");
            (format!("{BASE}{file}"), resource)
        });
    jsonschema::options()
        .with_resources(resources)
        .build(&parse(contract.source()))
        .expect("schema compilabile")
}

/// Esito della validazione: errori di schema e violazioni semantiche separati.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub schema: Vec<String>,
    pub semantic: Vec<String>,
}

impl Report {
    pub fn is_valid(&self) -> bool {
        self.schema.is_empty() && self.semantic.is_empty()
    }
}

/// Valida un documento. Gli invarianti semantici sono controllati solo se lo schema è rispettato.
pub fn validate(contract: Contract, doc: &Value) -> Report {
    let schema: Vec<String> = contract
        .validator()
        .iter_errors(doc)
        .map(|e| format!("{}: {}", e.instance_path, e))
        .collect();
    let semantic = if schema.is_empty() {
        semantic_errors(contract, doc)
    } else {
        Vec::new()
    };
    Report { schema, semantic }
}

fn ts(value: &Value) -> DateTime<Utc> {
    let raw = value.as_str().expect("timestamp già validato dallo schema");
    DateTime::parse_from_rfc3339(raw)
        .expect("timestamp RFC 3339")
        .with_timezone(&Utc)
}

fn transitions() -> &'static Value {
    static TABLE: OnceLock<Value> = OnceLock::new();
    TABLE.get_or_init(|| parse(TRANSITIONS))
}

/// Invarianti non esprimibili in JSON Schema (contracts/README.md). Presuppone un documento valido per lo schema.
pub fn semantic_errors(contract: Contract, doc: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    match contract {
        Contract::Delegation => {
            let (issued, expires) = (ts(&doc["issued_at"]), ts(&doc["expires_at"]));
            let lifetime_ms = (expires - issued).num_milliseconds();
            if lifetime_ms <= 0 {
                errors.push("expires_at deve seguire issued_at".into());
            } else if doc["timeout_ms"].as_i64().unwrap_or(0) > lifetime_ms {
                errors.push("timeout_ms supera la durata della delega".into());
            }
        }
        Contract::Envelope => {
            if doc["sender"] == doc["recipient"] {
                errors.push("sender e recipient coincidono".into());
            }
            if doc
                .get("causation_id")
                .is_some_and(|c| *c == doc["message_id"])
            {
                errors.push("causation_id non può riferirsi al messaggio stesso".into());
            }
        }
        Contract::Task => task_errors(doc, &mut errors),
        Contract::PolicyDecision => {
            let (decided, expires) = (ts(&doc["decided_at"]), ts(&doc["expires_at"]));
            if expires <= decided {
                errors.push("expires_at deve seguire decided_at".into());
            }
            if let Some(conf) = doc.get("confirmation") {
                if conf["parameters_digest"] != doc["request"]["parameters_digest"] {
                    errors.push("la conferma è legata a parametri diversi".into());
                }
                let conf_expires = ts(&conf["expires_at"]);
                if !(decided < conf_expires && conf_expires <= expires) {
                    errors.push("la conferma deve scadere entro la decisione".into());
                }
            }
        }
        Contract::AuditEvent | Contract::Error | Contract::ToolDescriptor => {}
    }
    errors
}

fn task_errors(doc: &Value, errors: &mut Vec<String>) {
    let table = transitions();
    let history = doc["history"].as_array().expect("history validata");
    let first = &history[0];
    if first.get("from").is_some() || first["to"] != table["initial"] {
        errors.push("la history deve iniziare con lo stato iniziale senza 'from'".into());
    }
    for pair in history.windows(2) {
        let (prev, step) = (&pair[0], &pair[1]);
        let prev_to = prev["to"].as_str().unwrap_or_default();
        if step.get("from") != Some(&prev["to"]) {
            errors.push(format!(
                "'from' {} non segue {prev_to}",
                step.get("from").unwrap_or(&Value::Null)
            ));
        } else if !table["transitions"][prev_to]
            .as_array()
            .is_some_and(|t| t.contains(&step["to"]))
        {
            errors.push(format!(
                "transizione non ammessa {prev_to} -> {}",
                step["to"].as_str().unwrap_or_default()
            ));
        }
    }
    let mut previous = ts(&doc["created_at"]);
    for step in history {
        let at = ts(&step["at"]);
        if at < previous {
            errors.push("istanti della history non monotoni".into());
        }
        previous = at;
    }
    if history.last().map(|s| &s["to"]) != Some(&doc["state"]) {
        errors.push("state diverso dall'ultima transizione".into());
    }
}

/// SHA-256 del JSON canonico (chiavi ordinate, nessuno spazio, UTF-8) senza il campo `hash`.
pub fn audit_hash(event: &Value) -> String {
    let mut body = event.as_object().cloned().unwrap_or_default();
    body.remove("hash");
    // serde_json::Map senza `preserve_order` è un BTreeMap: chiavi già ordinate.
    let canonical = serde_json::to_string(&Value::Object(body)).expect("serializzazione");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// Verifica sequenza e catena di hash di un flusso di eventi di audit.
pub fn audit_chain_errors(events: &[Value]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut prev_hash = GENESIS_HASH.to_string();
    for (index, event) in events.iter().enumerate() {
        if event["sequence"].as_u64() != Some(index as u64) {
            errors.push(format!("sequenza interrotta all'evento {index}"));
        }
        if event["prev_hash"].as_str() != Some(prev_hash.as_str()) {
            errors.push(format!("prev_hash errato all'evento {index}"));
        }
        if event["hash"].as_str() != Some(audit_hash(event).as_str()) {
            errors.push(format!("hash non corrispondente all'evento {index}"));
        }
        prev_hash = event["hash"].as_str().unwrap_or_default().to_string();
    }
    errors
}
