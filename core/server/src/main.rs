//! Avvio del Core JARVIS: `cargo run --release -p jarvis-core` oppure `./jarvis.sh`.

use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;

/// Passphrase da `JARVIS_BACKUP_PASSPHRASE` oppure dalla prima riga dello standard input.
fn passphrase() -> String {
    if let Ok(p) = std::env::var("JARVIS_BACKUP_PASSPHRASE") {
        return p;
    }
    eprint!(
        "Passphrase del backup (almeno {} caratteri): ",
        jarvis_core::backup::MIN_PASSPHRASE
    );
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim_end_matches(['\r', '\n']).to_string()
}

/// `jarvis backup` e `jarvis restore <file>` (RNF-08). Restituisce il codice di uscita.
fn maintenance(cfg: &Config, args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("node") => {
            // Nodo esecutore (RF-11): jarvis node <url-del-core> [--code C] [--name N] [--tools a,b]
            let Some(core_url) = args.get(1).filter(|a| a.starts_with("http")) else {
                eprintln!("Uso: jarvis node <url-del-core> <codice> [--name <nome>] [--tools system.info,fs.list,...]");
                return Some(2);
            };
            let opt = |flag: &str| {
                args.iter()
                    .position(|a| a == flag)
                    .and_then(|i| args.get(i + 1))
                    .cloned()
            };
            let host_name = std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("COMPUTERNAME"))
                .unwrap_or_else(|_| "Nodo".into());
            let agent_cfg = jarvis_core::node_agent::AgentConfig {
                core_url: core_url.trim_end_matches('/').to_string(),
                dir: std::env::var_os("JARVIS_NODE_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| cfg.data_dir.join("node")),
                name: opt("--name").unwrap_or(host_name),
                // Il codice si può passare anche come secondo argomento (comodo da PowerShell).
                code: opt("--code")
                    .or_else(|| args.get(2).filter(|a| !a.starts_with("--")).cloned()),
                capabilities: opt("--tools")
                    .map(|t| t.split(',').map(|x| x.trim().to_string()).collect())
                    .unwrap_or_default(),
                poll_wait_s: 20,
            };
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            Some(match jarvis_core::node_agent::run(&agent_cfg, stop) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("✗ {e}");
                    if matches!(e, jarvis_core::node_agent::AgentError::Revoked) {
                        3
                    } else {
                        1
                    }
                }
            })
        }
        Some("--version") => {
            println!("jarvis {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        Some("backup") => {
            let mut core = match Jarvis::new(cfg.clone()) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("✗ {e}. Con JARVIS avviato, crea il backup dalla sezione Sistema dell'interfaccia.");
                    return Some(1);
                }
            };
            Some(match core.create_backup(&passphrase()) {
                Ok(s) => {
                    println!(
                        "✓ Backup creato: {}",
                        cfg.backups_dir()
                            .join(s["file"].as_str().unwrap_or(""))
                            .display()
                    );
                    0
                }
                Err(e) => {
                    eprintln!("✗ Backup non riuscito: {e:?}");
                    1
                }
            })
        }
        Some("restore") => {
            let Some(file) = args.get(1) else {
                eprintln!("Uso: jarvis restore <file.jbak>");
                return Some(2);
            };
            // Il ripristino richiede il Core fermo: il blocco esclusivo sulla cartella dati lo garantisce.
            if cfg.data_dir.join(".lock").exists() {
                if let Ok(f) = std::fs::OpenOptions::new()
                    .write(true)
                    .open(cfg.data_dir.join(".lock"))
                {
                    if f.try_lock().is_err() {
                        eprintln!(
                            "✗ JARVIS è in esecuzione su {}: chiudilo prima del ripristino.",
                            cfg.data_dir.display()
                        );
                        return Some(1);
                    }
                }
            }
            Some(
                match jarvis_core::backup::restore(
                    std::path::Path::new(file),
                    &passphrase(),
                    &cfg.data_dir,
                ) {
                    Ok(s) => {
                        println!(
                            "✓ Ripristino completato da {} (creato il {})",
                            file,
                            s["created_at"].as_str().unwrap_or("?")
                        );
                        if let Some(prev) = s["previous_data"].as_str() {
                            println!("  I dati precedenti sono conservati in {prev}");
                        }
                        0
                    }
                    Err(e) => {
                        eprintln!("✗ Ripristino non riuscito: {e}. I dati attuali non sono stati modificati.");
                        1
                    }
                },
            )
        }
        Some("rollback") => {
            if core_running(cfg) {
                eprintln!(
                    "✗ JARVIS è in esecuzione su {}: chiudilo prima del rollback.",
                    cfg.data_dir.display()
                );
                return Some(1);
            }
            let snapshots = jarvis_core::rollback::list(&cfg.data_dir);
            if args.get(1).map(String::as_str) == Some("--list") || snapshots.is_empty() {
                if snapshots.is_empty() {
                    println!(
                        "Nessuna copia per il rollback in {}",
                        jarvis_core::rollback::rollback_dir(&cfg.data_dir).display()
                    );
                }
                for s in &snapshots {
                    println!("  {}", s.display());
                }
                return Some(if snapshots.is_empty() { 1 } else { 0 });
            }
            let which = args.get(1).map(std::path::PathBuf::from);
            Some(
                match jarvis_core::rollback::restore(&cfg.data_dir, which.as_deref()) {
                    Ok(s) => {
                        println!(
                            "✓ Dati riportati alla versione {} da {}",
                            s["version"].as_str().unwrap_or("?"),
                            s["restored_from"].as_str().unwrap_or("?")
                        );
                        println!(
                            "  I dati sostituiti sono conservati in {}",
                            s["previous_data"].as_str().unwrap_or("?")
                        );
                        println!("  Avvia ora la release di quella versione.");
                        0
                    }
                    Err(e) => {
                        eprintln!("✗ Rollback non riuscito: {e}");
                        1
                    }
                },
            )
        }
        _ => None,
    }
}

