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

#[test]
fn recurring_plans_become_a_suggestion_the_owner_applies_or_dismisses() {
    let mut j = jarvis();
    assert!(j.improvements().unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    for i in 0..3 {
        let plan = j
            .plan_create(&json!({"goal": "Calcola e conta", "steps": [
                {"id": "a", "tool": "math.calc", "arguments": {"expression": format!("{i}+1")}},
                {"id": "b", "tool": "text.count", "depends_on": ["a"], "arguments": {"text": "ciao mondo"}}]}))
            .unwrap();
        assert_eq!(
            run_to_end(&mut j, plan["id"].as_str().unwrap())["status"],
            "succeeded"
        );
    }
    let listed = j.improvements().unwrap();
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{listed}");
    assert_eq!(items[0]["kind"], "save_procedure");
    assert!(listed["policy"]
        .as_str()
        .unwrap()
        .contains("propone e tu decidi"));
    let id = items[0]["id"].as_str().unwrap().to_string();

    // Nulla cambia finché il proprietario non approva: nessun modello salvato.
    assert!(j.plan_templates()["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["builtin"] == true));
    let applied = j.improvement_apply(&id).unwrap();
    assert_eq!(applied["template"]["name"], "calcola-e-conta");
    assert!(j.plan_templates()["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "calcola-e-conta"));
    assert!(
        j.improvements().unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "già salvata"
    );
    assert!(matches!(
        j.improvement_apply(&id),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        j.improvement_apply("sconosciuto"),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn dismissed_suggestions_are_not_proposed_again_and_review_ones_have_no_action() {
    let mut j = jarvis();
    for i in 0..3 {
        let plan = j
            .plan_create(&json!({"goal": format!("Giro {i}"), "steps": [
                {"id": "a", "tool": "math.calc", "arguments": {"expression": "1+1"}},
                {"id": "b", "tool": "text.count", "arguments": {"text": "x"}}]}))
            .unwrap();
        run_to_end(&mut j, plan["id"].as_str().unwrap());
    }
    let id = j.improvements().unwrap()["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    j.improvement_dismiss(&id).unwrap();
    assert!(j.improvements().unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    // Uno strumento che fallisce spesso produce solo un suggerimento da valutare.
    for _ in 0..5 {
        let plan = j
            .plan_create(&json!({"goal": "Lettura", "steps": [
                {"id": "r", "tool": "fs.read", "arguments": {"path": "manca-davvero.txt"}}]}))
            .unwrap();
        run_to_end(&mut j, plan["id"].as_str().unwrap());
    }
    let items = j.improvements().unwrap();
    let review = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "review_tool")
        .expect("strumento inaffidabile segnalato");
    assert_eq!(review["id"], "tool-fs.read");
    assert!(matches!(
        j.improvement_apply("tool-fs.read"),
        Err(CoreError::BadRequest(_))
    ));
}

#[test]
fn supervision_pauses_an_agent_and_hands_its_work_to_the_generalist() {
    let mut j = jarvis();
    let ask = "Scrivi una funzione Python e correggi il bug nel codice";
    let before = j.agent_route_explained(ask);
    let chosen = before["agent"]["agent"].as_str().unwrap().to_string();
    assert_ne!(
        chosen, "generale",
        "la richiesta tocca una specializzazione: {before}"
    );
    assert!(before["agent"]["supervision"].is_null());

    let state = j
        .agent_limits_set(&json!({"agent": chosen, "enabled": false}))
        .unwrap();
    let entry = state["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["agent"] == chosen.as_str())
        .unwrap();
    assert_eq!(entry["state"], "in pausa");
    assert!(
        entry["role"].as_str().is_some_and(|r| !r.is_empty()),
        "ruolo per le interfacce"
    );
    assert_eq!(entry["performance"]["runs"], 0);
    assert_eq!(entry["name"], chosen.as_str());
    let after = j.agent_route_explained(ask);
    assert_eq!(
        after["agent"]["supervision"]["effective"],
        jarvis_core::agents::GENERAL
    );

    // Il registro mostra la supervisione accanto alle prestazioni.
    let registry = j.agent_registry();
    let item = registry["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == chosen.as_str())
        .unwrap();
    assert_eq!(item["supervision"]["state"], "in pausa");

    // Riattivato con un limite: lavora finché ha quota.
    j.agent_limits_set(&json!({"agent": chosen, "enabled": true, "max_runs_per_hour": 1}))
        .unwrap();
    assert!(j.agent_route_explained(ask)["agent"]["supervision"].is_null());
    assert!(matches!(
        j.agent_limits_set(&json!({"agent": "inventato", "enabled": false})),
        Err(CoreError::BadRequest(_))
    ));
}

fn package(version: &str, tool: &str) -> serde_json::Value {
    json!({"contract": "jarvis.plugin/1", "name": "utilita-base", "version": version,
        "description": "Competenza e procedura di prova",
        "skills": [{"name": "Conta le parole", "instructions": "Usa il conteggio quando serve.",
            "triggers": ["conta parole"], "tools": [tool]}],
        "plan_templates": [{"name": "conta", "description": "Conta le parole", "goal": "Conta {{testo}}",
            "params": [{"name": "testo", "required": true}],
            "steps": [{"id": "c", "tool": tool, "arguments": {"text": "{{testo}}"}}]}]})
}

#[test]
fn plugin_packages_install_run_update_and_remove_atomically() {
    let mut j = jarvis();
    assert!(j.plugins()["items"].as_array().unwrap().is_empty());
    let skills_before = j.skills().unwrap()["items"].as_array().unwrap().len();

    // Un pacchetto con uno strumento sconosciuto non cambia nulla.
    let bad = package("1.0.0", "disk.format");
    assert!(matches!(
        j.plugin_install(&bad),
        Err(CoreError::BadRequest(_))
    ));
    assert_eq!(
        j.skills().unwrap()["items"].as_array().unwrap().len(),
        skills_before
    );
    assert!(j.plugins()["items"].as_array().unwrap().is_empty());

    let entry = j.plugin_install(&package("1.0.0", "text.count")).unwrap();
    assert_eq!(entry["name"], "utilita-base");
    assert_eq!(entry["templates"][0], "utilita-base-conta");
    assert_eq!(entry["digest"].as_str().unwrap().len(), 64);
    assert_eq!(
        j.skills().unwrap()["items"].as_array().unwrap().len(),
        skills_before + 1
    );

    // La procedura del pacchetto si usa come ogni modello, sotto policy e audit.
    let templates = j.plan_templates();
    let mine = templates["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "utilita-base-conta")
        .expect("modello del pacchetto");
    assert_eq!(mine["plugin"], "utilita-base");
    let plan = j
        .plan_from_template(
            &json!({"template": "utilita-base-conta", "params": {"testo": "uno due tre"}}),
        )
        .unwrap();
    let done = run_to_end(&mut j, plan["id"].as_str().unwrap());
    assert_eq!(done["status"], "succeeded");
    assert_eq!(done["steps"][0]["output"]["words"], 3);

    // Aggiornare lo stesso pacchetto sostituisce i componenti senza duplicarli.
    j.plugin_install(&package("1.1.0", "text.count")).unwrap();
    assert_eq!(j.plugins()["items"].as_array().unwrap().len(), 1);
    assert_eq!(j.plugins()["items"][0]["version"], "1.1.0");
    assert_eq!(
        j.skills().unwrap()["items"].as_array().unwrap().len(),
        skills_before + 1
    );
    let named = j.plan_templates()["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] == "utilita-base-conta")
        .count();
    assert_eq!(named, 1);

    // Una versione non valida lascia installata quella precedente.
    assert!(j.plugin_install(&package("2.0.0", "disk.format")).is_err());
    assert_eq!(j.plugins()["items"][0]["version"], "1.1.0");

    j.plugin_remove("utilita-base").unwrap();
    assert!(j.plugins()["items"].as_array().unwrap().is_empty());
    assert_eq!(
        j.skills().unwrap()["items"].as_array().unwrap().len(),
        skills_before
    );
    assert!(j.plan_templates()["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["name"] != "utilita-base-conta"));
    assert!(matches!(
        j.plugin_remove("utilita-base"),
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn broad_delegations_fall_back_to_the_normal_chat_without_a_cloud_model() {
    // ADR-0172: senza modello cloud «sistema tutto» non crea piani e la chat risponde come sempre.
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let j = jarvis();
    let app = jarvis_core::api::router(jarvis_core::api::state(j, "t".into(), 18785));
    let call = |method: &str, uri: &str, body: serde_json::Value| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "127.0.0.1:18785")
            .header("content-type", "application/json")
            .header("x-jarvis-token", "t")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    for uri in ["/api/chat", "/api/chat/stream"] {
        let res = app
            .clone()
            .oneshot(call("POST", uri, json!({"message": "Sistema tutto"})))
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "{uri}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&bytes).contains("Ho preparato un piano"));
    }
    let res = app
        .oneshot(call("GET", "/api/plans", json!({})))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let plans: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(plans["items"].as_array().unwrap().is_empty(), "{plans}");
}
