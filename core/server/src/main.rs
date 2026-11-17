//! Avvio del Core JARVIS: `cargo run --release -p jarvis-core` oppure `./jarvis.sh`.

use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;

#[tokio::main]
async fn main() {
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configurazione non valida: {e}");
            std::process::exit(2);
        }
    };
    let addr = std::net::SocketAddr::new(cfg.host, cfg.port);
    let (token, port, data_dir, tick) = (
        cfg.token.clone(),
        cfg.port,
        cfg.data_dir.clone(),
        cfg.scheduler_tick_s,
    );
    let core = match Jarvis::new(cfg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Avvio del Core non riuscito: {e}");
            std::process::exit(1);
        }
    };
    let state = api::state(core, token, port);
    let warm = state.core.clone();
    let health = tokio::task::spawn_blocking(move || {
        warm.lock().map(|mut j| j.health()).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    *state.last_health.lock().unwrap() = health.clone();

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "Porta {addr} non disponibile: {e}. Imposta JARVIS_PORT per usarne un'altra."
            );
            std::process::exit(1);
        }
    };
    let backend = &health["ai"]["gateway"]["backend"];
    println!("JARVIS v{} pronto", env!("CARGO_PKG_VERSION"));
    println!("  Interfaccia : http://localhost:{port}/");
    println!("  Dati        : {}", data_dir.display());
    println!(
        "  AI          : {} ({}){}",
        backend["name"].as_str().unwrap_or("non disponibile"),
        backend["model"].as_str().unwrap_or("-"),
        health["ai"]["gateway"]["reason"]
            .as_str()
            .map(|r| format!(" — {r}"))
            .unwrap_or_default()
    );
    println!(
        "  Automazioni : {} attive, controllo ogni {tick} s",
        health["automations"]
    );
    println!("  Arresto     : Ctrl+C");
    api::spawn_scheduler(state.clone(), std::time::Duration::from_secs(tick));
    let app = api::router(state);
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("Server terminato con errore: {e}");
    }
    println!("JARVIS arrestato.");
}

/// Arresto con Ctrl+C oppure, se `JARVIS_EXIT_WITH_PARENT=stdin`, quando il processo padre (per esempio
/// l'app desktop) chiude lo stdin: accade anche se il padre termina in modo anomalo, su ogni sistema operativo.
async fn shutdown_signal() {
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    if std::env::var("JARVIS_EXIT_WITH_PARENT").is_ok_and(|v| v == "stdin") {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
            let _ = tx.send(());
        });
    } else {
        std::mem::forget(tx);
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = rx => println!("Processo padre terminato: arresto del Core."),
    }
}
