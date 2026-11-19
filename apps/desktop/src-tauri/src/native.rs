//! Esperienza nativa dell'app desktop (ADR-0044): icona nella barra dei menu (macOS) o nell'area di notifica
//! (Windows, Linux), menu dell'applicazione su macOS, scorciatoia globale per richiamare JARVIS, finestra che
//! si nasconde alla chiusura invece di fermare l'assistente, notifiche native del sistema.

use std::collections::HashSet;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

/// Mostra e porta in primo piano la finestra principale.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Alterna la finestra: nascosta o in secondo piano -> in primo piano; in primo piano -> nascosta.
pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show(app);
        }
    }
}

/// Scorciatoia globale: ⌘⇧J su macOS, Ctrl+Maiusc+J altrove.
pub fn shortcut() -> Shortcut {
    let mods = if cfg!(target_os = "macos") {
        Modifiers::SUPER | Modifiers::SHIFT
    } else {
        Modifiers::CONTROL | Modifiers::SHIFT
    };
    Shortcut::new(Some(mods), Code::KeyJ)
}

pub fn shortcut_plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, sc, event| {
            if sc == &shortcut() && event.state() == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .build()
}

/// Icona della barra dei menu o dell'area di notifica con il menu essenziale.
pub fn tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let label = if cfg!(target_os = "macos") {
        "⌘⇧J"
    } else {
        "Ctrl+Maiusc+J"
    };
    let open = MenuItem::with_id(
        app,
        "open",
        format!("Apri JARVIS ({label})"),
        true,
        None::<&str>,
    )?;
    let hide = MenuItem::with_id(app, "hide", "Nascondi", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci da JARVIS", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &hide, &sep, &quit])?;
    let mut builder = TrayIconBuilder::with_id("jarvis")
        .tooltip("JARVIS")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "hide" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Menu dell'applicazione su macOS: voci standard (Modifica con copia e incolla, Finestra) più JARVIS.
#[cfg(target_os = "macos")]
pub fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    use tauri::menu::{AboutMetadata, Submenu};
    let about = PredefinedMenuItem::about(
        app,
        Some("Informazioni su JARVIS"),
        Some(AboutMetadata {
            name: Some("JARVIS".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            comments: Some("Assistente personale privato e locale".into()),
            ..Default::default()
        }),
    )?;
    let jarvis = Submenu::with_items(
        app,
        "JARVIS",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Nascondi JARVIS"))?,
            &PredefinedMenuItem::hide_others(app, Some("Nascondi altre"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Esci da JARVIS"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Modifica",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Annulla"))?,
            &PredefinedMenuItem::redo(app, Some("Ripeti"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("Taglia"))?,
            &PredefinedMenuItem::copy(app, Some("Copia"))?,
            &PredefinedMenuItem::paste(app, Some("Incolla"))?,
            &PredefinedMenuItem::select_all(app, Some("Seleziona tutto"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Finestra",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Riduci"))?,
            &PredefinedMenuItem::maximize(app, Some("Ingrandisci"))?,
            &PredefinedMenuItem::fullscreen(app, Some("Schermo intero"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Chiudi finestra"))?,
        ],
    )?;
    Menu::with_items(app, &[&jarvis, &edit, &window])
}

/// Notifiche native: legge le notifiche non lette del Core ogni 10 secondi e mostra quelle nuove quando la
/// finestra non è in primo piano (la WebView di macOS non offre le notifiche Web).
pub fn watch_notifications<R: Runtime>(app: AppHandle<R>, port: u16, token: String) {
    std::thread::spawn(move || {
        let http = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build();
        let url = format!("http://127.0.0.1:{port}/api/notifications?limit=10");
        let mut seen: HashSet<String> = HashSet::new();
        let mut first = true;
        loop {
            let items: Vec<serde_json::Value> = http
                .get(&url)
                .set("X-Jarvis-Token", &token)
                .call()
                .ok()
                .and_then(|r| r.into_json::<serde_json::Value>().ok())
                .and_then(|v| v["items"].as_array().cloned())
                .unwrap_or_default();
            let focused = app
                .get_webview_window("main")
                .and_then(|w| Some(w.is_visible().ok()? && w.is_focused().ok()?))
                .unwrap_or(false);
            for n in items.iter().filter(|n| n["read_at"].is_null()) {
                let id = n["notification_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                // Al primo giro si registrano le notifiche già presenti senza mostrarle.
                if seen.insert(id) && !first && !focused {
                    let _ = app
                        .notification()
                        .builder()
                        .title(n["title"].as_str().unwrap_or("JARVIS"))
                        .body(n["body"].as_str().unwrap_or_default())
                        .show();
                }
            }
            first = false;
            std::thread::sleep(Duration::from_secs(10));
        }
    });
}

/// Registra la scorciatoia globale; se è già usata da un'altra app JARVIS funziona comunque senza.
pub fn register_shortcut<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.global_shortcut().register(shortcut());
}
