//! JARVIS desktop (RF-10): guscio Tauri sopra l'interfaccia servita dal Core.
//!
//! - La finestra parte da una pagina locale di attesa; il Core viene avviato su una porta di loopback
//!   libera con i dati nella cartella dell'applicazione dell'utente, poi la finestra naviga verso di esso.
//! - La navigazione è limitata alla pagina locale e al Core; gli altri indirizzi http(s) (per esempio le
//!   fonti Web) si aprono nel browser di sistema.
//! - Nessuna capability Tauri è concessa: l'interfaccia remota non ha accesso alle API native.
//! - Chiudere la finestra la nasconde: JARVIS resta attivo nella barra dei menu o nell'area di notifica e
//!   torna con ⌘⇧J / Ctrl+Maiusc+J (ADR-0044). Uscendo dall'app («Esci da JARVIS») il Core viene terminato
//!   (il worker Python si chiude con il suo stdin).
//! - Installata con il programma di setup (ADR-0016), l'app usa il Core incluso accanto all'eseguibile
//!   (`jarvis-core`), il runtime AI e i contratti tra le risorse e un Python autonomo incluso: non serve
//!   installare altro.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native;

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

/// macOS: dopo che il proprietario ha autorizzato l'apertura dell'app (ADR-0020, firma ad hoc), toglie l'attributo
/// di quarantena ereditato dal .dmg dai file del pacchetto, così il Core e il Python inclusi non vengono bloccati
/// da Gatekeeper come programmi scaricati. Nessun effetto se l'app gira dal .dmg o da una copia traslocata.
#[cfg(target_os = "macos")]
fn clear_quarantine() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(bundle) = exe.ancestors().nth(3) else {
        return;
    };
    let path = bundle.to_string_lossy();
    if !path.ends_with(".app")
        || path.contains("/AppTranslocation/")
        || path.starts_with("/Volumes/")
    {
        return;
    }
    let _ = std::process::Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(bundle)
        .status();
}

fn main() {
    let app = tauri::Builder::default()
        // Una seconda apertura porta in primo piano la finestra esistente invece di avviare un altro Core.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            native::show(app)
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(native::shortcut_plugin())
        .plugin(tauri_plugin_notification::init())
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
            let opener_new = app.handle().clone();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("JARVIS")
                    .inner_size(1200.0, 820.0)
                    .min_inner_size(380.0, 520.0)
                    .theme(Some(tauri::Theme::Dark))
                    .background_color(tauri::webview::Color(10, 17, 28, 255))
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
                    // Collegamenti con target=_blank (es. il download di Ollama): browser di sistema, mai una
                    // nuova finestra dell'app.
                    .on_new_window(move |url, _features| {
                        if matches!(url.scheme(), "http" | "https") {
                            let _ = opener_new.opener().open_url(url.as_str(), None::<&str>);
                        }
                        tauri::webview::NewWindowResponse::Deny
                    });
            // macOS: barra del titolo trasparente sullo sfondo scuro della finestra, senza testo (ADR-0036).
            #[cfg(target_os = "macos")]
            let window = window
                .title_bar_style(tauri::TitleBarStyle::Transparent)
                .hidden_title(true);
            let window = window.build()?;
            #[cfg(target_os = "macos")]
            clear_quarantine();
            #[cfg(target_os = "macos")]
            app.set_menu(native::app_menu(app.handle())?)?;
            native::tray(app.handle())?;
            native::register_shortcut(app.handle());
            // Chiudere la finestra la nasconde: l'assistente resta attivo (voce, automazioni, notifiche).
            let hider = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hider.hide();
                }
            });

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
                        let mut url: url::Url = core.url.parse().expect("URL del Core");
                        // La dashboard applica le rifiniture dell'app desktop (ADR-0035).
                        url.query_pairs_mut().append_pair("app", "desktop");
                        *core_port.lock().unwrap_or_else(|e| e.into_inner()) = Some(core.port);
                        native::watch_notifications(handle.clone(), core.port, core.token.clone());
                        *handle
                            .state::<CoreState>()
                            .0
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(core);
                        let _ = window.navigate(url);
                    }
                    Err(e) => {
                        let message = serde_json::to_string(&e.to_string()).unwrap_or_default();
                        let _ = window.eval(format!(
                            "window.jarvisStartupError = {message}; window.showError?.(window.jarvisStartupError);"
                        ));
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("avvio dell'applicazione JARVIS");

    app.run(|handle, event| {
        // macOS: clic sull'icona nel Dock con la finestra nascosta.
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            native::show(handle);
        }
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
