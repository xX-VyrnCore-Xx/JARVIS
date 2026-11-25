//! Pianificatore guidato dal modello (ADR-0160): la risposta del modello diventa una bozza validata dal Core,
//! mai un piano avviato; strumenti sconosciuti, credenziali e piani impossibili sono rifiutati.

use std::path::PathBuf;

use jarvis_core::cloud::{Answer, Failure, Provider};
use jarvis_core::config::Config;
use jarvis_core::orchestrator::{CoreError, Jarvis};
use serde_json::json;

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18785,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-planner-{}", uuid::Uuid::new_v4().simple())),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    Jarvis::new(cfg).unwrap()
}

#[test]
fn request_needs_a_valid_goal_without_credentials_and_a_configured_model() {
    let j = jarvis();
    assert!(matches!(
        j.plan_propose_request(""),
        Err(CoreError::BadRequest(_))
    ));
    assert!(matches!(
        j.plan_propose_request(&"x".repeat(301)),
        Err(CoreError::BadRequest(_))
    ));
    let secret = j
        .plan_propose_request("usa la chiave sk-abcdefghijklmnopqrstuvwxyz0123456789 per scrivere");
    assert!(secret.is_err(), "una credenziale non va inviata al modello");
    // Senza chiave cloud non c'è nessun modello a cui chiedere.
    assert!(j.plan_propose_request("calcola 2+2").unwrap().is_none());
}

