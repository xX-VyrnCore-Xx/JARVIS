use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use sha2::Digest;
use tower::ServiceExt;

const PORT: u16 = 18773;
const REMOTE: &str = "jarvis-push.esempio.ts.net";

struct TestApp {
    app: Router,
    data: PathBuf,
}

fn app() -> TestApp {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!(".push-test-{}", uuid::Uuid::new_v4().simple()));
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
async fn owner_and_chat_device_manage_only_their_own_encrypted_tokens() {
    let env = app();
    let owner_token = "fcm:owner_token_123456789012345";
    let device_token = "fcm:device_token_12345678901234";
    let (device_auth, device_id) = pair(&env.app, json!(["chat"])).await;
    let device_header = ("x-jarvis-device", device_auth.as_str());

    let (status, _, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        &local(),
        None,
        Some(json!({"token": owner_token, "platform": "web"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, owner_result, response_text) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        Some(json!({"token": owner_token, "platform": "web"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{owner_result}");
    assert_eq!(owner_result["registered"], true);
    assert!(
        ["ready", "configuration_missing", "configuration_invalid"]
            .contains(&owner_result["delivery_status"].as_str().unwrap()),
        "stato esplicito senza esporre la configurazione: {owner_result}"
    );
    if std::env::var_os("JARVIS_FCM_SERVICE_ACCOUNT_FILE").is_none() {
        assert_eq!(owner_result["delivery_status"], "configuration_missing");
    }
    assert!(!response_text.contains(owner_token));

    let raw = std::fs::read(env.data.join("jarvis.sqlite3")).unwrap();
    assert!(
        !String::from_utf8_lossy(&raw).contains(owner_token),
        "il token deve essere cifrato nell'archivio"
    );
    let (conn, _, platform): (String, String, String) =
        rusqlite::Connection::open(env.data.join("jarvis.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT token_hash, owner_id, platform FROM push_subscriptions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!(conn.len(), 64);
    assert_eq!(platform, "web");
    assert_ne!(conn, owner_token);
    assert_eq!(
        conn,
        hex::encode(sha2::Sha256::digest(owner_token.as_bytes()))
    );

    let (status, _, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        REMOTE,
        None,
        Some(json!({"token": device_token, "platform": "android"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, device_result, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        REMOTE,
        Some(device_header),
        Some(json!({"token": device_token, "platform": "android"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{device_result}");

    let (status, _, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        Some(json!({"token": device_token, "platform": "android"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "token già legato al device"
    );

    let (status, removed, _) = call(
        &env.app,
        "DELETE",
        "/api/push/subscriptions",
        REMOTE,
        Some(device_header),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["removed"], 1);
    let (status, owner_removed, _) = call(
        &env.app,
        "DELETE",
        "/api/push/subscriptions",
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{owner_removed}");
    assert_eq!(owner_removed["removed"], 1);
    assert_eq!(
        rusqlite::Connection::open(env.data.join("jarvis.sqlite3"))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM push_subscriptions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );

    // Anche la revoca del dispositivo elimina ogni token residuo associato.
    let (status, _, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        REMOTE,
        Some(device_header),
        Some(json!({"token": device_token, "platform": "android"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = call(
        &env.app,
        "POST",
        &format!("/api/devices/{device_id}/revoke"),
        &local(),
        Some(("x-jarvis-token", "owner-session-token")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        rusqlite::Connection::open(env.data.join("jarvis.sqlite3"))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM push_subscriptions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
    env.cleanup();
}

#[tokio::test]
async fn invalid_tokens_and_non_chat_devices_cannot_register() {
    let env = app();
    for (token, platform) in [
        ("short", "android"),
        ("fcm:token with whitespace_1234567890", "web"),
        ("fcm:valid_token_123456789012345", "ios"),
    ] {
        let (status, _, _) = call(
            &env.app,
            "POST",
            "/api/push/subscriptions",
            &local(),
            Some(("x-jarvis-token", "owner-session-token")),
            Some(json!({"token": token, "platform": platform})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (device_auth, _) = pair(&env.app, json!(["node"])).await;
    let (status, _, _) = call(
        &env.app,
        "POST",
        "/api/push/subscriptions",
        REMOTE,
        Some(("x-jarvis-device", &device_auth)),
        Some(json!({"token": "fcm:valid_token_123456789012345", "platform": "android"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    env.cleanup();
}
