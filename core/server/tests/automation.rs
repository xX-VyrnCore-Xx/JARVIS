//! Test dell'Automation Engine (RF-07): pianificazione, persistenza al riavvio, policy in background,
//! approvazione legata ai passi, cronologia delle esecuzioni. Il tempo è simulato con `run_due(now)`.

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use jarvis_contracts::{validate, Contract};
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

fn config(data: &Path) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18766,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    }
}

fn fresh() -> (Jarvis, PathBuf) {
    let data = std::env::temp_dir().join(format!("jarvis-auto-{}", uuid::Uuid::new_v4().simple()));
    (Jarvis::new(config(&data)).unwrap(), data)
}

fn interval_workflow(steps: Value) -> Value {
    json!({"name": "Prova", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": steps})
}

#[test]
fn chat_creation_requires_confirmation_and_records_approval() {
    let (mut j, data) = fresh();
    let out = j.chat("ogni minuto scrivi battito.txt: vivo").unwrap();
    assert_eq!(
        out.task["state"], "waiting_confirmation",
        "creare un'automazione richiede conferma"
    );
    assert!(j.workflows().unwrap().is_empty());
    let id = out.confirmation.as_ref().unwrap()["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = j.confirm(&id, true).unwrap();
    assert_eq!(done.task["state"], "succeeded");
    let wfs = j.workflows().unwrap();
    assert_eq!(wfs.len(), 1);
    assert_eq!(wfs[0]["approval"]["method"], "chat_confirmation");
    assert_eq!(
        wfs[0]["trigger"],
        json!({"kind": "interval", "every_seconds": 60})
    );

    // Nessuna esecuzione prima della scadenza; dopo 61 s il passo ad alto rischio è eseguito perché pre-approvato.
    assert!(j.run_due(Utc::now()).unwrap().is_empty());
    let runs = j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["status"], "succeeded");
    assert_eq!(runs[0]["steps"][0]["preapproved"], true);
    assert!(validate(Contract::WorkflowRun, &runs[0]).is_valid());
    assert_eq!(
        std::fs::read_to_string(data.join("workspace/battito.txt")).unwrap(),
        "vivo"
    );
}

#[test]
fn schedule_survives_restart_and_catches_up_once() {
    let (mut j, data) = fresh();
    j.create_workflow(&interval_workflow(
        json!([{"tool": "time.now", "arguments": {}}]),
    ))
    .unwrap();
    drop(j);
    let mut again = Jarvis::new(config(&data)).unwrap();
    assert_eq!(
        again.workflows().unwrap().len(),
        1,
        "il workflow sopravvive al riavvio"
    );
    // Core spento per 10 minuti: una sola esecuzione di recupero, poi si riprende dal presente.
    let runs = again.run_due(Utc::now() + Duration::minutes(10)).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["trigger"]["kind"], "catch_up");
    assert!(again
        .run_due(Utc::now() + Duration::minutes(10))
        .unwrap()
        .is_empty());
    let wf = &again.workflows().unwrap()[0];
    assert_eq!(wf["last_run"]["status"], "succeeded");
    let history = again.workflow_runs(wf["id"].as_str().unwrap(), 10).unwrap();
    assert_eq!(
        history.len(),
        1,
        "la cronologia delle esecuzioni è conservata"
    );
}

#[test]
fn policy_still_applies_in_background() {
    let (mut j, data) = fresh();
    // Gli argomenti sono validi per lo schema, ma il percorso esce dalla workspace: la policy nega anche se approvato.
    j.create_workflow(&interval_workflow(json!([
        {"tool": "fs.write", "arguments": {"path": "../fuori.txt", "content": "x"}},
        {"tool": "time.now", "arguments": {}}
    ])))
    .unwrap();
    let runs = j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert_eq!(runs[0]["status"], "failed");
    assert_eq!(runs[0]["steps"][0]["status"], "denied");
    assert_eq!(
        runs[0]["steps"][0]["error_code"],
        "policy.path_outside_workspace"
    );
    assert_eq!(
        runs[0]["steps"][1]["status"], "skipped",
        "on_failure=stop salta i passi successivi"
    );
    assert!(!data.join("fuori.txt").exists());
}

#[test]
fn continue_on_failure_gives_partial_run() {
    let (mut j, _) = fresh();
    let mut wf = interval_workflow(json!([
        {"tool": "fs.read", "arguments": {"path": "manca.txt"}},
        {"tool": "time.now", "arguments": {}}
    ]));
    wf["on_failure"] = "continue".into();
    j.create_workflow(&wf).unwrap();
    let runs = j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert_eq!(runs[0]["status"], "partial");
    assert_eq!(runs[0]["steps"][0]["error_code"], "fs.not_found");
    assert_eq!(runs[0]["steps"][1]["status"], "succeeded");
}

#[test]
fn tampered_steps_are_not_executed_and_workflow_is_suspended() {
    let (mut j, data) = fresh();
    let created = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    j.tamper_workflow_for_tests(&id, json!([{"tool": "fs.write", "version": "1.0.0", "arguments": {"path": "x.txt", "content": "intruso"}}]))
        .unwrap();
    let runs = j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert_eq!(runs[0]["status"], "failed");
    assert_eq!(runs[0]["error"]["code"], "security.approval_mismatch");
    assert!(!data.join("workspace/x.txt").exists());
    assert_eq!(j.workflows().unwrap()[0]["enabled"], false);
    assert!(j
        .run_due(Utc::now() + Duration::seconds(200))
        .unwrap()
        .is_empty());
}

#[test]
fn once_trigger_runs_once_then_disables() {
    let (mut j, _) = fresh();
    let at =
        (Utc::now() + Duration::seconds(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    j.create_workflow(&json!({"name": "Una volta", "trigger": {"kind": "once", "at": at}, "steps": [{"tool": "time.now", "arguments": {}}]}))
        .unwrap();
    assert_eq!(
        j.run_due(Utc::now() + Duration::seconds(31)).unwrap().len(),
        1
    );
    assert!(j
        .run_due(Utc::now() + Duration::days(2))
        .unwrap()
        .is_empty());
    assert_eq!(j.workflows().unwrap()[0]["enabled"], false);
}

#[test]
fn disabled_workflows_do_not_run_and_can_be_resumed() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    j.set_workflow_enabled(&id, false).unwrap();
    assert!(j
        .run_due(Utc::now() + Duration::minutes(5))
        .unwrap()
        .is_empty());
    j.set_workflow_enabled(&id, true).unwrap();
    assert_eq!(
        j.run_due(Utc::now() + Duration::seconds(61)).unwrap().len(),
        1
    );
}

#[test]
fn invalid_workflows_are_rejected() {
    let (mut j, _) = fresh();
    let cases = [
        json!({"name": "x", "trigger": {"kind": "interval", "every_seconds": 5}, "steps": [{"tool": "time.now", "arguments": {}}]}),
        json!({"name": "x", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": [{"tool": "workflow.delete", "arguments": {"id": "abcd"}}]}),
        json!({"name": "x", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": [{"tool": "fs.read", "arguments": {}}]}),
        json!({"name": "x", "trigger": {"kind": "once", "at": "2020-01-01T00:00:00Z"}, "steps": [{"tool": "time.now", "arguments": {}}]}),
        json!({"name": "x", "trigger": {"kind": "interval", "every_seconds": 60}, "steps": []}),
    ];
    for case in cases {
        assert!(
            j.create_workflow(&case).is_err(),
            "doveva essere rifiutato: {case}"
        );
    }
    assert!(j.workflows().unwrap().is_empty());
}

#[test]
fn chat_lists_runs_and_deletes_with_confirmation() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "system.info", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let list = j.chat("elenca automazioni").unwrap();
    assert!(
        list.reply.as_ref().unwrap().contains(&id),
        "{:?}",
        list.reply
    );
    let run = j.chat(&format!("esegui automazione {id}")).unwrap();
    assert_eq!(run.task["state"], "succeeded");
    assert_eq!(
        j.workflow_runs(&id, 10).unwrap()[0]["trigger"]["kind"],
        "manual"
    );
    let del = j.chat(&format!("elimina automazione {id}")).unwrap();
    assert_eq!(del.task["state"], "waiting_confirmation");
    let cid = del.confirmation.as_ref().unwrap()["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    j.confirm(&cid, true).unwrap();
    assert!(j.workflows().unwrap().is_empty());
}

#[test]
fn every_run_is_audited_with_intact_chain() {
    let (mut j, _) = fresh();
    j.create_workflow(&interval_workflow(
        json!([{"tool": "time.now", "arguments": {}}]),
    ))
    .unwrap();
    j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    let audit = j.audit_log(100).unwrap();
    assert_eq!(audit["chain_valid"], true);
    let actors: Vec<String> = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}:{}",
                e["actor"]["id"].as_str().unwrap(),
                e["outcome"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        actors.contains(&"automation:allowed".to_string()),
        "{actors:?}"
    );
    assert!(
        actors.contains(&"owner:succeeded".to_string()),
        "creazione dal proprietario registrata: {actors:?}"
    );
}

// ---------------------------------------------------------------------- modifica (versioni)

#[test]
fn chat_edit_requires_confirmation_and_creates_a_new_approved_version() {
    let (mut j, data) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    let old_digest = j.workflows().unwrap()[0]["approval"]["steps_digest"].clone();

    let out = j
        .chat(&format!(
            "modifica automazione {id}: scrivi stato.txt: aggiornato"
        ))
        .unwrap();
    assert_eq!(
        out.task["state"], "waiting_confirmation",
        "una modifica richiede conferma"
    );
    assert_eq!(
        j.workflows().unwrap()[0]["version"],
        1,
        "nessun effetto prima della conferma"
    );
    let cid = out.confirmation.as_ref().unwrap()["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = j.confirm(&cid, true).unwrap();
    assert!(
        done.reply.as_ref().unwrap().contains("versione 2"),
        "{:?}",
        done.reply
    );

    let wf = &j.workflows().unwrap()[0];
    assert_eq!(wf["version"], 2);
    assert_eq!(wf["approval"]["method"], "chat_confirmation");
    assert_ne!(
        wf["approval"]["steps_digest"], old_digest,
        "nuova approvazione legata ai nuovi passi"
    );
    // Il nuovo passo ad alto rischio è eseguito perché coperto dalla nuova approvazione.
    let runs = j.run_due(Utc::now() + Duration::seconds(125)).unwrap();
    assert_eq!(runs[0]["status"], "succeeded");
    assert_eq!(runs[0]["workflow_version"], 2);
    assert_eq!(
        std::fs::read_to_string(data.join("workspace/stato.txt")).unwrap(),
        "aggiornato"
    );
    // Cronologia: la prima esecuzione resta associata alla versione 1.
    let history = j.workflow_runs(&id, 10).unwrap();
    assert_eq!(
        history
            .iter()
            .map(|r| r["workflow_version"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [2, 1]
    );
    let versions = j.workflow_versions(&id).unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|v| v["version"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(
        versions[1]["steps"][0]["tool"], "time.now",
        "la versione precedente è conservata intatta"
    );
}

#[test]
fn trigger_change_reschedules_from_now() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = j
        .update_workflow(
            &id,
            &json!({"trigger": {"kind": "interval", "every_seconds": 7200}}),
        )
        .unwrap();
    assert_eq!(out["changes"][0], "frequenza: ogni minuto → ogni 2 ore");
    assert!(
        j.run_due(Utc::now() + Duration::seconds(61))
            .unwrap()
            .is_empty(),
        "la vecchia scadenza non vale più"
    );
    assert_eq!(
        j.run_due(Utc::now() + Duration::seconds(7201))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rename_keeps_schedule_and_is_audited() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let before = j.workflows().unwrap()[0]["next_run_at"].clone();
    j.update_workflow(
        &id,
        &json!({"name": "Ora esatta", "on_failure": "continue"}),
    )
    .unwrap();
    let wf = &j.workflows().unwrap()[0];
    assert_eq!(
        (
            wf["name"].as_str().unwrap(),
            wf["on_failure"].as_str().unwrap(),
            wf["version"].as_u64().unwrap()
        ),
        ("Ora esatta", "continue", 2)
    );
    assert_eq!(
        wf["next_run_at"], before,
        "senza cambio di frequenza la prossima esecuzione resta invariata"
    );
    assert_eq!(wf["approval"]["method"], "owner_ui");
    let audit = j.audit_log(5).unwrap();
    assert!(audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "workflow.update" && e["actor"]["kind"] == "owner"));
}

#[test]
fn invalid_or_empty_edits_are_rejected_without_changes() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    for body in [
        json!({}),
        json!({"name": "Prova"}),
        json!({"trigger": {"kind": "interval", "every_seconds": 5}}),
        json!({"steps": [{"tool": "workflow.delete", "arguments": {"id": "abcd"}}]}),
        json!({"steps": [{"tool": "fs.read", "arguments": {}}]}),
        json!({"trigger": {"kind": "once", "at": "2020-01-01T00:00:00Z"}}),
    ] {
        assert!(
            j.update_workflow(&id, &body).is_err(),
            "doveva essere rifiutata: {body}"
        );
    }
    assert_eq!(j.workflows().unwrap()[0]["version"], 1);
    assert!(j
        .update_workflow("ffffffff", &json!({"name": "x"}))
        .is_err());
}

#[test]
fn editing_a_suspended_workflow_keeps_it_suspended() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    j.set_workflow_enabled(&id, false).unwrap();
    j.update_workflow(
        &id,
        &json!({"trigger": {"kind": "interval", "every_seconds": 120}}),
    )
    .unwrap();
    let wf = &j.workflows().unwrap()[0];
    assert_eq!(wf["enabled"], false);
    assert!(wf["next_run_at"].is_null());
    assert!(j
        .run_due(Utc::now() + Duration::minutes(10))
        .unwrap()
        .is_empty());
}

#[test]
fn tampering_after_an_edit_is_still_detected() {
    let (mut j, _) = fresh();
    let id = j
        .create_workflow(&interval_workflow(
            json!([{"tool": "time.now", "arguments": {}}]),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    j.update_workflow(
        &id,
        &json!({"steps": [{"tool": "system.info", "arguments": {}}]}),
    )
    .unwrap();
    j.tamper_workflow_for_tests(&id, json!([{"tool": "fs.write", "version": "1.0.0", "arguments": {"path": "x", "content": "y"}}])).unwrap();
    let runs = j.run_due(Utc::now() + Duration::seconds(61)).unwrap();
    assert_eq!(runs[0]["error"]["code"], "security.approval_mismatch");
}
