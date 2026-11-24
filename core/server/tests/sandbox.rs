//! Terminale sandbox (ADR-0105): catalogo, regole della chat, interruttore del proprietario e
//! assenza di conferma per i comandi isolati. L'esecuzione reale richiede Docker (test `#[ignore]` nel modulo).

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::json;

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18786,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-sandbox-{}", uuid::Uuid::new_v4().simple())),
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
fn status_is_readable_and_reset_needs_confirmation() {
    let mut j = jarvis();
    let status = j.chat("stato sandbox").unwrap().to_json();
    assert!(
        status["reply"].as_str().unwrap().contains("Sandbox"),
        "{status}"
    );
    let reset = j.chat("azzera la sandbox").unwrap().to_json();
    assert_eq!(reset["task"]["state"], "waiting_confirmation", "{reset}");
}

#[test]
fn commands_run_without_confirmation_and_fail_cleanly_without_docker() {
    let mut j = jarvis();
    let out = j.chat("sandbox: echo ciao").unwrap().to_json();
    assert_ne!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert!(out["confirmation"].is_null(), "{out}");
}

#[test]
fn the_owner_switch_blocks_the_terminal() {
    let mut j = jarvis();
    assert_eq!(j.sandbox_settings()["enabled"], true);
    j.sandbox_update(&json!({"enabled": false})).unwrap();
    assert_eq!(j.sandbox_settings()["enabled"], false);
    let out = j.chat("sandbox: echo ciao").unwrap().to_json();
    let text = out.to_string();
    assert!(
        text.contains("sandbox.disabled") || text.contains("disattivat"),
        "{text}"
    );
    assert!(j.sandbox_update(&json!({"enabled": "sì"})).is_err());
}
