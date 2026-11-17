//! JARVIS desktop (RF-10): guscio Tauri sopra l'interfaccia servita dal Core.
//!
//! - La finestra parte da una pagina locale di attesa; il Core viene avviato su una porta di loopback
//!   libera con i dati nella cartella dell'applicazione dell'utente, poi la finestra naviga verso di esso.
//! - La navigazione è limitata alla pagina locale e al Core; gli altri indirizzi http(s) (per esempio le
//!   fonti Web) si aprono nel browser di sistema.
//! - Nessuna capability Tauri è concessa: l'interfaccia remota non ha accesso alle API native.
//! - Alla chiusura dell'app il Core viene terminato (il worker Python si chiude con il suo stdin).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use jarvis_launcher::{launch, LaunchConfig, RunningCore};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

struct CoreState(Mutex<Option<RunningCore>>);

/// Radice del repository: `JARVIS_HOME`, altrimenti la posizione del sorgente al momento della build.
fn jarvis_home() -> PathBuf {
    std::env::var_os("JARVIS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
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
            std::thread::spawn(move || {
                let started =
                    LaunchConfig::discover(&jarvis_home(), &data_dir).and_then(|cfg| launch(&cfg));
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
