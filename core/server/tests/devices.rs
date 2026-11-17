//! Test di RF-11: associazione di un dispositivo remoto con codice monouso, permessi limitati, revoca immediata.
//! Le richieste remote arrivano con l'header Host del proxy fidato (es. `tailscale serve`).

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

const PORT: u16 = 18771;
const REMOTE: &str = "jarvis.esempio.ts.net";

fn app() -> Router {
    let data = std::env::temp_dir().join(format!("jarvis-dev-{}", uuid::Uuid::new_v4().simple()));
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data,
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    api::router(api::state_with_remote(
        Jarvis::new(cfg).unwrap(),
        "t".into(),
        PORT,
        vec![REMOTE.into()],
    ))
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    host: &str,
    auth: Option<(&str, &str)>,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", host);
    if let Some((k, v)) = auth {
        b = b.header(k, v);
    }
    let req = match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
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
    let (_, code, _) = call(
        app,
        "POST",
        "/api/devices/pairing",
        &local(),
        Some(("x-jarvis-token", "t")),
        Some(json!({"scopes": scopes})),
    )
    .await;
    let code = code["code"].as_str().unwrap().to_string();
    let (status, paired, _) = call(app, "POST", "/api/devices/pair", REMOTE, None, Some(json!({"code": format!("{}-{}", &code[..4], &code[4..]).to_lowercase(), "name": "Telefono", "platform": "android"}))).await;
    assert_eq!(status, StatusCode::OK, "{paired}");
    (
        paired["token"].as_str().unwrap().to_string(),
        paired["device"]["device_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn remote_page_has_no_session_token_and_unknown_hosts_are_refused() {
    let app = app();
    let (status, _, html) = call(&app, "GET", "/", REMOTE, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(r#"name="jarvis-token" content="""#),
        "token di sessione non esposto da remoto"
    );
    let (_, _, html) = call(&app, "GET", "/", &local(), None, None).await;
    assert!(html.contains(r#"name="jarvis-token" content="t""#));
    let (status, _, _) = call(&app, "GET", "/", "attaccante.example", None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Il token di sessione non vale da remoto: serve un dispositivo associato.
    let (status, _, _) = call(
        &app,
        "GET",
        "/api/tasks",
        REMOTE,
        Some(("x-jarvis-token", "t")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for path in [
        "/manifest.webmanifest",
        "/sw.js",
        "/icon.svg",
        "/pair",
        "/pair.js",
    ] {
        assert_eq!(
            call(&app, "GET", path, REMOTE, None, None).await.0,
            StatusCode::OK,
            "{path}"
        );
    }
}

#[tokio::test]
async fn paired_device_chats_within_its_scopes_and_revocation_is_immediate() {
    let app = app();
    let (token, id) = pair(&app, json!(["chat"])).await;
    let dev = Some(("x-jarvis-device", token.as_str()));

    let (status, out, _) = call(
        &app,
        "POST",
        "/api/chat",
        REMOTE,
        dev,
        Some(json!({"message": "che ore sono"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let (status, me, _) = call(&app, "GET", "/api/devices/me", REMOTE, dev, None).await;
    assert_eq!(
        (status, me["name"].as_str()),
        (StatusCode::OK, Some("Telefono"))
    );
    assert!(me.get("token").is_none());

    // Operazioni del proprietario negate dal dispositivo.
    for (m, p) in [
        ("GET", "/api/devices"),
        ("POST", "/api/devices/pairing"),
        ("GET", "/api/audit"),
        ("POST", "/api/backup"),
        ("GET", "/api/connectors"),
        ("GET", "/api/workspace"),
        ("GET", "/api/memory"),
        ("GET", "/api/metrics"),
    ] {
        let (status, _, _) = call(&app, m, p, REMOTE, dev, Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{m} {p}");
    }
    // Senza il permesso «confirm» non può confermare.
    let (_, out, _) = call(
        &app,
        "POST",
        "/api/chat",
        REMOTE,
        dev,
        Some(json!({"message": "scrivi nota.txt: dal telefono"})),
    )
    .await;
    let cid = out["confirmation"]["confirmation_id"]
        .as_str()
        .expect("conferma richiesta")
        .to_string();
    let (status, _, _) = call(
        &app,
        "POST",
        &format!("/api/confirmations/{cid}"),
        REMOTE,
        dev,
        Some(json!({"approve": true})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Elenco del proprietario: nessun token, nessuna impronta.
    let (_, list, text) = call(
        &app,
        "GET",
        "/api/devices",
        &local(),
        Some(("x-jarvis-token", "t")),
        None,
    )
    .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert!(!text.contains(&token));

    let (status, revoked, _) = call(
        &app,
        "POST",
        &format!("/api/devices/{id}/revoke"),
        &local(),
        Some(("x-jarvis-token", "t")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(revoked["revoked_at"].is_string());
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/chat",
        REMOTE,
        dev,
        Some(json!({"message": "che ore sono"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "revoca immediata");
    let (_, audit, _) = call(
        &app,
        "GET",
        "/api/audit",
        &local(),
        Some(("x-jarvis-token", "t")),
        None,
    )
    .await;
    let audit = audit.to_string();
    assert!(audit.contains("device.pair") && audit.contains("device.revoke"));
}

#[tokio::test]
async fn confirm_scope_allows_confirmations() {
    let app = app();
    let (token, _) = pair(&app, json!(["chat", "confirm"])).await;
    let dev = Some(("x-jarvis-device", token.as_str()));
    let (_, out, _) = call(
        &app,
        "POST",
        "/api/chat",
        REMOTE,
        dev,
        Some(json!({"message": "scrivi nota.txt: dal telefono"})),
    )
    .await;
    let cid = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, done, _) = call(
        &app,
        "POST",
        &format!("/api/confirmations/{cid}"),
        REMOTE,
        dev,
        Some(json!({"approve": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
}

#[tokio::test]
async fn pairing_codes_are_single_use_and_brute_force_is_stopped() {
    let app = app();
    let (_, code, _) = call(
        &app,
        "POST",
        "/api/devices/pairing",
        &local(),
        Some(("x-jarvis-token", "t")),
        Some(json!({})),
    )
    .await;
    let code = code["code"].as_str().unwrap().to_string();
    let body = |c: &str| Some(json!({"code": c, "name": "x", "platform": "web"}));
    assert_eq!(
        call(&app, "POST", "/api/devices/pair", REMOTE, None, body(&code))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "POST", "/api/devices/pair", REMOTE, None, body(&code))
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "monouso"
    );

    let (_, code, _) = call(
        &app,
        "POST",
        "/api/devices/pairing",
        &local(),
        Some(("x-jarvis-token", "t")),
        Some(json!({})),
    )
    .await;
    let code = code["code"].as_str().unwrap().to_string();
    for _ in 0..5 {
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/devices/pair",
                REMOTE,
                None,
                body("ZZZZZZZZ")
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        call(&app, "POST", "/api/devices/pair", REMOTE, None, body(&code))
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "dopo 5 errori i codici in attesa sono annullati"
    );
    // Un codice non si genera da remoto.
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/devices/pairing",
            REMOTE,
            None,
            Some(json!({}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
