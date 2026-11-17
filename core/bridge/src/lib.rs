//! Prototipo di bridge fra Core Rust e worker Python (Fase 2, RNF-01, RNF-02).
//!
//! Trasporto: JSON Lines su stdio, un `jarvis.envelope/1` per riga. Il Core
//! valida in uscita e in ingresso con `jarvis-contracts`, controlla la
//! correlazione della risposta, applica il timeout della delega e, in caso di
//! timeout o arresto del worker, termina il processo e riporta un errore con
//! effetto `unknown` (AUTONOMY_AND_TOOLS.md: verificare lo stato prima di
//! ripetere). Il trasporto è una prova tecnica, non la scelta RPC definitiva.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use jarvis_contracts::{validate, Contract};
use serde_json::{json, Value};
use uuid::Uuid;

/// Errore del bridge, convertibile nel contratto `error` v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    pub code: &'static str,
    pub category: &'static str,
    pub message: String,
    pub retryable: bool,
    pub effect: &'static str,
}

impl BridgeError {
    fn new(
        code: &'static str,
        category: &'static str,
        message: impl Into<String>,
        retryable: bool,
        effect: &'static str,
    ) -> Self {
        Self {
            code,
            category,
            message: message.into(),
            retryable,
            effect,
        }
    }

    /// Rappresentazione conforme a `contracts/v1/error.schema.json`.
    pub fn to_contract(&self) -> Value {
        json!({
            "code": self.code,
            "category": self.category,
            "message": self.message,
            "retryable": self.retryable,
            "effect": self.effect,
        })
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.code, self.category, self.message)
    }
}

impl std::error::Error for BridgeError {}

/// Processo worker collegato su stdio.
pub struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<std::io::Result<String>>,
    sender: Value,
}

impl Worker {
    /// Avvia il worker. `command` deve parlare il protocollo JSON Lines descritto nel modulo.
    pub fn spawn(mut command: Command) -> Result<Self, BridgeError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                BridgeError::new(
                    "bridge.spawn_failed",
                    "unavailable",
                    format!("avvio del worker non riuscito: {e}"),
                    true,
                    "not_applied",
                )
            })?;
        let stdout = child.stdout.take().expect("stdout in pipe");
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines: rx,
            sender: json!({"kind": "module", "id": "core"}),
        })
    }

    /// Invia una delega e attende la risposta entro `timeout_ms` della delega.
    ///
    /// Restituisce l'envelope di risposta validato; `status` può essere
    /// `succeeded` o `failed` (errore riportato dall'agente). Gli errori di
    /// trasporto, contratto o timeout sono `BridgeError`.
    pub fn delegate(
        &mut self,
        task_id: &str,
        correlation_id: &str,
        delegation: &Value,
    ) -> Result<Value, BridgeError> {
        let report = validate(Contract::Delegation, delegation);
        if !report.is_valid() {
            return Err(BridgeError::new(
                "bridge.invalid_delegation",
                "validation",
                format!("{report:?}"),
                false,
                "not_applied",
            ));
        }
        let recipient = json!({"kind": "agent", "id": delegation["delegate"]["agent"]});
        let timeout = Duration::from_millis(delegation["timeout_ms"].as_u64().unwrap_or(1));
        self.request(
            task_id,
            correlation_id,
            recipient,
            "delegation.request",
            delegation,
            timeout,
        )
    }

    /// Richiesta generica: costruisce e valida l'envelope, attende una risposta correlata entro `timeout`.
    pub fn request(
        &mut self,
        task_id: &str,
        correlation_id: &str,
        recipient: Value,
        message_type: &str,
        payload: &Value,
        timeout: Duration,
    ) -> Result<Value, BridgeError> {
        let request = json!({
            "contract": "jarvis.envelope/1",
            "message_id": Uuid::new_v4().to_string(),
            "correlation_id": correlation_id,
            "task_id": task_id,
            "sender": self.sender,
            "recipient": recipient,
            "type": message_type,
            "status": "proposed",
            "issued_at": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            "payload": payload,
        });
        let report = validate(Contract::Envelope, &request);
        if !report.is_valid() {
            return Err(BridgeError::new(
                "bridge.invalid_envelope",
                "validation",
                format!("{report:?}"),
                false,
                "not_applied",
            ));
        }

        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Self::closed("canale di ingresso già chiuso", "not_applied"))?;
        let mut line = serde_json::to_string(&request).expect("serializzazione");
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|_| Self::closed("scrittura verso il worker non riuscita", "unknown"))?;

        let raw = match self.lines.recv_timeout(timeout) {
            Ok(Ok(raw)) => raw,
            Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => {
                self.terminate();
                return Err(Self::closed(
                    "il worker si è chiuso senza risposta",
                    "unknown",
                ));
            }
            Err(RecvTimeoutError::Timeout) => {
                self.terminate();
                return Err(BridgeError::new(
                    "bridge.timeout",
                    "timeout",
                    format!(
                        "nessuna risposta entro {} ms; worker terminato",
                        timeout.as_millis()
                    ),
                    false,
                    "unknown",
                ));
            }
        };

        let response: Value = serde_json::from_str(&raw).map_err(|_| {
            BridgeError::new(
                "bridge.invalid_response",
                "internal",
                "risposta non JSON",
                false,
                "unknown",
            )
        })?;
        let report = validate(Contract::Envelope, &response);
        if !report.is_valid() {
            return Err(BridgeError::new(
                "bridge.invalid_response",
                "internal",
                format!("risposta non conforme: {report:?}"),
                false,
                "unknown",
            ));
        }
        if response["causation_id"] != request["message_id"]
            || response["correlation_id"] != request["correlation_id"]
            || response["task_id"] != request["task_id"]
        {
            return Err(BridgeError::new(
                "bridge.uncorrelated_response",
                "internal",
                "risposta non correlata alla richiesta",
                false,
                "unknown",
            ));
        }
        Ok(response)
    }

    fn closed(message: &str, effect: &'static str) -> BridgeError {
        BridgeError::new(
            "bridge.worker_unavailable",
            "unavailable",
            message,
            false,
            effect,
        )
    }

    /// Chiude stdin, attende l'uscita del worker e ne restituisce il codice.
    pub fn shutdown(mut self) -> Option<i32> {
        self.stdin.take();
        self.child.wait().ok().and_then(|s| s.code())
    }

    fn terminate(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Solo per i test: termina il processo come farebbe un crash esterno.
    #[doc(hidden)]
    pub fn kill_for_tests(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Vero se il processo del worker è terminato.
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if !self.has_exited() {
            self.terminate();
        }
    }
}
