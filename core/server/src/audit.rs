//! Registro di audit append-only in JSON Lines con catena SHA-256 (ADR-0002) e firma Ed25519 di ogni evento
//! (ADR-0030).
//!
//! Chi può scrivere il file potrebbe ricalcolare l'intera catena di hash; non può però produrre le firme senza
//! la chiave privata, che sta nell'archivio dei segreti (portachiavi su macOS e Windows). Le firme stanno in un
//! file accanto (`<registro>.sig.jsonl`), così il formato degli eventi resta quello del contratto.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use jarvis_contracts::{audit_chain_errors, audit_hash, validate, Contract, GENESIS_HASH};
use serde_json::{Map, Value};

/// Messaggio firmato: legato a stream, posizione e hash dell'evento.
fn signed_message(stream: &str, sequence: u64, hash: &str) -> String {
    format!("jarvis-audit/1|{stream}|{sequence}|{hash}")
}

pub struct AuditLog {
    signer: Option<ed25519_dalek::SigningKey>,
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
            signer: None,
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
        if let Some(key) = &self.signer {
            use ed25519_dalek::Signer as _;
            let hash = event["hash"].as_str().unwrap_or_default();
            let sig = key.sign(signed_message(&self.stream, self.next_sequence, hash).as_bytes());
            let mut sigs = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.signatures_path())?;
            writeln!(
                sigs,
                "{}",
                serde_json::json!({"sequence": self.next_sequence, "hash": hash, "signature": hex::encode(sig.to_bytes())})
            )?;
            sigs.sync_data()?;
        }
        self.next_sequence += 1;
        self.last_hash = event["hash"].as_str().unwrap_or_default().to_string();
        Ok(event)
    }

    /// Attiva la firma con la chiave privata (32 byte) custodita nell'archivio dei segreti.
    pub fn set_signer(&mut self, seed: &[u8; 32]) {
        self.signer = Some(ed25519_dalek::SigningKey::from_bytes(seed));
    }

    /// Chiave pubblica in esadecimale, da conservare fuori dal computer per verifiche indipendenti.
    pub fn public_key(&self) -> Option<String> {
        self.signer
            .as_ref()
            .map(|k| hex::encode(k.verifying_key().to_bytes()))
    }

    pub fn signatures_path(&self) -> PathBuf {
        let mut p = self.path.clone().into_os_string();
        p.push(".sig.jsonl");
        PathBuf::from(p)
    }

    /// Verifica le firme di tutti gli eventi con la chiave pubblica attiva.
    pub fn verify_signatures(&self) -> std::io::Result<Value> {
        let Some(key) = &self.signer else {
            return Ok(serde_json::json!({"enabled": false}));
        };
        verify_signatures(
            &self.read_all()?,
            &read_events(&self.signatures_path())?,
            &key.verifying_key(),
            &self.stream,
        )
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

/// Esito della verifica: eventi firmati, eventi precedenti all'attivazione (senza firma) ed errori.
/// Dopo il primo evento firmato ogni evento deve avere una firma valida sul proprio hash.
pub fn verify_signatures(
    events: &[Value],
    signatures: &[Value],
    key: &ed25519_dalek::VerifyingKey,
    stream: &str,
) -> std::io::Result<Value> {
    use std::collections::HashMap;
    let by_seq: HashMap<u64, &Value> = signatures
        .iter()
        .filter_map(|s| s["sequence"].as_u64().map(|n| (n, s)))
        .collect();
    let (mut signed, mut unsigned_before, mut errors) = (0u64, 0u64, Vec::<String>::new());
    let mut started = false;
    for e in events {
        let seq = e["sequence"].as_u64().unwrap_or(u64::MAX);
        let hash = e["hash"].as_str().unwrap_or_default();
        match by_seq.get(&seq) {
            None if !started => unsigned_before += 1,
            None => errors.push(format!("evento {seq} senza firma")),
            Some(s) => {
                started = true;
                let ok = s["hash"] == hash
                    && hex::decode(s["signature"].as_str().unwrap_or_default())
                        .ok()
                        .and_then(|b| <[u8; 64]>::try_from(b.as_slice()).ok())
                        .map(|b| ed25519_dalek::Signature::from_bytes(&b))
                        .is_some_and(|sig| {
                            key.verify_strict(signed_message(stream, seq, hash).as_bytes(), &sig)
                                .is_ok()
                        });
                if ok {
                    signed += 1;
                } else {
                    errors.push(format!("firma non valida per l'evento {seq}"));
                }
            }
        }
    }
    Ok(serde_json::json!({
        "enabled": true,
        "public_key": hex::encode(key.to_bytes()),
        "signed": signed,
        "unsigned_before_activation": unsigned_before,
        "valid": errors.is_empty(),
        "errors": errors.into_iter().take(20).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_detect_a_recomputed_chain() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-audit-sig-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core-security.jsonl");
        let mut log = AuditLog::open(&path, "core-security").unwrap();
        let event = |action: &str| {
            let mut m = Map::new();
            m.insert(
                "correlation_id".into(),
                uuid::Uuid::new_v4().to_string().into(),
            );
            m.insert(
                "actor".into(),
                serde_json::json!({"kind": "owner", "id": "owner"}),
            );
            m.insert("node".into(), "core-local".into());
            m.insert("action".into(), action.into());
            m.insert("outcome".into(), "succeeded".into());
            m.insert("effect".into(), "applied".into());
            m
        };
        log.append(event("prima.della.firma")).unwrap();
        log.set_signer(&[7u8; 32]);
        log.append(event("memory.forget")).unwrap();
        log.append(event("connector.revoke")).unwrap();
        let report = log.verify_signatures().unwrap();
        assert_eq!(
            (
                report["signed"].as_u64(),
                report["unsigned_before_activation"].as_u64(),
                report["valid"].as_bool()
            ),
            (Some(2), Some(1), Some(true)),
            "{report}"
        );

        // Un attaccante modifica un evento e ricalcola tutta la catena: gli hash tornano, le firme no.
        let mut events = log.read_all().unwrap();
        events[1]["action"] = "memory.list".into();
        let mut prev = GENESIS_HASH.to_string();
        for e in events.iter_mut() {
            e["prev_hash"] = prev.clone().into();
            e["hash"] = "0".repeat(64).into();
            e["hash"] = audit_hash(e).into();
            prev = e["hash"].as_str().unwrap().to_string();
        }
        assert!(
            audit_chain_errors(&events).is_empty(),
            "catena ricalcolata coerente"
        );
        let lines: String = events.iter().map(|e| format!("{e}\n")).collect();
        std::fs::write(&path, lines).unwrap();
        let report = log.verify_signatures().unwrap();
        assert_eq!(report["valid"], false, "{report}");
        assert!(report["errors"].to_string().contains("evento 1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
