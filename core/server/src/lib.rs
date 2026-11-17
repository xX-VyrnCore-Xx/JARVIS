//! Core JARVIS — prototipo v0.1.
//!
//! API locale (solo loopback, token di sessione), Orchestrator, Security Engine,
//! strumenti confinati nella workspace, memoria SQLite e audit a catena di hash.
//! L'intelligenza è nel worker Python (`ai/jarvis_ai`) raggiunto via `jarvis-bridge`.

pub mod api;
pub mod audit;
pub mod catalog;
pub mod config;
pub mod orchestrator;
pub mod policy;
pub mod store;
pub mod tools;

/// Istante corrente in RFC 3339 UTC con millisecondi (`common.schema.json#/$defs/timestamp`).
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
