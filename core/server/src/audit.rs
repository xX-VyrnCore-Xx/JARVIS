//! Registro di audit append-only in JSON Lines con catena SHA-256 (ADR-0002).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use jarvis_contracts::{audit_chain_errors, audit_hash, validate, Contract, GENESIS_HASH};
use serde_json::{Map, Value};

pub struct AuditLog {
    path: PathBuf,
    stream: String,
    next_sequence: u64,
    last_hash: String,
}

impl AuditLog {
    pub fn open(path: &Path, stream: &str) -> std::io::Result<Self> {
        let events = read_events(path)?;
        let last_hash = events
            .last()
            .and_then(|e| e["hash"].as_str())
            .unwrap_or(GENESIS_HASH)
            .to_string();
        Ok(Self {
            path: path.to_path_buf(),
            stream: stream.into(),
            next_sequence: events.len() as u64,
            last_hash,
        })
    }

    /// Aggiunge un evento: `fields` contiene i campi specifici (actor, action, outcome, ...).
    pub fn append(&mut self, fields: Map<String, Value>) -> std::io::Result<Value> {
        let mut event = fields;
        event.insert("contract".into(), "jarvis.audit-event/1".into());
        event.insert("event_id".into(), uuid::Uuid::new_v4().to_string().into());
        event.insert("sequence".into(), self.next_sequence.into());
        event.insert("stream".into(), self.stream.clone().into());
        event.insert("at".into(), crate::now().into());
        event.insert("prev_hash".into(), self.last_hash.clone().into());
        event.insert("hash".into(), "0".repeat(64).into());
        let mut event = Value::Object(event);
        event["hash"] = audit_hash(&event).into();
        let report = validate(Contract::AuditEvent, &event);
        if !report.is_valid() {
            return Err(std::io::Error::other(format!(
                "evento di audit non conforme: {report:?}"
            )));
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{event}")?;
        file.sync_data()?;
        self.next_sequence += 1;
        self.last_hash = event["hash"].as_str().unwrap_or_default().to_string();
        Ok(event)
    }

    pub fn read_all(&self) -> std::io::Result<Vec<Value>> {
        read_events(&self.path)
    }

    pub fn verify(&self) -> std::io::Result<Vec<String>> {
        Ok(audit_chain_errors(&self.read_all()?))
    }
}

fn read_events(path: &Path) -> std::io::Result<Vec<Value>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    BufReader::new(file)
        .lines()
        .filter(|l| l.as_ref().map(|l| !l.trim().is_empty()).unwrap_or(true))
        .map(|l| l.and_then(|l| serde_json::from_str(&l).map_err(std::io::Error::other)))
        .collect()
}
