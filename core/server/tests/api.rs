//! Test d'integrazione dell'API con il worker Python reale in modalità offline (deterministica).
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
const PORT: u16 = 18765;

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    let data = std::env::temp_dir().join(format!("jarvis-test-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        ai_backend: Some("offline".into()),
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

fn kinds(out: &Value) -> Vec<String> {
    out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn reply_without_tools() {
    let e = env();
    let out = e.chat("ciao").await;
    assert_eq!(out["task"]["state"], "succeeded");
    assert!(out["reply"].as_str().unwrap().contains("JARVIS"));
    assert_eq!(out["backend"]["name"], "offline");
    assert_eq!(kinds(&out), ["delegation"]);
}

#[tokio::test]
async fn low_risk_tool_runs_without_confirmation() {
    let e = env();
    let out = e.chat("che ore sono").await;
    assert_eq!(
        kinds(&out),
        ["delegation", "tool_call", "policy", "tool_result"]
    );
    assert_eq!(out["steps"][2]["decision"], "allow");
    assert!(out["reply"].as_str().unwrap().starts_with("Sono le"));
}

#[tokio::test]
async fn high_risk_write_requires_single_use_confirmation() {
    let e = env();
    let out = e.chat("scrivi note.txt: contenuto sintetico").await;
    assert_eq!(out["task"]["state"], "waiting_confirmation");
    assert!(
        !e.data.join("workspace/note.txt").exists(),
        "nessuna scrittura prima della conferma"
    );
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (_, pending) = e.api("GET", "/api/confirmations", None).await;
    assert_eq!(pending["items"].as_array().unwrap().len(), 1);

    let (status, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["task"]["state"], "succeeded");
    assert_eq!(
        std::fs::read_to_string(e.data.join("workspace/note.txt")).unwrap(),
        "contenuto sintetico"
    );

    let (reuse, _) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(
        reuse,
        StatusCode::NOT_FOUND,
        "una conferma vale una sola volta"
    );

    let read = e.chat("leggi note.txt").await;
    assert!(read["reply"]
        .as_str()
        .unwrap()
        .contains("contenuto sintetico"));
}

#[tokio::test]
async fn rejected_confirmation_cancels_without_effects() {
    let e = env();
    let out = e.chat("scrivi altro.txt: da non scrivere").await;
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": false})),
        )
        .await;
    assert_eq!(done["task"]["state"], "cancelled");
    assert!(!e.data.join("workspace/altro.txt").exists());
}

#[tokio::test]
async fn path_traversal_is_denied_by_policy() {
    let e = env();
    for message in ["leggi ../../etc/passwd", "leggi /etc/passwd"] {
        let out = e.chat(message).await;
        let policy = out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "policy")
            .unwrap();
        assert_eq!(policy["decision"], "deny", "{message}");
        assert_eq!(policy["reasons"][0], "policy.path_outside_workspace");
        assert!(
            !kinds(&out).contains(&"tool_result".to_string()),
            "nessuna esecuzione dopo un diniego"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_is_blocked_at_execution() {
    let e = env();
    std::os::unix::fs::symlink("/etc", e.data.join("workspace/esterno")).unwrap();
    let out = e.chat("leggi esterno/hostname").await;
    let result = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "tool_result")
        .unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["error"], "fs.outside_workspace");
}

#[tokio::test]
async fn memory_round_trip_and_owner_deletion() {
    let e = env();
    let saved = e.chat("ricorda che il colore preferito è il blu").await;
    assert_eq!(saved["steps"][2]["decision"], "allow");
    let (_, mem) = e.api("GET", "/api/memory", None).await;
    let item = &mem["items"][0];
    assert_eq!(item["text"], "il colore preferito è il blu");

    let recall = e.chat("colore preferito?").await;
    assert!(
        recall["reply"].as_str().unwrap().contains("blu"),
        "la memoria entra nel contesto: {recall}"
    );

    let forget = e
        .chat(&format!("dimentica {}", item["id"].as_str().unwrap()))
        .await;
    assert_eq!(
        forget["task"]["state"], "waiting_confirmation",
        "cancellare la memoria via agente richiede conferma"
    );

    let (status, _) = e
        .api(
            "DELETE",
            &format!("/api/memory/{}", item["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // Restano solo gli episodi delle attività (ADR-0022), che non contengono il fatto cancellato.
    let (_, mem) = e.api("GET", "/api/memory?kind=semantic", None).await;
    assert!(mem["items"].as_array().unwrap().is_empty());
    let (_, all) = e.api("GET", "/api/memory", None).await;
    assert!(!all.to_string().contains("blu"), "{all}");
}

#[tokio::test]
async fn audit_chain_records_every_decision() {
    let e = env();
    e.chat("che ore sono").await;
    let out = e.chat("scrivi a.txt: x").await;
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    e.api(
        "POST",
        &format!("/api/confirmations/{id}"),
        Some(json!({"approve": true})),
    )
    .await;
    let (_, audit) = e.api("GET", "/api/audit", None).await;
    assert_eq!(audit["chain_valid"], true);
    let outcomes: Vec<&str> = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|e| e["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(
        outcomes,
        ["allowed", "succeeded", "allowed", "confirmed", "succeeded"]
    );

    // Manomissione del file: la verifica la rileva.
    let path = e.data.join("audit/core-security.jsonl");
    let tampered =
        std::fs::read_to_string(&path)
            .unwrap()
            .replacen("\"succeeded\"", "\"failed\"", 1);
    std::fs::write(&path, tampered).unwrap();
    let (_, audit) = e.api("GET", "/api/audit", None).await;
    assert_eq!(audit["chain_valid"], false);
}

#[tokio::test]
async fn api_requires_token_and_local_host() {
    let e = env();
    let host = format!("127.0.0.1:{PORT}");
    let (s, _) = e
        .call(
            "POST",
            "/api/chat",
            Some(json!({"message": "ciao"})),
            None,
            &host,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = e
        .call("GET", "/api/memory", None, Some("sbagliato"), &host)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = e
        .call(
            "GET",
            "/api/health",
            None,
            Some(TOKEN),
            &format!("attacker.example:{PORT}"),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "difesa dal DNS rebinding");
    let (s, health) = e.call("GET", "/api/health", None, None, &host).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(health["ai"]["status"], "up");
}

#[tokio::test]
async fn invalid_input_is_rejected() {
    let e = env();
    let (s, _) = e
        .api("POST", "/api/chat", Some(json!({"message": "   "})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .api(
            "POST",
            "/api/chat",
            Some(json!({"message": "x".repeat(8001)})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .api(
            "POST",
            "/api/confirmations/inesistente",
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn index_embeds_session_token_and_csp() {
    let e = env();
    let req = Request::builder()
        .uri("/")
        .header("host", format!("localhost:{PORT}"))
        .body(Body::empty())
        .unwrap();
    let res = e.app.clone().oneshot(req).await.unwrap();
    assert!(res.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("script-src 'self'"));
    let html =
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains(TOKEN));
}

#[tokio::test]
async fn conversation_and_tasks_persist_across_restart() {
    let e = env();
    e.chat("ciao").await;
    drop(e.app);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: e.data.clone(),
        python: "python3".into(),
        ai_dir: root.join("ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let again = Env {
        app: api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT)),
        data: e.data.clone(),
    };
    let (_, conv) = again.api("GET", "/api/conversation", None).await;
    assert_eq!(conv["items"].as_array().unwrap().len(), 2);
    let (_, tasks) = again.api("GET", "/api/tasks", None).await;
    assert_eq!(tasks["items"][0]["state"], "succeeded");
}
