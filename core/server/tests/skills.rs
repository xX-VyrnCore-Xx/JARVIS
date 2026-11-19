//! Competenze (ADR-0048): incluse al primo avvio, create e modificate dal proprietario con strumenti del
//! catalogo, attivate dalle parole di attivazione o dal nome, disattivabili, registrate nell'audit.

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::json;

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18784,
        data_dir: std::env::temp_dir()
            .join(format!("jarvis-skills-{}", uuid::Uuid::new_v4().simple())),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    Jarvis::new(cfg).unwrap()
}

fn skill_steps(out: &serde_json::Value) -> Vec<String> {
    out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "skill")
        .map(|s| s["skill"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn owner_manages_skills_and_they_activate_from_the_request() {
    let mut j = jarvis();
    let list = j.skills().unwrap();
    let ids: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for id in [
        "briefing",
        "scrittura",
        "ricerca",
        "organizzazione",
        "spese",
    ] {
        assert!(ids.contains(&id), "{list}");
    }

    // Strumento inesistente o istruzioni vuote: rifiutate.
    assert!(j
        .skill_save(
            None,
            &json!({"name": "X", "instructions": "fai", "tools": ["shell.nonesiste"]})
        )
        .is_err());
    assert!(j
        .skill_save(None, &json!({"name": "X", "instructions": "  "}))
        .is_err());

    let saved = j
        .skill_save(
            None,
            &json!({"name": "Viaggi", "description": "Pianificazione dei viaggi", "instructions": "Proponi sempre un itinerario giorno per giorno.",
                    "triggers": ["itinerario", "viaggio a"], "tools": ["weather.forecast", "web.search"]}),
        )
        .unwrap();
    let id = saved["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("viaggi-"), "{saved}");

    let out = j
        .chat("prepara un itinerario per Lisbona")
        .unwrap()
        .to_json();
    assert_eq!(skill_steps(&out), std::slice::from_ref(&id), "{out}");
    let out = j
        .chat("usa spese: quanto ho messo da parte?")
        .unwrap()
        .to_json();
    assert_eq!(skill_steps(&out), ["spese"], "richiamo per nome: {out}");
    assert!(skill_steps(&j.chat("che ore sono").unwrap().to_json()).is_empty());

    // Disattivata: non si attiva più.
    j.skill_save(
        Some(&id),
        &json!({"name": "Viaggi", "instructions": "Proponi sempre un itinerario.", "triggers": ["itinerario"], "enabled": false}),
    )
    .unwrap();
    assert!(skill_steps(&j.chat("prepara un itinerario per Roma").unwrap().to_json()).is_empty());

    // Le incluse si disattivano ma non si eliminano; le altre sì.
    assert!(j.skill_delete("briefing").is_err());
    j.skill_delete(&id).unwrap();
    assert!(j.skill_delete(&id).is_err());
    assert!(j
        .skill_save(
            Some("inesistente"),
            &json!({"name": "Y", "instructions": "z"})
        )
        .is_err());

    let audit = j.audit_log(100).unwrap().to_string();
    assert!(
        audit.contains("skill.save") && audit.contains("skill.delete"),
        "{audit}"
    );
}