#[test]
fn model_answer_becomes_a_draft_and_nothing_runs_by_itself() {
    let mut j = jarvis();
    let text = r#"```json
{"goal":"Calcola e conta","steps":[
 {"id":"calc","tool":"math.calc","arguments":{"expression":"6*7"}},
 {"id":"words","tool":"text.count","depends_on":["calc"],"arguments":{"text":"risultato ${calc.result}"}}]}
```"#;
    let out = j
        .plan_propose_finish(
            "calcola",
            Ok(Answer::Text(text.into())),
            Provider::Nim,
            false,
        )
        .unwrap();
    assert_eq!(out["created"], false);
    assert_eq!(out["proposal"]["proposed_by"], "model");
    assert_eq!(out["proposal"]["steps"].as_array().unwrap().len(), 2);
    assert!(j.plan_list().unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());

    let made = j
        .plan_propose_finish(
            "calcola",
            Ok(Answer::Text(text.into())),
            Provider::Nim,
            true,
        )
        .unwrap();
    assert_eq!(made["created"], true);
    let id = made["plan"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        made["plan"]["status"], "pending",
        "la bozza non parte da sola"
    );
    assert_eq!(made["plan"]["proposed_by"], "model");

    // L'esecuzione resta una scelta esplicita e passa dalla policy come ogni piano.
    let started = j.plan_start(&id).unwrap();
    assert_eq!(started["status"], "running");
}

#[test]
fn bad_model_answers_are_rejected_with_a_clear_message() {
    let mut j = jarvis();
    let ask = |j: &mut Jarvis, a: Result<Answer, Failure>| {
        j.plan_propose_finish("g", a, Provider::Nim, true)
    };
    let unknown = json!({"goal":"g","steps":[{"id":"a","tool":"disk.format"}]}).to_string();
    assert!(
        matches!(ask(&mut j, Ok(Answer::Text(unknown))), Err(CoreError::BadRequest(m)) if m.contains("non disponibile"))
    );
    // Gli strumenti di missione e di piano non si pianificano da un piano.
    let meta =
        json!({"goal":"g","steps":[{"id":"a","tool":"mission.create","arguments":{}}]}).to_string();
    assert!(ask(&mut j, Ok(Answer::Text(meta))).is_err());
    assert!(ask(&mut j, Ok(Answer::Text("non so".into()))).is_err());
    assert!(ask(&mut j, Ok(Answer::Call(json!({"tool": "math.calc"})))).is_err());
    assert!(matches!(
        ask(&mut j, Err(Failure::Unavailable)),
        Err(CoreError::Unavailable(_))
    ));
    assert!(j.plan_list().unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

fn run_to_end(j: &mut Jarvis, id: &str) -> serde_json::Value {
    j.plan_start(id).unwrap();
    for _ in 0..50 {
        let doc = j.plan_step(id).unwrap();
        if doc["status"] != "running" {
            return doc;
        }
    }
    panic!("il piano non termina");
}

fn failed_plan(j: &mut Jarvis) -> String {
    let plan = j
        .plan_create(&json!({"goal": "Leggi e conta", "steps": [
            {"id": "ok", "tool": "math.calc", "arguments": {"expression": "1+1"}},
            {"id": "read", "tool": "fs.read", "arguments": {"path": "non-esiste-davvero.txt"}},
            {"id": "after", "tool": "text.count", "depends_on": ["read"], "arguments": {"text": "x"}}]}))
        .unwrap();
    let id = plan["id"].as_str().unwrap().to_string();
    let done = run_to_end(j, &id);
    assert_eq!(done["status"], "failed");
    id
}

#[test]
fn only_a_stopped_plan_can_be_replanned_and_the_chain_is_bounded() {
    let mut j = jarvis();
    let ok = j
        .plan_create(&json!({"goal": "Calcolo", "steps": [{"id": "a", "tool": "math.calc", "arguments": {"expression": "1+1"}}]}))
        .unwrap();
    let id = ok["id"].as_str().unwrap();
    assert!(
        matches!(j.plan_replan_request(id), Err(CoreError::BadRequest(_))),
        "piano non avviato"
    );
    assert!(matches!(
        j.plan_replan_request("inesistente"),
        Err(CoreError::NotFound(_))
    ));
    let failed = failed_plan(&mut j);
    // Senza chiave cloud non c'è un modello da interrogare, ma la richiesta è ammessa.
    assert!(j.plan_replan_request(&failed).unwrap().is_none());
}

#[test]
fn recovery_plan_is_a_linked_draft_and_the_original_is_untouched() {
    let mut j = jarvis();
    let failed = failed_plan(&mut j);
    let before = j.plan_get(&failed).unwrap();
    let text = r#"{"goal":"Leggi e conta","steps":[
        {"id":"retry","tool":"text.count","arguments":{"text":"ciao mondo"}}]}"#;
    let out = j
        .plan_replan_finish(&failed, Ok(Answer::Text(text.into())), Provider::Nim, true)
        .unwrap();
    assert_eq!(out["created"], true);
    assert_eq!(
        out["plan"]["status"], "pending",
        "il piano di recupero non parte da solo"
    );
    assert_eq!(out["plan"]["replan_of"], failed.as_str());
    assert_eq!(out["plan"]["replan_depth"], 1);
    let after = j.plan_get(&failed).unwrap();
    assert_eq!(before["status"], after["status"]);
    assert_eq!(before["steps"], after["steps"]);

    // Una catena di recuperi si ferma: oltre il limite decide il proprietario.
    // La prima ripianificazione ha successo; da qui in poi i recuperi falliscono ancora.
    let mut current = failed.clone();
    let broken =
        r#"{"goal":"g","steps":[{"id":"r","tool":"fs.read","arguments":{"path":"manca.txt"}}]}"#;
    let mut refused = false;
    for _ in 0..5 {
        if current != failed {
            run_to_end(&mut j, &current);
        }
        match j.plan_replan_request(&current) {
            Ok(_) => {
                let next = j
                    .plan_replan_finish(
                        &current,
                        Ok(Answer::Text(broken.into())),
                        Provider::Nim,
                        true,
                    )
                    .unwrap();
                current = next["plan"]["id"].as_str().unwrap().to_string();
            }
            Err(CoreError::BadRequest(m)) => {
                assert!(m.contains("più volte"));
                refused = true;
                break;
            }
            Err(e) => panic!("errore inatteso: {e:?}"),
        }
    }
    assert!(refused, "la catena delle ripianificazioni deve fermarsi");
}

#[test]
fn replanning_is_blocked_by_the_emergency_stop() {
    let mut j = jarvis();
    let failed = failed_plan(&mut j);
    j.emergency_set(true).unwrap();
    assert!(matches!(
        j.plan_replan_request(&failed),
        Err(CoreError::Unavailable(_))
    ));
    assert!(matches!(
        j.plan_propose_request("calcola"),
        Err(CoreError::Unavailable(_))
    ));
}
