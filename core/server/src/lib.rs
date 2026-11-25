//! Core JARVIS — prototipo v0.1.
//!
//! API locale (solo loopback, token di sessione), Orchestrator, Security Engine,
//! strumenti confinati nella workspace, memoria SQLite e audit a catena di hash.
//! L'intelligenza è nel worker Python (`ai/jarvis_ai`) raggiunto via `jarvis-bridge`.

pub mod accounts;
pub mod agents;
pub mod api;
pub mod appcontrol;
pub mod assets;
pub mod audit;
pub mod automation;
pub mod backup;
pub mod browser;
pub mod catalog;
pub mod channels;
mod chat_crypto;
pub mod cloud;
pub mod config;
pub mod connector_catalog;
pub mod connectors;
mod container_runtime;
pub mod dashboard;
pub mod datatools;
pub mod devtools;
pub mod docs;
pub mod executive;
pub mod expert;
pub mod guard;
pub mod handoff;
pub mod improvements;
pub mod intent;
pub mod mcp;
pub mod mcp_client;
pub mod memcog;
pub mod memory;
pub mod missions;
pub mod n8n;
pub mod net;
pub mod node_agent;
pub mod nodes;
pub mod oauth;
pub mod observability;
pub mod openapi;
pub mod orchestrator;
pub mod passkeys;
pub mod perf;
pub mod permissions;
pub mod persona;
pub mod planner;
pub mod plans;
pub mod plugins;
pub mod policy;
pub mod privacy;
pub mod proactive;
pub mod profile;
pub mod promptfix;
pub mod prompts;
pub mod push;
pub mod reflect;
pub mod rollback;
pub mod screen;
pub mod secrets;
pub mod selfupdate;
pub mod setup;
pub mod skills;
pub mod speech;
pub mod sysstats;
pub mod tailscale;

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
pub mod supervisor;
pub mod tls;
pub mod toolbox;
pub mod tools;
pub mod toolsys;
pub mod updates;
pub mod watch;
pub mod web;
pub mod workspaces;
pub mod world;

/// Istante corrente in RFC 3339 UTC con millisecondi (`common.schema.json#/$defs/timestamp`).
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

mod desktop;
pub mod sandbox;
