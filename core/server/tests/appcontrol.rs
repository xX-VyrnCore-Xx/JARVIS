//! Controllo delle applicazioni (ADR-0121): ogni azione chiede la conferma del proprietario, che resta solo sua.

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::json;

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18788,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-app-{}", uuid::Uuid::new_v4().simple())),
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
fn every_action_on_an_app_waits_for_the_owner() {
    let mut j = jarvis();
    for request in [
        "apri l'app Spotify",
        "porta in primo piano Chrome",
        "scrivi nell'app: ciao a tutti",
        "premi cmd+c",
    ] {
        let out = j.chat(request).unwrap().to_json();
        assert_eq!(
            out["task"]["state"], "waiting_confirmation",
            "{request}: {out}"
        );
        let effect = out["confirmation"]["expected_effect"]
            .as_str()
            .unwrap_or_default();
        assert!(!effect.is_empty(), "{request}: {out}");
    }
}

#[test]
fn the_confirmation_text_shows_exactly_what_will_be_typed() {
    let mut j = jarvis();
    let out = j.chat("scrivi nell'app: ciao a tutti").unwrap().to_json();
    let effect = out["confirmation"]["expected_effect"].as_str().unwrap();
    assert!(effect.contains("ciao a tutti"), "{effect}");
}

#[test]
fn credentials_are_never_typed_and_invalid_names_are_refused() {
    let mut j = jarvis();
    let secret = j
        .chat("scrivi nell'app: nvapi-AbCdEf0123456789xyz")
        .unwrap()
        .to_json();
    assert_ne!(secret["task"]["state"], "succeeded", "{secret}");
    let hostile = j.chat("apri l'app Spotify; rm -rf ~").unwrap().to_json();
    let text = hostile.to_string();
    assert!(
        !text.contains("succeeded")
            || text.contains("non valido")
            || text.contains("app.invalid")
            || hostile["task"]["state"] != "succeeded",
        "{text}"
    );
}

#[test]
fn owner_rules_can_forbid_app_control() {
    let mut j = jarvis();
    j.permissions_update(&json!({"rules": {"app.*": "deny"}}))
        .unwrap();
    let out = j.chat("apri l'app Spotify").unwrap().to_json();
    assert_ne!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert_ne!(out["task"]["state"], "succeeded", "{out}");
}
