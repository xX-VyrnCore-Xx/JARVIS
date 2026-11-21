//! Agenti temporanei (RF-03, ADR-0040) con il worker Python reale in modalità offline: task figlio con ruolo e
//! strumenti di sola lettura, stessa traccia del principale, nessuna azione che richieda conferma.

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
const PORT: u16 = 18776;

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
async fn temporary_agent_runs_a_subtask_in_the_same_trace() {
    let e = env();
    let out = e
        .chat("delega a un agente analista del tempo: che ore sono")
        .await;
    assert_eq!(out["task"]["state"], "succeeded", "{out}");
    let sub = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "subagent")
        .cloned()
        .unwrap();
    assert_eq!(sub["role"], "analista del tempo");
    assert_eq!(sub["status"], "succeeded", "{sub}");
    assert_eq!(sub["tools"][0]["tool"], "time.now");
    let reply = out["reply"].as_str().unwrap();
    assert!(
        reply.contains("L'agente temporaneo «analista del tempo» ha risposto"),
        "{reply}"
    );

    // Il task figlio è registrato con il principale come genitore e la stessa correlazione.
    let (_, tasks) = e.api("GET", "/api/tasks?limit=10", None).await;
    let child = tasks["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["task_id"] == sub["task_id"])
        .cloned()
        .unwrap();
    assert_eq!(child["parent_task_id"], out["task"]["task_id"]);
    assert_eq!(child["correlation_id"], out["task"]["correlation_id"]);
    assert_eq!(child["requester"]["kind"], "agent");
    assert_eq!(child["state"], "succeeded");

    // Una sola traccia: la radice del figlio è figlia della radice del principale.
    let id = out["task"]["task_id"].as_str().unwrap();
    let (_, trace) = e.api("GET", &format!("/api/tasks/{id}/trace"), None).await;
    let spans = trace["spans"].as_array().unwrap();
    let root = spans
        .iter()
        .find(|s| s["kind"] == "task" && s["parent_span_id"].is_null())
        .unwrap();
    let child_root = spans
        .iter()
        .find(|s| s["name"] == "agente temporaneo")
        .expect("radice del figlio");
    assert_eq!(child_root["parent_span_id"], root["span_id"], "{trace}");
    assert!(spans.iter().all(|s| s["trace_id"] == trace["trace_id"]));
    assert_eq!(trace["total_ms"], root["duration_ms"]);
    let _ = std::fs::remove_dir_all(&e.data);
}

#[tokio::test]
async fn temporary_agent_cannot_write_or_ask_for_confirmation() {
    let e = env();
    let out = e
        .chat("delega a un agente scrittore: scrivi segreto.txt: ciao")
        .await;
    assert!(
        out["confirmation"].is_null(),
        "nessuna conferma da un agente temporaneo: {out}"
    );
    let sub = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "subagent")
        .cloned()
        .unwrap();
    // fs.write non è tra gli strumenti dell'agente temporaneo: il worker non lo propone e il Core non lo esegue.
    assert!(
        sub["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["tool"] != "fs.write" || t["status"] == "denied"),
        "{sub}"
    );
    assert!(!e.data.join("workspace/segreto.txt").exists());
    let _ = std::fs::remove_dir_all(&e.data);
}
