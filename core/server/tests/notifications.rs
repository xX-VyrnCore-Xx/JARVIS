//! Test di RF-13: «un guasto simulato è rilevabile e correlabile al task».

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use jarvis_contracts::{validate, Contract};
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

fn config(data: &Path, python: &str) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18767,
        data_dir: data.to_path_buf(),
        python: python.into(),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    }
}

fn fresh_with(python: &str) -> (Jarvis, PathBuf) {
    let data = std::env::temp_dir().join(format!("jarvis-notif-{}", uuid::Uuid::new_v4().simple()));
    (Jarvis::new(config(&data, python)).unwrap(), data)
}

fn fresh() -> (Jarvis, PathBuf) {
    fresh_with(&std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()))
}

fn items(j: &Jarvis) -> Vec<Value> {
    let all = j.notifications(100, false, None).unwrap();
    let list = all["items"].as_array().unwrap().clone();
    for n in &list {
        assert!(
            validate(Contract::Notification, n).is_valid(),
            "notifica non conforme: {n}"
        );
    }
    list
}

fn tasks(j: &Jarvis) -> Vec<Value> {
    j.tasks(100).unwrap()
}

#[test]
fn unavailable_agent_is_notified_and_correlated_to_the_failed_task() {
    let (mut j, _) = fresh_with("/percorso/inesistente/python");
    let out = j.chat("ciao").unwrap();
    assert_eq!(out.task["state"], "failed");
    let n = items(&j);
    let alert = n
        .iter()
        .find(|n| n["title"] == "Agente AI non disponibile")
        .expect("notifica del guasto");
    assert_eq!(alert["severity"], "error");
    assert_eq!(alert["task_id"], out.task["task_id"], "correlata al task");
    assert_eq!(alert["correlation_id"], out.task["correlation_id"]);
    // Il guasto ripetuto non genera una seconda notifica finché la prima non è letta.
    j.chat("ciao di nuovo").unwrap();
    assert_eq!(
        items(&j)
            .iter()
            .filter(|n| n["title"] == "Agente AI non disponibile")
            .count(),
        1
    );
    assert_eq!(
        tasks(&j).iter().filter(|t| t["state"] == "failed").count(),
        2
    );
}

#[test]
fn failing_automation_is_notified_once_with_task_and_workflow() {
    let (mut j, _) = fresh();
    let wf = j
        .create_workflow(&json!({"name": "Lettura mancante", "trigger": {"kind": "interval", "every_seconds": 60},
                                 "steps": [{"tool": "fs.read", "arguments": {"path": "manca.txt"}}]}))
        .unwrap();
    let run = &j.run_due(Utc::now() + Duration::seconds(61)).unwrap()[0];
    assert_eq!(run["status"], "failed");
    let n = items(&j);
    let alert = n
        .iter()
        .find(|n| n["category"] == "automation")
        .expect("notifica dell'automazione");
    assert_eq!(alert["severity"], "error");
    assert_eq!(alert["workflow_id"], wf["workflow_id"]);
    assert_eq!(
        alert["task_id"], run["task_id"],
        "correlata al task dell'esecuzione"
    );
    assert!(alert["body"].as_str().unwrap().contains("fs.not_found"));
    // Seconda esecuzione fallita: nessuna nuova notifica finché la prima è non letta.
    j.run_due(Utc::now() + Duration::seconds(130)).unwrap();
    assert_eq!(
        items(&j)
            .iter()
            .filter(|n| n["category"] == "automation")
            .count(),
        1
    );
    // Letta la prima, il guasto successivo torna a essere notificato.
    j.mark_notification_read(alert["notification_id"].as_str().unwrap())
        .unwrap();
    j.run_due(Utc::now() + Duration::seconds(200)).unwrap();
    assert_eq!(
        items(&j)
            .iter()
            .filter(|n| n["category"] == "automation")
            .count(),
        2
    );
}

