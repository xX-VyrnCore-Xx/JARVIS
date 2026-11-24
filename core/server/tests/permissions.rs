//! Permessi del proprietario (ADR-0116): le regole restringono la policy del catalogo e non possono allargarla.

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::json;

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18787,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-perm-{}", uuid::Uuid::new_v4().simple())),
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
fn an_ask_rule_turns_a_free_tool_into_a_confirmation_and_deny_blocks_it() {
    let mut j = jarvis();
    let free = j.chat("che ore sono").unwrap().to_json();
    assert_ne!(free["task"]["state"], "waiting_confirmation", "{free}");

    j.permissions_update(&json!({"rules": {"time.now": "ask"}}))
        .unwrap();
    let asked = j.chat("che ore sono").unwrap().to_json();
    assert_eq!(asked["task"]["state"], "waiting_confirmation", "{asked}");

    j.permissions_update(&json!({"rules": {"time.now": "deny"}}))
        .unwrap();
    let denied = j.chat("che ore sono").unwrap().to_json();
    assert_ne!(denied["task"]["state"], "succeeded", "{denied}");
    assert!(
        denied.to_string().contains("owner_rule_deny") || denied.to_string().contains("vietato"),
        "{denied}"
    );
}

#[test]
fn the_read_only_profile_blocks_tools_that_change_something_but_not_reads() {
    let mut j = jarvis();
    j.permissions_update(&json!({"profile": "read_only"}))
        .unwrap();
    let blocked = j.chat("sandbox: echo ciao").unwrap().to_json();
    assert_ne!(blocked["task"]["state"], "succeeded", "{blocked}");
    let time = j.chat("che ore sono").unwrap().to_json();
    assert_eq!(time["task"]["state"], "succeeded", "{time}");
}

#[test]
fn rules_for_unknown_tools_and_bad_input_are_rejected_and_the_listing_shows_restrictions() {
    let mut j = jarvis();
    assert!(j
        .permissions_update(&json!({"rules": {"strumento.inesistente": "deny"}}))
        .is_err());
    assert!(j.permissions_update(&json!({"profile": "libero"})).is_err());
    j.permissions_update(&json!({"profile": "prudent"}))
        .unwrap();
    let listing = j.permissions();
    assert_eq!(listing["profile"], "prudent");
    let tools = listing["tools"].as_array().unwrap();
    let get = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap().clone();
    assert_eq!(get("notes.add")["restriction"], "ask");
    assert_eq!(get("time.now")["restriction"], "none");
}