/// Vero se un Core detiene il blocco della cartella dati.
fn core_running(cfg: &Config) -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open(cfg.data_dir.join(".lock"))
        .is_ok_and(|f| f.try_lock().is_err())
}

#[tokio::main]
async fn main() {
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configurazione non valida: {e}");
            std::process::exit(2);
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = maintenance(&cfg, &args) {
        std::process::exit(code);
    }
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
    // Dispositivi associati (RF-11): host pubblicati da un proxy fidato verso 127.0.0.1, es. `tailscale serve`.
    let remote_hosts: Vec<String> = std::env::var("JARVIS_ALLOWED_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .filter(|h| {
            let ok = h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') && h.contains('.');
            if !ok {
                eprintln!("JARVIS_ALLOWED_HOSTS: host ignorato «{h}» (servono nomi completi, senza caratteri jolly)");
            }
            ok
        })
        .collect();
    if !remote_hosts.is_empty() {
        println!(
            "  Dispositivi : accesso associato da {}",
            remote_hosts.join(", ")
        );
    }
    let state = api::state_with_remote(core, token, port, remote_hosts);
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
    // mTLS dei nodi (ADR-0041): porta dedicata, solo su richiesta esplicita del proprietario.
    if let Ok(tls_addr) = std::env::var("JARVIS_NODE_TLS_ADDR") {
        match tls_addr.parse::<std::net::SocketAddr>() {
            Ok(addr) => {
                let identity = state
                    .core
                    .lock()
                    .map_err(|e| e.to_string())
                    .and_then(|mut j| j.enable_node_tls(addr.port()));
                match (identity, tokio::net::TcpListener::bind(addr).await) {
                    (Ok(identity), Ok(listener)) => {
                        let fp = identity.fingerprint().unwrap_or_default();
                        println!(
                            "  Nodi (mTLS) : {addr} · impronta del Core {}…",
                            &fp[..16.min(fp.len())]
                        );
                        let s = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = api::serve_node_tls(s, listener, identity).await {
                                eprintln!("Porta TLS dei nodi terminata: {e}");
                            }
                        });
                    }
                    (Err(e), _) => eprintln!("mTLS dei nodi non attivato: {e}"),
                    (_, Err(e)) => eprintln!("Porta TLS dei nodi {addr} non disponibile: {e}"),
                }
            }
            Err(_) => {
                eprintln!("JARVIS_NODE_TLS_ADDR non valido: «{tls_addr}» (esempio: 0.0.0.0:8766)")
            }
        }
    }
    let core = state.core.clone();
    let app = api::router(state);
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("Server terminato con errore: {e}");
    }
    // Ultima istantanea delle metriche, così lo storico copre anche la fine della sessione.
    if let Ok(mut j) = core.lock() {
        let _ = j.persist_metrics(true);
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
