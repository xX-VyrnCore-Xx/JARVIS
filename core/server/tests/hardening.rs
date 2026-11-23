//! Indurimento del backend (ADR-0099): permessi dei dispositivi sulle rotte nuove, filtro e limite dei
//! tentativi di accesso remoti, flussi in tempo reale limitati, token del proprietario.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const PORT: u16 = 18787;
const REMOTE: &str = "jarvis-hardening.esempio.ts.net";

struct TestApp {
    app: Router,
    data: PathBuf,
}

fn app() -> TestApp {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!(".hardening-test-{}", uuid::Uuid::new_v4().simple()));
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "owner-session-token".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    TestApp {
        app: api::router(api::state_with_remote(
            Jarvis::new(cfg).unwrap(),
            "owner-session-token".into(),
            PORT,
            vec![REMOTE.into()],
        )),
        data,
    }
}

impl TestApp {
    fn cleanup(self) {
        let lock = self.data.with_file_name(format!(
            ".{}.jarvis-lock",
            self.data.file_name().unwrap().to_string_lossy()
        ));
        drop(self.app);
        std::fs::remove_dir_all(self.data).unwrap();
        let _ = std::fs::remove_file(lock);
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    host: &str,
    auth: Option<(&str, &str)>,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", host);
    if let Some((name, value)) = auth {
        request = request.header(name, value);
    }
    let request = match body {
        Some(value) => request
            .header("content-type", "application/json")
            .body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::Null),
        text,
    )
}

fn local() -> String {
    format!("127.0.0.1:{PORT}")
}

async fn pair(app: &Router, scopes: Value) -> (String, String) {
    let (_, invite, _) = call(
        app,
        "POST",
        "/api/devices/pairing",
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        Some(json!({"scopes": scopes})),
    )
    .await;
    let code = invite["code"].as_str().unwrap();
    let (status, paired, _) = call(
        app,
        "POST",
        "/api/devices/pair",
        REMOTE,
        None,
        Some(json!({"code": code, "name": "Telefono", "platform": "android"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paired}");
    (
        paired["token"].as_str().unwrap().to_string(),
        paired["device"]["device_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn chat_devices_reach_the_live_stream_but_not_owner_only_routes() {
    let t = app();
    let (token, _) = pair(&t.app, json!(["chat"])).await;
    let device = ("x-jarvis-device", token.as_str());
    let request = Request::builder()
        .uri("/api/notifications/stream")
        .header("host", REMOTE)
        .header("x-jarvis-device", token.as_str())
        .body(Body::empty())
        .unwrap();
    let response = t.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    drop(response);
    assert_eq!(
        status,
        StatusCode::OK,
        "il flusso è consentito a un dispositivo con permesso chat"
    );
    for (method, uri) in [
        ("GET", "/api/missions"),
        ("POST", "/api/missions"),
        ("POST", "/api/missions/abcd1234/run"),
        ("PATCH", "/api/proactive"),
        ("POST", "/api/proactive/briefing"),
        ("GET", "/api/push/status"),
        ("GET", "/api/skills"),
        ("GET", "/api/memory"),
    ] {
        let (status, body, _) =
            call(&t.app, method, uri, REMOTE, Some(device), Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
        assert_eq!(body["error"]["code"], "owner_only");
    }
    t.cleanup();
}

#[tokio::test]
async fn malformed_and_unknown_remote_tokens_are_rejected_then_throttled_without_blocking_the_owner(
) {
    let t = app();
    let (good, _) = pair(&t.app, json!(["chat"])).await;
    // Token malformato: respinto subito.
    for bad in ["", "corto", &"z".repeat(64), &"a".repeat(65)] {
        let (status, _, _) = call(
            &t.app,
            "GET",
            "/api/tasks",
            REMOTE,
            Some(("x-jarvis-device", bad)),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{bad:?}");
    }
    // Token ben formati ma sconosciuti: dopo 30 errori scatta il limite.
    let mut statuses = Vec::new();
    for i in 0..40 {
        let guess = format!("{i:064x}");
        let (status, _, _) = call(
            &t.app,
            "GET",
            "/api/tasks",
            REMOTE,
            Some(("x-jarvis-device", &guess)),
            None,
        )
        .await;
        statuses.push(status);
    }
    assert!(
        statuses[..20]
            .iter()
            .all(|s| *s == StatusCode::UNAUTHORIZED),
        "{statuses:?}"
    );
    assert_eq!(statuses[39], StatusCode::TOO_MANY_REQUESTS, "{statuses:?}");
    let (_, _, text) = call(
        &t.app,
        "GET",
        "/api/tasks",
        REMOTE,
        Some(("x-jarvis-device", &good)),
        None,
    )
    .await;
    assert!(
        text.contains("troppi tentativi"),
        "durante l'attacco anche i dispositivi attendono: {text}"
    );
    // Il proprietario sul canale locale non è toccato.
    let (status, _, _) = call(
        &t.app,
        "GET",
        "/api/tasks",
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    t.cleanup();
}

#[tokio::test]
async fn the_owner_token_must_match_exactly() {
    let t = app();
    for wrong in [
        "",
        "owner-session-toke",
        "owner-session-tokenX",
        "OWNER-SESSION-TOKEN",
        "owner-session-token ",
    ] {
        let (status, _, _) = call(
            &t.app,
            "GET",
            "/api/tasks",
            &local(),
            Some(("x-jarvis-token", wrong)),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{wrong:?}");
    }
    let (status, _, _) = call(&t.app, "GET", "/api/tasks", &local(), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Un host non ammesso (rebinding DNS) è respinto prima ancora del token.
    let (status, _, _) = call(
        &t.app,
        "GET",
        "/api/tasks",
        "evil.example",
        Some(("x-jarvis-token", "owner-session-token")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    t.cleanup();
}

#[tokio::test]
async fn live_streams_are_capped() {
    let t = app();
    let owner = Some(("x-jarvis-token", "owner-session-token"));
    let mut open = Vec::new();
    for _ in 0..16 {
        let request = Request::builder()
            .uri("/api/notifications/stream")
            .header("host", local())
            .header("x-jarvis-token", "owner-session-token")
            .body(Body::empty())
            .unwrap();
        let response = t.app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        open.push(response);
    }
    let (status, body, _) = call(
        &t.app,
        "GET",
        "/api/notifications/stream",
        &local(),
        owner,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    // Chiusa una connessione, il posto si libera.
    open.pop();
    let (status, _, _) = call(&t.app, "GET", "/api/health", &local(), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let request = Request::builder()
        .uri("/api/notifications/stream")
        .header("host", local())
        .header("x-jarvis-token", "owner-session-token")
        .body(Body::empty())
        .unwrap();
    let response = t.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "un posto si è liberato");
    drop(open);
    drop(response);
    t.cleanup();
}
