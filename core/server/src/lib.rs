//! Core JARVIS — prototipo v0.1.
//!
//! API locale (solo loopback, token di sessione), Orchestrator, Security Engine,
//! strumenti confinati nella workspace, memoria SQLite e audit a catena di hash.
//! L'intelligenza è nel worker Python (`ai/jarvis_ai`) raggiunto via `jarvis-bridge`.

pub mod agents;
pub mod api;
pub mod audit;
pub mod automation;
pub mod backup;
pub mod catalog;
pub mod channels;
pub mod config;
pub mod connectors;
pub mod memory;
pub mod node_agent;
pub mod nodes;
pub mod oauth;
pub mod observability;
pub mod orchestrator;
pub mod passkeys;
pub mod policy;
pub mod rollback;
pub mod secrets;
pub mod setup;

/// Su Windows i processi figli (worker Python, pip) non devono aprire una finestra di console quando il
/// Core è avviato dall'app desktop. Altrove non fa nulla.
pub fn hide_console(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}
pub mod store;
pub mod tls;
pub mod tools;
pub mod updates;
pub mod web;

/// Istante corrente in RFC 3339 UTC con millisecondi (`common.schema.json#/$defs/timestamp`).
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
