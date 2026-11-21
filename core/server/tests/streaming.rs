//! Chat in streaming (ADR-0025): worker Python sintetico che risponde a frammenti.
//! I frammenti arrivano al client come righe NDJSON prima dell'esito, identico a quello di `/api/chat`.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-streaming";
const PORT: u16 = 18767;

#[tokio::test]
async fn chat_stream_sends_deltas_then_the_same_final_result() {
    let data =
        std::env::temp_dir().join(format!("jarvis-stream-{}", uuid::Uuid::new_v4().simple()));
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![("JARVIS_TEST_STREAM".into(), "1".into())],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let app = api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT));
    let req = Request::builder()
        .method("POST")
        .uri("/api/chat/stream")
        .header("host", format!("127.0.0.1:{PORT}"))
        .header("content-type", "application/json")
        .header("x-jarvis-token", TOKEN)
        .body(Body::from(json!({"message": "presentati"}).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("application/x-ndjson"));
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let lines: Vec<Value> = String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let deltas: Vec<&str> = lines
        .iter()
        .filter(|l| l["type"] == "delta")
        .map(|l| l["text"].as_str().unwrap())
        .collect();
    assert_eq!(deltas, ["Ciao, ", "sono ", "JARVIS."], "{lines:?}");
    let done = lines.last().unwrap();
    assert_eq!(done["type"], "done", "{lines:?}");
    assert_eq!(done["result"]["reply"], "Ciao, sono JARVIS.");
    assert_eq!(done["result"]["task"]["state"], "succeeded");

    // La conversazione salvata e i token misurati sono quelli della risposta completa.
    let get = |uri: &str| {
        Request::builder()
            .uri(uri)
            .header("host", format!("127.0.0.1:{PORT}"))
            .header("x-jarvis-token", TOKEN)
            .body(Body::empty())
            .unwrap()
    };
    let conv = app
        .clone()
        .oneshot(get("/api/conversation?limit=5"))
        .await
        .unwrap();
    let conv: Value =
        serde_json::from_slice(&conv.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(conv.to_string().contains("Ciao, sono JARVIS."));
    let metrics = app.oneshot(get("/api/metrics")).await.unwrap();
    let metrics: Value =
        serde_json::from_slice(&metrics.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(metrics["counters"]["ai.tokens.prompt"], 40);
    assert_eq!(metrics["counters"]["ai.tokens.output"], 3);
    let _ = std::fs::remove_dir_all(&data);
}
