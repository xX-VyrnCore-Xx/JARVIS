//! Test del rollback di release (RNF-08, ADR-0015): copia automatica prima di un cambio di versione e ripristino.

use std::path::{Path, PathBuf};

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use jarvis_core::rollback;

fn config(data: &Path) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18772,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    }
}

fn memory_texts(j: &Jarvis) -> Vec<String> {
    j.memory()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn version_change_snapshots_data_and_rollback_restores_it() {
    let root = std::env::temp_dir().join(format!("jarvis-rb-{}", uuid::Uuid::new_v4().simple()));
    let data = root.join("data");
    {
        let mut j = Jarvis::new(config(&data)).unwrap();
        j.chat("ricorda che prima dell'aggiornamento").unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(data.join("VERSION")).unwrap(),
        rollback::CURRENT
    );
    // Stessa versione: nessuna copia.
    drop(Jarvis::new(config(&data)).unwrap());
    assert!(rollback::list(&data).is_empty());

    // Simula dati lasciati da una versione precedente.
    std::fs::write(data.join("VERSION"), "0.0.9").unwrap();
    std::fs::write(
        data.join("updates.json"),
        r#"{"enabled":true,"channel":"beta"}"#,
    )
    .unwrap();
    {
        let mut j = Jarvis::new(config(&data)).unwrap();
        let notes = j.notifications(10, true, None).unwrap().to_string();
        assert!(notes.contains("aggiornato da 0.0.9"), "{notes}");
        j.chat("ricorda che dopo l'aggiornamento").unwrap();
        assert_eq!(memory_texts(&j).len(), 2);
    }
    let snaps = rollback::list(&data);
    assert_eq!(snaps.len(), 1);
    assert!(snaps[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("0.0.9-"));

    let broken = root.join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("jarvis.sqlite3"), "non è SQLite").unwrap();
    assert!(rollback::restore(&data, Some(&broken)).is_err());
    let out = rollback::restore(&data, None).unwrap();
    assert!(std::fs::read_to_string(data.join("updates.json"))
        .unwrap()
        .contains("beta"));
    assert_eq!(out["version"], "0.0.9");
    assert!(
        Path::new(out["previous_data"].as_str().unwrap())
            .join("jarvis.sqlite3")
            .exists(),
        "dati sostituiti conservati"
    );
    std::fs::write(data.join("VERSION"), rollback::CURRENT).unwrap();
    let j = Jarvis::new(config(&data)).unwrap();
    let texts = memory_texts(&j);
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(texts[0].contains("prima"));
    let events = j.audit_log(500).unwrap();
    assert_eq!(events["chain_valid"], true, "{events}");
}

#[test]
fn only_the_last_three_snapshots_are_kept() {
    let data = std::env::temp_dir()
        .join(format!("jarvis-rb-{}", uuid::Uuid::new_v4().simple()))
        .join("data");
    drop(Jarvis::new(config(&data)).unwrap());
    for v in ["0.0.1", "0.0.2", "0.0.3", "0.0.4"] {
        std::fs::write(data.join("VERSION"), v).unwrap();
        drop(Jarvis::new(config(&data)).unwrap());
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    let names: Vec<String> = rollback::list(&data)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(
        names[0].starts_with("0.0.2") && names[2].starts_with("0.0.4"),
        "{names:?}"
    );
}
