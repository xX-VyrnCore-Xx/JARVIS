//! Processi e terminale (RF-05, ADR-0038): lettura dei processi senza conferma, comandi del terminale solo dopo
//! la conferma del proprietario, comandi distruttivi negati, nessun comando nelle automazioni.
//! Richiede `python3` con `jsonschema` (tests/contracts/requirements.txt).

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-di-test";
const PORT: u16 = 18775;

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    std::env::set_var("JARVIS_SHELL", "on");
    let data = std::env::temp_dir().join(format!("jarvis-test-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let core = Jarvis::new(cfg).expect("core");
    Env {
        app: api::router(api::state(core, TOKEN.into(), PORT)),
        data,
    }
}

impl Env {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
        host: &str,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", host)
            .header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("x-jarvis-token", t);
        }
        let req = req
            .body(
                body.map(|b| Body::from(b.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn api(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(
            method,
            path,
            body,
            Some(TOKEN),
            &format!("127.0.0.1:{PORT}"),
        )
        .await
    }

    async fn chat(&self, message: &str) -> Value {
        let (status, out) = self
            .api("POST", "/api/chat", Some(json!({"message": message})))
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        assert!(
            jarvis_contracts::validate(jarvis_contracts::Contract::Task, &out["task"]).is_valid(),
            "task non conforme: {out}"
        );
        out
    }
}

#[tokio::test]
async fn processes_are_listed_without_confirmation() {
    let e = env();
    let out = e.chat("quali processi usano più memoria?").await;
    assert_eq!(out["task"]["state"], "succeeded", "{out}");
    assert!(out["confirmation"].is_null());
    let reply = out["reply"].as_str().unwrap();
    assert!(reply.contains("Processi con più uso di memoria"), "{reply}");
    assert!(reply.contains("pid"), "{reply}");
    let _ = std::fs::remove_dir_all(&e.data);
}

#[tokio::test]
async fn terminal_command_runs_only_after_confirmation_in_the_workspace() {
    let e = env();
    let out = e
        .chat("terminale: echo ciao-dal-terminale > risultato.txt")
        .await;
    assert_eq!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert!(out["confirmation"]["expected_effect"]
        .as_str()
        .unwrap()
        .contains("echo ciao-dal-terminale"));
    let file = e.data.join("workspace/risultato.txt");
    assert!(!file.exists(), "nessun effetto prima della conferma");
    let id = out["confirmation"]["confirmation_id"].as_str().unwrap();
    let (status, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["task"]["state"], "succeeded", "{done}");
    assert!(
        done["reply"].as_str().unwrap().contains("codice 0"),
        "{done}"
    );
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("ciao-dal-terminale"));
    let (_, audit) = e.api("GET", "/api/audit?limit=20", None).await;
    assert!(
        audit.to_string().contains("shell.run"),
        "comando registrato nell'audit"
    );
    let _ = std::fs::remove_dir_all(&e.data);
}

#[tokio::test]
async fn destructive_commands_are_denied_without_asking() {
    let e = env();
    for cmd in [
        "sudo rm -rf /tmp/x",
        "rm -rf / --no-preserve-root",
        "shutdown -h now",
    ] {
        let out = e.chat(&format!("terminale: {cmd}")).await;
        assert!(out["confirmation"].is_null(), "{cmd}: {out}");
        let policy = out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "policy")
            .unwrap();
        assert_eq!(policy["decision"], "deny", "{cmd}: {out}");
        assert_eq!(policy["reasons"][0], "policy.command_blocked");
    }
    let _ = std::fs::remove_dir_all(&e.data);
}

#[tokio::test]
async fn automations_cannot_contain_terminal_commands() {
    let e = env();
    let (status, out) = e
        .api(
            "POST",
            "/api/workflows",
            Some(
                json!({"name": "Pulizia", "trigger": {"type": "interval", "every_seconds": 3600},
                        "steps": [{"tool": "shell.run", "arguments": {"command": "ls"}}]}),
            ),
        )
        .await;
    assert_ne!(status, StatusCode::OK, "{out}");
    assert!(out.to_string().contains("terminale"), "{out}");
    let _ = std::fs::remove_dir_all(&e.data);
}