#[test]
fn successful_interval_runs_are_silent_but_once_and_manual_are_reported() {
    let (mut j, _) = fresh();
    let wf = j
        .create_workflow(&json!({"name": "Ora", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": [{"tool": "time.now", "arguments": {}}]}))
        .unwrap();
    j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert!(
        items(&j).iter().all(|n| n["category"] != "automation"),
        "nessun rumore per i successi periodici"
    );
    j.run_workflow_now(wf["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        items(&j)
            .iter()
            .filter(|n| n["severity"] == "success")
            .count(),
        1
    );
}

#[test]
fn tampered_automation_raises_a_security_notification() {
    let (mut j, _) = fresh();
    let wf = j
        .create_workflow(&json!({"name": "Ora", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": [{"tool": "time.now", "arguments": {}}]}))
        .unwrap();
    j.tamper_workflow_for_tests(
        wf["id"].as_str().unwrap(),
        json!([{"tool": "system.info", "version": "1.0.0", "arguments": {}}]),
    )
    .unwrap();
    j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    let n = items(&j);
    let alert = n
        .iter()
        .find(|n| n["category"] == "security")
        .expect("notifica di sicurezza");
    assert!(alert["title"]
        .as_str()
        .unwrap()
        .starts_with("Automazione sospesa"));
}

#[test]
fn pending_confirmation_is_notified_and_cleared_when_resolved() {
    let (mut j, _) = fresh();
    let out = j.chat("scrivi a.txt: x").unwrap();
    let unread = j.notifications(50, true, None).unwrap();
    assert_eq!(unread["unread"], 1);
    assert_eq!(unread["items"][0]["category"], "confirmation");
    assert_eq!(unread["items"][0]["task_id"], out.task["task_id"]);
    let id = out.confirmation.as_ref().unwrap()["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    j.confirm(&id, false).unwrap();
    assert_eq!(
        j.notifications(50, true, None).unwrap()["unread"],
        0,
        "risolta la conferma, la notifica si chiude"
    );
}

#[test]
fn monitor_detects_audit_tampering_and_worker_crash() {
    let (mut j, data) = fresh();
    j.chat("che ore sono").unwrap();
    // Crash del worker fuori dalle richieste.
    j.kill_worker_for_tests();
    j.monitor().unwrap();
    assert!(items(&j)
        .iter()
        .any(|n| n["title"] == "Worker AI terminato"));
    // Manomissione dell'audit.
    let path = data.join("audit/core-security.jsonl");
    let tampered =
        std::fs::read_to_string(&path)
            .unwrap()
            .replacen("\"succeeded\"", "\"failed\"", 1);
    std::fs::write(&path, tampered).unwrap();
    j.monitor().unwrap();
    j.monitor().unwrap();
    let security: Vec<Value> = items(&j)
        .into_iter()
        .filter(|n| n["category"] == "security")
        .collect();
    assert_eq!(
        security.len(),
        1,
        "una sola notifica finché non viene letta"
    );
    assert_eq!(security[0]["link"]["view"], "audit");
    // Il sistema resta utilizzabile: il worker viene riavviato alla richiesta successiva.
    assert_eq!(j.chat("ciao").unwrap().task["state"], "succeeded");
}

#[test]
fn read_all_and_since_filter() {
    let (mut j, _) = fresh();
    j.chat("scrivi a.txt: x").unwrap();
    let first = items(&j)[0]["created_at"].as_str().unwrap().to_string();
    std::thread::sleep(std::time::Duration::from_millis(5));
    j.chat("scrivi b.txt: y").unwrap();
    let newer = j.notifications(50, false, Some(&first)).unwrap();
    assert_eq!(newer["items"].as_array().unwrap().len(), 1);
    assert_eq!(j.mark_all_notifications_read().unwrap()["marked"], 2);
    assert_eq!(j.notifications(50, true, None).unwrap()["unread"], 0);
    assert!(j.mark_notification_read("inesistente").is_err());
}
