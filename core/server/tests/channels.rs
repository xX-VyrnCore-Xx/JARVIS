//! Notifiche sul telefono (ADR-0026): canali ntfy e Telegram contro server sintetici locali.
//! Coprono configurazione, segreti mai restituiti, filtro per gravità, testo privato, prova d'invio e rimozione.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-canali";
const PORT: u16 = 18768;

#[derive(Clone, Debug)]
struct Received {
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// Server HTTP che registra ogni richiesta e risponde 200.
fn recorder() -> (String, Arc<Mutex<Vec<Received>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(vec![]));
    let sink = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut headers = vec![];
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((k, v)) = line.trim_end().split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            sink.lock().unwrap().push(Received {
                path: first
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string(),
                headers,
                body: String::from_utf8_lossy(&body).to_string(),
            });
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
        }
    });
    (url, log)
}

struct Env {
    app: axum::Router,
}

impl Env {
    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", format!("127.0.0.1:{PORT}"))
            .header("content-type", "application/json")
            .header("x-jarvis-token", TOKEN)
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
}

fn wait_for(log: &Arc<Mutex<Vec<Received>>>, n: usize) -> Vec<Received> {
    for _ in 0..100 {
        if log.lock().unwrap().len() >= n {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    log.lock().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn external_channels_receive_filtered_private_notifications() {
    let data =
        std::env::temp_dir().join(format!("jarvis-channels-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_var("JARVIS_SECRETS_KEYCHAIN", "off");
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
    let env = Env {
        app: api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT)),
    };
    let (ntfy_url, ntfy) = recorder();
    let (tg_url, telegram) = recorder();

    // Configurazioni non sicure rifiutate; Telegram richiede il token.
    let (status, _) = env
        .call(
            "POST",
            "/api/notify/channels",
            Some(json!({"kind": "ntfy", "target": "corto", "server": ntfy_url})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = env
        .call(
            "POST",
            "/api/notify/channels",
            Some(json!({"kind": "telegram", "target": "4242", "server": tg_url})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, ntfy_ch) = env
        .call("POST", "/api/notify/channels", Some(json!({"kind": "ntfy", "name": "Telefono", "target": "jarvis-test-7f3k2", "server": ntfy_url, "secret": "tk-ntfy-sintetico"})))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{ntfy_ch}");
    let (status, tg_ch) = env
        .call("POST", "/api/notify/channels", Some(json!({"kind": "telegram", "target": "4242", "server": tg_url, "secret": "123:bot-sintetico", "include_body": true, "min_severity": "info"})))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{tg_ch}");

    // I segreti non tornano mai al client.
    let (_, list) = env.call("GET", "/api/notify/channels", None).await;
    let text = list.to_string();
    assert!(
        !text.contains("tk-ntfy-sintetico") && !text.contains("bot-sintetico"),
        "{text}"
    );
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    assert!(list["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["has_secret"] == true));

    // Una conferma in attesa (warning) raggiunge entrambi i canali; ntfy senza il testo, Telegram con il testo.
    let (_, out) = env
        .call(
            "POST",
            "/api/chat",
            Some(json!({"message": "scrivi nota.txt: dato riservato"})),
        )
        .await;
    assert_eq!(out["task"]["state"], "waiting_confirmation", "{out}");
    let got = wait_for(&ntfy, 1);
    let msg = got.iter().find(|r| r.path == "/").expect("messaggio ntfy");
    let header = |name: &str| {
        msg.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(header("authorization"), "Bearer tk-ntfy-sintetico");
    let pushed: Value = serde_json::from_str(&msg.body).unwrap();
    assert_eq!(pushed["topic"], "jarvis-test-7f3k2");
    assert!(
        pushed["title"].as_str().unwrap().starts_with("JARVIS · "),
        "{pushed}"
    );
    assert_eq!(pushed["priority"], 4);
    assert!(
        !msg.body.contains("riservato"),
        "senza consenso il testo resta sul computer: {}",
        msg.body
    );
    let got = wait_for(&telegram, 1);
    let msg = &got[0];
    assert_eq!(msg.path, "/bot123:bot-sintetico/sendMessage");
    let body: Value = serde_json::from_str(&msg.body).unwrap();
    assert_eq!(body["chat_id"], "4242");
    assert!(body["text"].as_str().unwrap().starts_with("JARVIS · "));

    // Prova d'invio dall'interfaccia.
    let id = ntfy_ch["channel_id"].as_str().unwrap();
    let (status, out) = env
        .call("POST", &format!("/api/notify/channels/{id}/test"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert!(wait_for(&ntfy, 2)
        .iter()
        .any(|r| r.body.contains("Prova di notifica")));

    // Rimozione: canale e segreto spariscono, con audit.
    let (status, _) = env
        .call("DELETE", &format!("/api/notify/channels/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, list) = env.call("GET", "/api/notify/channels", None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let (status, _) = env
        .call("POST", &format!("/api/notify/channels/{id}/test"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, audit) = env.call("GET", "/api/audit?limit=50", None).await;
    let audit = audit.to_string();
    assert!(audit.contains("notify.channel.add") && audit.contains("notify.channel.delete"));
    let _ = std::fs::remove_dir_all(&data);
}
