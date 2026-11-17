//! Prova Rust–Python: il Core avvia il worker reale `ai/jarvis_ai/worker.py`.
//! Richiede `python3` con `jsonschema` (tests/contracts/requirements.txt).

use std::path::{Path, PathBuf};
use std::process::Command;

use jarvis_bridge::Worker;
use jarvis_contracts::{validate, Contract};
use serde_json::Value;

const TASK: &str = "0b6f4c1e-1d2a-4c3b-9e8f-0000000000a1";
const CORRELATION: &str = "0b6f4c1e-1d2a-4c3b-9e8f-0000000000c0";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn worker(handler: &str) -> Worker {
    let python = std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into());
    let mut cmd = Command::new(python);
    cmd.args(["-m", "jarvis_ai.worker", "--handler", handler])
        .current_dir(repo().join("ai"));
    Worker::spawn(cmd).expect("avvio del worker")
}

fn delegation(objective: &str, timeout_ms: u64) -> Value {
    let path = repo().join("tests/contracts/fixtures/valid/delegation.research.json");
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["objective"] = objective.into();
    doc["timeout_ms"] = timeout_ms.into();
    doc
}

#[test]
fn delegation_round_trip_is_valid_and_correlated() {
    let mut w = worker("echo");
    for i in 0..3 {
        let objective = format!("obiettivo sintetico {i}");
        let response = w
            .delegate(TASK, CORRELATION, &delegation(&objective, 5_000))
            .expect("risposta");
        assert!(validate(Contract::Envelope, &response).is_valid());
        assert_eq!(response["status"], "succeeded");
        assert_eq!(response["payload"]["objective"], objective.as_str());
        assert_eq!(response["recipient"]["id"], "core");
    }
    assert_eq!(w.shutdown(), Some(0));
}

#[test]
fn agent_failure_is_propagated_as_contract_error() {
    let mut w = worker("synthetic");
    let response = w
        .delegate(TASK, CORRELATION, &delegation("fail", 5_000))
        .expect("risposta");
    assert_eq!(response["status"], "failed");
    assert_eq!(response["error"]["code"], "agent.synthetic_failure");
    assert!(validate(Contract::Error, &response["error"]).is_valid());
    // Il worker resta disponibile dopo un errore dell'agente.
    let ok = w
        .delegate(TASK, CORRELATION, &delegation("dopo l'errore", 5_000))
        .expect("risposta");
    assert_eq!(ok["status"], "succeeded");
}

#[test]
fn timeout_kills_worker_and_reports_unknown_effect() {
    let mut w = worker("synthetic");
    let err = w
        .delegate(TASK, CORRELATION, &delegation("sleep:3000", 200))
        .unwrap_err();
    assert_eq!(
        (err.code, err.category, err.effect, err.retryable),
        ("bridge.timeout", "timeout", "unknown", false)
    );
    assert!(validate(Contract::Error, &err.to_contract()).is_valid());
    assert!(w.has_exited(), "il worker deve essere terminato");
    let again = w
        .delegate(TASK, CORRELATION, &delegation("dopo il timeout", 1_000))
        .unwrap_err();
    assert_eq!(again.code, "bridge.worker_unavailable");
}

#[test]
fn worker_crash_is_detected() {
    let mut w = worker("synthetic");
    let err = w
        .delegate(TASK, CORRELATION, &delegation("crash", 5_000))
        .unwrap_err();
    assert_eq!(
        (err.code, err.effect),
        ("bridge.worker_unavailable", "unknown")
    );
    assert!(validate(Contract::Error, &err.to_contract()).is_valid());
}

#[test]
fn malformed_response_is_rejected() {
    let mut w = worker("synthetic");
    let err = w
        .delegate(TASK, CORRELATION, &delegation("malformed", 5_000))
        .unwrap_err();
    assert_eq!(err.code, "bridge.invalid_response");
}

#[test]
fn invalid_delegation_is_rejected_before_sending() {
    let mut w = worker("echo");
    let mut bad = delegation("x", 5_000);
    bad["completion_conditions"] = Value::Array(vec![]);
    let err = w.delegate(TASK, CORRELATION, &bad).unwrap_err();
    assert_eq!(
        (err.code, err.effect),
        ("bridge.invalid_delegation", "not_applied")
    );
    assert_eq!(
        w.shutdown(),
        Some(0),
        "nessun messaggio doveva raggiungere il worker"
    );
}
