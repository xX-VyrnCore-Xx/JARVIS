//! Chat in streaming (ADR-0025): worker Python reale contro un Ollama sintetico che risponde a frammenti.
//! I frammenti arrivano al client come righe NDJSON prima dell'esito, identico a quello di `/api/chat`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
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

/// Ollama minimo: `/api/tags` con un modello, `/api/chat` in streaming con tre frammenti.
fn fake_ollama() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let payload = if first.starts_with("GET /api/tags") {
                json!({"models": [{"name": "qwen2.5:3b"}]}).to_string()
            } else {
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                assert_eq!(req["stream"], true, "il worker chiede lo streaming");
                [
                    json!({"message": {"role": "assistant", "content": "Ciao, "}, "done": false}),
                    json!({"message": {"role": "assistant", "content": "sono "}, "done": false}),
                    json!({"message": {"role": "assistant", "content": "JARVIS."}, "done": false}),
                    json!({"message": {"role": "assistant", "content": ""}, "done": true, "prompt_eval_count": 40, "eval_count": 3}),
                ]
                .iter()
                .map(|c| c.to_string() + "\n")
                .collect()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
        }
    });
    url
}

#[tokio::test]
async fn chat_stream_sends_deltas_then_the_same_final_result() {
    let ollama = fake_ollama();
    let data =
        std::env::temp_dir().join(format!("jarvis-stream-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        ai_backend: Some("ollama".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![("JARVIS_OLLAMA_URL".into(), ollama)],
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
