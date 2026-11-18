//! JARVIS desktop (RF-10): guscio Tauri sopra l'interfaccia servita dal Core.
//!
//! - La finestra parte da una pagina locale di attesa; il Core viene avviato su una porta di loopback
//!   libera con i dati nella cartella dell'applicazione dell'utente, poi la finestra naviga verso di esso.
//! - La navigazione è limitata alla pagina locale e al Core; gli altri indirizzi http(s) (per esempio le
//!   fonti Web) si aprono nel browser di sistema.
//! - Nessuna capability Tauri è concessa: l'interfaccia remota non ha accesso alle API native.
//! - Alla chiusura dell'app il Core viene terminato (il worker Python si chiude con il suo stdin).
//! - Installata con il programma di setup (ADR-0016), l'app usa il Core incluso accanto all'eseguibile
//!   (`jarvis-core`), il runtime AI e i contratti tra le risorse e un Python autonomo incluso: non serve
//!   installare altro.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use jarvis_launcher::{launch, LaunchConfig, LaunchError, RunningCore};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

struct CoreState(Mutex<Option<RunningCore>>);

/// Radice del repository: `JARVIS_HOME`, altrimenti la posizione del sorgente al momento della build.
fn jarvis_home() -> PathBuf {
    std::env::var_os("JARVIS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

/// Configurazione dell'app installata: risorse con `ai/` e `contracts/`, Core e Python inclusi.
/// `None` quando l'app gira dai sorgenti (sviluppo).
fn bundled_config(resources: &std::path::Path, data_dir: &std::path::Path) -> Option<LaunchConfig> {
    if !resources.join("ai/jarvis_ai/worker.py").is_file() {
        return None;
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let core = exe_dir.join(if cfg!(windows) {
        "jarvis-core.exe"
    } else {
        "jarvis-core"
    });
    let python = if cfg!(windows) {
        resources.join("python/python.exe")
    } else {
        resources.join("python/bin/python3")
    };
    Some(LaunchConfig {
        home: resources.to_path_buf(),
        core_bin: core,
        data_dir: data_dir.to_path_buf(),
        python: python.to_string_lossy().into_owned(),
        // Le risorse installate possono essere in sola lettura: niente cache .pyc accanto ai sorgenti.
        env: vec![("PYTHONDONTWRITEBYTECODE".into(), "1".into())],
        startup_timeout: std::time::Duration::from_secs(90),
    })
}

fn local_page(url: &url::Url) -> bool {
    matches!(url.scheme(), "tauri") || url.host_str() == Some("tauri.localhost")
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(CoreState(Mutex::new(None)))
        .setup(|app| {
            let data_dir = match std::env::var_os("JARVIS_DATA_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let core_port = std::sync::Arc::new(Mutex::new(None::<u16>));
            let allowed = core_port.clone();
            let opener = app.handle().clone();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("JARVIS")
                    .inner_size(1200.0, 820.0)
                    .min_inner_size(380.0, 520.0)
                    .on_navigation(move |url| {
                        let core = *allowed.lock().unwrap_or_else(|e| e.into_inner());
                        let to_core =
                            matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"))
                                && url.port() == core
                                && core.is_some();
                        if local_page(url) || to_core {
                            return true;
                        }
                        if matches!(url.scheme(), "http" | "https") {
                            let _ = opener.opener().open_url(url.as_str(), None::<&str>);
                        }
                        false
                    })
                    .build()?;

            let handle = app.handle().clone();
            let bundled = app
                .path()
                .resource_dir()
                .ok()
                .and_then(|res| bundled_config(&res, &data_dir));
            std::thread::spawn(move || {
                let started = match bundled {
                    Some(cfg) if !cfg.core_bin.is_file() => {
                        Err(LaunchError::CoreNotFound(vec![cfg.core_bin.clone()]))
                    }
                    Some(cfg) => launch(&cfg),
                    None => LaunchConfig::discover(&jarvis_home(), &data_dir)
                        .and_then(|cfg| launch(&cfg)),
                };
                match started {
                    Ok(core) => {
                        let url: url::Url = core.url.parse().expect("URL del Core");
                        *core_port.lock().unwrap_or_else(|e| e.into_inner()) = Some(core.port);
                        *handle
                            .state::<CoreState>()
                            .0
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(core);
                        let _ = window.navigate(url);
                    }
                    Err(e) => {
                        let message = serde_json::to_string(&e.to_string()).unwrap_or_default();
                        let _ =
                            window.eval(format!("window.showError && window.showError({message})"));
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("avvio dell'applicazione JARVIS");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(mut core) = handle
                .state::<CoreState>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                core.stop();
            }
        }
    });
}
