//! Missioni autonome (ADR-0096): ciclo di vita, esecuzione locale con il worker reale in modalità a regole,
//! diario, notifica, audit, ricorrenza, interruzioni del proprietario e avvio dalla chat con conferma.

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18785,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-missions-{}", uuid::Uuid::new_v4().simple())),
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

fn run_local(j: &mut Jarvis, id: &str) -> Value {
    let mut run = j.mission_begin(id).unwrap().expect("missione attiva");
    assert!(
        !run.uses_cloud(),
        "senza chiave il worker locale guida la missione"
    );
    j.mission_run_local(&mut run).unwrap()
}

#[test]
fn a_one_shot_mission_runs_in_the_background_and_reports() {
    let mut j = jarvis();
    let created = j.mission_create(&json!({"goal": "che ore sono"})).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["status"], "active");
    assert_eq!(
        j.mission_due(chrono::Utc::now()).as_deref(),
        Some(id.as_str())
    );

    let done = run_local(&mut j, &id);
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["runs"], 1);
    assert_eq!(done["journal"][0]["tools"], json!(["time.now"]));
    assert!(
        done["result"].as_str().unwrap().starts_with("Sono le"),
        "{done}"
    );
    // Conclusa: non è più pianificata.
    assert!(j
        .mission_due(chrono::Utc::now() + chrono::Duration::days(1))
        .is_none());

    let notes = j.notifications(20, false, None).unwrap();
    let mission: Vec<&Value> = notes["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["category"] == "mission")
        .collect();
    assert_eq!(mission.len(), 1, "{notes}");
    assert!(mission[0]["title"]
        .as_str()
        .unwrap()
        .starts_with("Missione completata"));
    let audit = j.audit_log(200).unwrap().to_string();
    assert!(
        audit.contains("mission.create") && audit.contains("mission.run"),
        "{audit}"
    );
}

