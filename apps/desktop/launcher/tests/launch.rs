//! Avvia il Core reale (build di debug del workspace) come farebbe l'app desktop.

use std::path::PathBuf;
use std::time::Duration;

use jarvis_launcher::{find_core_binary, launch, LaunchConfig, LaunchError};

fn home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn config() -> LaunchConfig {
    let data = std::env::temp_dir().join(format!("jarvis-launcher-{}", std::process::id()));
    let mut cfg = LaunchConfig::discover(&home(), &data)
        .expect("binario del Core (cargo build -p jarvis-core)");
    cfg.env.push(("JARVIS_AI_BACKEND".into(), "offline".into()));
    cfg
}

#[test]
fn launches_core_on_free_port_and_stops_it() {
    let cfg = config();
    let mut core = launch(&cfg).expect("avvio del Core");
    assert!(core.url.starts_with("http://localhost:"));
    let health: String = ureq::get(&format!("http://127.0.0.1:{}/api/health", core.port))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    assert!(health.contains("\"status\":\"up\""), "{health}");
    // L'interfaccia è servita dal Core con il token di sessione.
    let page = ureq::get(&core.url)
        .set("Host", &format!("localhost:{}", core.port))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    assert!(page.contains("jarvis-token"));
    // Il token generato dal launcher è quello del Core: l'app lo usa per le notifiche native (ADR-0044).
    assert_eq!(core.token.len(), 32);
    assert!(
        page.contains(&core.token),
        "la pagina usa il token del launcher"
    );
    let notifications = ureq::get(&format!(
        "http://127.0.0.1:{}/api/notifications?limit=5",
        core.port
    ))
    .set("X-Jarvis-Token", &core.token)
    .call()
    .unwrap()
    .into_string()
    .unwrap();
    assert!(notifications.contains("\"items\""), "{notifications}");
    assert!(
        cfg.data_dir.join("jarvis.sqlite3").exists(),
        "i dati vanno nella cartella dell'utente"
    );
    core.stop();
    assert!(!core.is_alive());
    assert!(
        ureq::get(&format!("http://127.0.0.1:{}/api/health", core.port))
            .timeout(Duration::from_secs(1))
            .call()
            .is_err()
    );
}

#[test]
fn core_exits_when_the_parent_goes_away() {
    let mut cfg = config();
    cfg.data_dir =
        std::env::temp_dir().join(format!("jarvis-launcher-orphan-{}", std::process::id()));
    let mut core = launch(&cfg).expect("avvio del Core");
    core.close_parent_pipe_for_tests();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while core.is_alive() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!core.is_alive(), "il Core non deve restare orfano");
    assert!(std::fs::read_to_string(&core.log)
        .unwrap()
        .contains("Processo padre terminato"));
}

#[test]
fn startup_failure_is_reported_with_log() {
    let mut cfg = config();
    cfg.data_dir =
        std::env::temp_dir().join(format!("jarvis-launcher-fail-{}", std::process::id()));
    cfg.env.push(("JARVIS_HOST".into(), "0.0.0.0".into()));
    cfg.env.push(("JARVIS_PORT".into(), "x".into()));
    // Variabile non valida: il Core rifiuta di partire e il launcher lo riporta con il percorso del log.
    match launch(&cfg) {
        Err(LaunchError::Exited { log, .. }) => assert!(std::fs::read_to_string(log)
            .unwrap()
            .contains("Configurazione non valida")),
        Err(other) => panic!("errore inatteso: {other}"),
        Ok(_) => panic!("doveva fallire"),
    }
}

#[test]
fn missing_binary_lists_searched_paths() {
    let err = find_core_binary(&std::env::temp_dir().join("nessun-jarvis")).unwrap_err();
    assert!(err
        .to_string()
        .contains("cargo build --release -p jarvis-core"));
}