#[test]
fn recurring_missions_are_rescheduled_and_the_owner_can_pause_cancel_and_delete() {
    let mut j = jarvis();
    let id = j
        .mission_create(&json!({"goal": "controlla lo spazio sul disco", "every_minutes": 60}))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let after = run_local(&mut j, &id);
    assert_eq!(
        (after["status"].as_str(), after["runs"].as_u64()),
        (Some("active"), Some(1)),
        "{after}"
    );
    let next = after["next_run_at"].as_str().expect("prossima esecuzione");
    assert!(next > jarvis_core::now().as_str());
    assert!(
        j.mission_due(chrono::Utc::now()).is_none(),
        "non è ancora scaduta"
    );

    assert_eq!(j.mission_action(&id, "pause").unwrap()["status"], "paused");
    assert!(j.mission_action(&id, "pause").is_err(), "già in pausa");
    assert!(
        j.mission_action(&id, "delete").is_err(),
        "non si elimina una missione aperta"
    );
    assert_eq!(j.mission_action(&id, "resume").unwrap()["status"], "active");
    // «Esegui ora» la rende subito scaduta.
    j.mission_action(&id, "run").unwrap();
    assert_eq!(
        j.mission_due(chrono::Utc::now()).as_deref(),
        Some(id.as_str())
    );
    assert_eq!(
        j.mission_action(&id, "cancel").unwrap()["status"],
        "cancelled"
    );
    assert!(
        j.mission_begin(&id).unwrap().is_none(),
        "una missione annullata non parte"
    );
    j.mission_action(&id, "delete").unwrap();
    assert!(j.missions().unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn an_owner_cancellation_during_a_run_is_respected() {
    let mut j = jarvis();
    let id = j
        .mission_create(&json!({"goal": "controlla lo spazio sul disco", "every_minutes": 5}))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut run = j.mission_begin(&id).unwrap().unwrap();
    j.mission_action(&id, "cancel").unwrap();
    let done = j.mission_run_local(&mut run).unwrap();
    assert_eq!(done["status"], "cancelled", "{done}");
    assert_eq!(done["runs"], 1, "l'esecuzione resta nel diario");
    assert!(done["next_run_at"].is_null());
}

#[test]
fn a_mission_cannot_use_tools_that_need_confirmation() {
    let mut j = jarvis();
    let id = j
        .mission_create(&json!({"goal": "scrivi bozza.txt: testo di prova"}))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = run_local(&mut j, &id);
    // fs.write chiede conferma: non è tra gli strumenti concessi, quindi nemmeno la regola locale lo propone.
    assert_eq!(done["journal"][0]["tools"], json!([]), "{done}");
    assert!(
        !done["journal"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("Scritto"),
        "{done}"
    );
}

#[test]
fn missions_start_from_the_chat_only_after_confirmation() {
    let mut j = jarvis();
    let out = j
        .chat("avvia una missione ogni 30 minuti: controlla lo spazio sul disco")
        .unwrap()
        .to_json();
    assert_eq!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert!(
        j.missions().unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "niente prima della conferma"
    );
    j.confirm(
        out["confirmation"]["confirmation_id"].as_str().unwrap(),
        true,
    )
    .unwrap();
    let items = j.missions().unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["every_minutes"], 30);
    let id = items[0]["id"].as_str().unwrap().to_string();
    let listed = j.chat("le mie missioni").unwrap().to_json();
    assert!(listed["reply"].as_str().unwrap().contains(&id), "{listed}");
    let cancelled = j
        .chat(&format!("annulla la missione {id}"))
        .unwrap()
        .to_json();
    assert!(
        cancelled["reply"].as_str().unwrap().contains("annullata"),
        "{cancelled}"
    );
}

#[test]
fn open_missions_are_limited() {
    let mut j = jarvis();
    for i in 0..8 {
        j.mission_create(&json!({"goal": format!("controlla il disco numero {i}")}))
            .unwrap();
    }
    assert!(j
        .mission_create(&json!({"goal": "una missione di troppo"}))
        .is_err());
}

#[test]
fn a_cloud_guided_mission_advances_one_checked_tool_at_a_time() {
    use jarvis_core::cloud::{Answer, Failure};
    use jarvis_core::orchestrator::Next;
    let mut j = jarvis();
    j.configure_cloud(&json!({"provider": "ollama_cloud", "key": "chiave-sintetica-di-prova"}))
        .unwrap();
    let id = j
        .mission_create(&json!({"goal": "controlla lo spazio sul disco e riassumi"}))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut run = j.mission_begin(&id).unwrap().unwrap();
    assert!(run.uses_cloud());

    let Next::Ask(first) = j.mission_advance(&mut run, None).unwrap() else {
        panic!("la prima mossa è una richiesta al modello");
    };
    let body = first.payload();
    let names: Vec<&str> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"system__disks") && names.contains(&"web__search"),
        "{names:?}"
    );
    // Strumenti con conferma, deleghe e missioni annidate non vengono nemmeno offerti.
    for forbidden in [
        "fs__write",
        "shell__run",
        "agent__delegate",
        "mission__start",
        "web__form",
    ] {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} non deve essere offerto"
        );
    }
    assert!(first.system.contains("COMPLETATA") && first.system.contains("esecuzione 1 di 3"));

    // Il modello propone un controllo dei dischi: passa dalla policy e il risultato torna al modello.
    let call = json!({"tool": "system.disks", "version": "1.0.0", "arguments": {}});
    let Next::Ask(second) = j
        .mission_advance(&mut run, Some(Ok(Answer::Call(call))))
        .unwrap()
    else {
        panic!("dopo lo strumento il modello riceve l'esito");
    };
    let messages = second.payload()["messages"].as_array().unwrap().clone();
    assert!(messages
        .iter()
        .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("succeeded")));
    // Una proposta fuori ambito viene rifiutata ma il ciclo prosegue.
    let call = json!({"tool": "fs.write", "version": "1.0.0", "arguments": {"path": "x.txt", "content": "y"}});
    let Next::Ask(third) = j
        .mission_advance(&mut run, Some(Ok(Answer::Call(call))))
        .unwrap()
    else {
        panic!("anche un rifiuto torna al modello");
    };
    assert!(
        third.steps[1].output.contains("denied"),
        "{}",
        third.steps[1].output
    );
    let Next::Done(doc) = j
        .mission_advance(
            &mut run,
            Some(Ok(Answer::Text("COMPLETATA: spazio sufficiente".into()))),
        )
        .unwrap()
    else {
        panic!("il rapporto chiude la missione");
    };
    assert_eq!(
        (doc["status"].as_str(), doc["result"].as_str()),
        (Some("completed"), Some("spazio sufficiente")),
        "{doc}"
    );
    assert_eq!(
        doc["journal"][0]["tools"],
        json!(["system.disks"]),
        "il rifiuto non è un'esecuzione"
    );

    // Un errore del provider non cancella la missione: riprova più tardi.
    let id2 = j
        .mission_create(
            &json!({"goal": "controlla lo spazio sul disco e riassumi", "every_minutes": 60}),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut run = j.mission_begin(&id2).unwrap().unwrap();
    let Next::Done(doc) = j
        .mission_advance(
            &mut run,
            Some(Err(Failure::Connection(jarvis_core::net::Cause::Other))),
        )
        .unwrap()
    else {
        panic!("un errore chiude l'esecuzione");
    };
    assert_eq!(
        (doc["status"].as_str(), doc["failures"].as_u64()),
        (Some("active"), Some(1)),
        "{doc}"
    );
    assert!(doc["next_run_at"].is_string());
}
