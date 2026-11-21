//! OAuth 2.0 per i connettori (ADR-0027) contro un server di autorizzazione sintetico che verifica PKCE.
//! Coprono avvio, `state` monouso, scambio del codice, chiamata con l'access token, rinnovo automatico,
//! rifiuto del consenso e revoca.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use sha2::Digest;
use tower::ServiceExt;

const TOKEN: &str = "token-oauth";
const PORT: u16 = 18769;

#[derive(Default)]
struct AuthServer {
    /// challenge registrata al momento del codice
    challenge: Option<String>,
    issued: u32,
    refreshed: u32,
    last_bearer: String,
    revoked: Vec<String>,
    /// Access token accettato dall'API; il test lo cambia per simulare una revoca lato servizio.
    accepted: Option<String>,
}

fn form(body: &str) -> std::collections::HashMap<String, String> {
    url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

/// Endpoint dei token e API protetta. Il codice valido è «codice-1» e la challenge viene fissata dal test.
fn auth_server() -> (String, Arc<Mutex<AuthServer>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let state = Arc::new(Mutex::new(AuthServer::default()));
    let shared = state.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let (mut length, mut auth) = (0usize, String::new());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((k, v)) = line.trim_end().split_once(':') {
                    match k.to_ascii_lowercase().as_str() {
                        "content-length" => length = v.trim().parse().unwrap_or(0),
                        "authorization" => auth = v.trim().to_string(),
                        _ => {}
                    }
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8_lossy(&body).to_string();
            let mut s = shared.lock().unwrap();
            let (status, payload) = if first.starts_with("POST /token") {
                let f = form(&body);
                match f.get("grant_type").map(String::as_str) {
                    Some("authorization_code") => {
                        let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                        let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(sha2::Sha256::digest(verifier.as_bytes()));
                        if f.get("code").map(String::as_str) == Some("codice-1")
                            && s.challenge.as_deref() == Some(computed.as_str())
                            && f["client_id"] == "jarvis-test"
                        {
                            s.issued += 1;
                            // Scade subito: la prima chiamata deve rinnovarlo.
                            (
                                200,
                                json!({"access_token": "accesso-1", "refresh_token": "rinnovo-1", "expires_in": 10, "token_type": "Bearer"}),
                            )
                        } else {
                            (400, json!({"error": "invalid_grant"}))
                        }
                    }
                    Some("refresh_token")
                        if f.get("refresh_token").map(String::as_str) == Some("rinnovo-1") =>
                    {
                        s.refreshed += 1;
                        let token = format!("accesso-{}", s.refreshed + 1);
                        (
                            200,
                            json!({"access_token": token, "expires_in": 3600, "token_type": "Bearer"}),
                        )
                    }
                    _ => (400, json!({"error": "unsupported_grant_type"})),
                }
            } else if first.starts_with("POST /revoke") {
                let f = form(&body);
                s.revoked.push(format!(
                    "{}:{}",
                    f.get("token_type_hint").cloned().unwrap_or_default(),
                    f.get("token").cloned().unwrap_or_default()
                ));
                (200, json!({}))
            } else if first.starts_with("GET /api/profilo") {
                s.last_bearer = auth.clone();
                let accepted = s.accepted.clone().unwrap_or_else(|| "accesso-2".into());
                if auth == format!("Bearer {accepted}") {
                    (200, json!({"nome": "Proprietario sintetico"}))
                } else {
                    (401, json!({"error": "token scaduto"}))
                }
            } else {
                (404, json!({}))
            };
            let payload = payload.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
        }
    });
    (url, state)
}

struct Env {
    app: axum::Router,
}

impl Env {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: bool,
    ) -> (StatusCode, String) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", format!("localhost:{PORT}"))
            .header("content-type", "application/json");
        if token {
            req = req.header("x-jarvis-token", TOKEN);
        }
        let res = self
            .app
            .clone()
            .oneshot(
                req.body(
                    body.map(|b| Body::from(b.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn json(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (s, t) = self.call(method, path, body, true).await;
        (s, serde_json::from_str(&t).unwrap_or(Value::Null))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_pkce_flow_authorizes_refreshes_and_revokes() {
    std::env::set_var("JARVIS_SECRETS_KEYCHAIN", "off");
    let (server, auth) = auth_server();
    let data = std::env::temp_dir().join(format!("jarvis-oauth-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let web = jarvis_core::web::WebConfig {
        allow_private: true,
        ..Default::default()
    };
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
        web,
    };
    let env = Env {
        app: api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT)),
    };
    let manifest = json!({
        "contract": "jarvis.connector/1", "id": "profilo", "name": "Profilo", "version": "1.0.0", "description": "Servizio OAuth sintetico",
        "base_url": format!("{server}/api"), "allowed_hosts": ["127.0.0.1"],
        "auth": {"type": "oauth2", "authorize_url": format!("{server}/authorize"), "token_url": format!("{server}/token"), "revoke_url": format!("{server}/revoke"),
                 "client_id": "jarvis-test", "scopes": ["profile.read"]},
        "operations": [{"name": "leggi", "summary": "Profilo del proprietario", "method": "GET", "path": "/profilo", "effects": "read",
                        "input_schema": {"type": "object", "additionalProperties": false, "properties": {}}}],
    });
    let (status, out) = env.json("POST", "/api/connectors", Some(manifest)).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["auth"], "oauth2");
    let (status, _) = env
        .json(
            "POST",
            "/api/connectors/profilo/authorize",
            Some(json!({"credential": "x"})),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "OAuth non accetta credenziali incollate"
    );

    // Avvio: pagina di consenso con PKCE S256 e redirect sul loopback del Core.
    let (status, start) = env
        .json(
            "POST",
            "/api/connectors/profilo/oauth/start",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{start}");
    let authorize = url::Url::parse(start["authorize_url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<String, String> =
        authorize.query_pairs().into_owned().collect();
    assert_eq!(
        q["redirect_uri"],
        format!("http://localhost:{PORT}/api/connectors/oauth/callback")
    );
    assert_eq!(
        (q["code_challenge_method"].as_str(), q["scope"].as_str()),
        ("S256", "profile.read")
    );
    auth.lock().unwrap().challenge = Some(q["code_challenge"].clone());

    // Uno state inventato o un codice sbagliato non autorizzano nulla.
    let (status, page) = env
        .call(
            "GET",
            "/api/connectors/oauth/callback?state=falso&code=codice-1",
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("non riuscita"));

    // Ritorno dal servizio: senza token di sessione, autenticato dallo state monouso.
    let cb = format!(
        "/api/connectors/oauth/callback?state={}&code=codice-1",
        q["state"]
    );
    let (status, page) = env.call("GET", &cb, None, false).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Profilo è collegato"));
    let (status, _) = env.call("GET", &cb, None, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "state già usato");
    assert_eq!(auth.lock().unwrap().issued, 1);

    // I token non tornano mai al client.
    let (_, list) = env.json("GET", "/api/connectors", None).await;
    assert!(!list.to_string().contains("accesso-1") && !list.to_string().contains("rinnovo-1"));
    assert_eq!(list["items"][0]["authorized"], true);

    // Chiamata dalla chat: il token in scadenza viene rinnovato prima della richiesta.
    let (_, out) = env
        .json(
            "POST",
            "/api/chat",
            Some(json!({"message": "connettore profilo leggi"})),
        )
        .await;
    assert!(
        out["reply"]
            .as_str()
            .unwrap_or_default()
            .contains("Proprietario sintetico"),
        "{out}"
    );
    {
        let s = auth.lock().unwrap();
        assert_eq!(s.refreshed, 1, "rinnovo automatico");
        assert_eq!(s.last_bearer, "Bearer accesso-2");
    }

    // Il servizio invalida il token prima della scadenza dichiarata: 401, rinnovo immediato e una sola
    // nuova chiamata (ADR-0037).
    auth.lock().unwrap().accepted = Some("accesso-3".into());
    let (_, out) = env
        .json(
            "POST",
            "/api/chat",
            Some(json!({"message": "connettore profilo leggi"})),
        )
        .await;
    assert!(
        out["reply"]
            .as_str()
            .unwrap_or_default()
            .contains("Proprietario sintetico"),
        "{out}"
    );
    {
        let s = auth.lock().unwrap();
        assert_eq!(s.refreshed, 2, "rinnovo dopo il 401");
        assert_eq!(s.last_bearer, "Bearer accesso-3");
    }

    // Consenso negato dal proprietario sul servizio.
    let (_, start) = env
        .json(
            "POST",
            "/api/connectors/profilo/oauth/start",
            Some(json!({})),
        )
        .await;
    let st: String = url::Url::parse(start["authorize_url"].as_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .unwrap();
    let (status, page) = env
        .call(
            "GET",
            &format!("/api/connectors/oauth/callback?state={st}&error=access_denied"),
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("non ha concesso"));

    // Revoca: token cancellati e strumenti rimossi.
    let (status, out) = env
        .json("POST", "/api/connectors/profilo/revoke", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["remote_revoked"], true, "{out}");
    assert_eq!(
        auth.lock().unwrap().revoked,
        vec!["refresh_token:rinnovo-1".to_string()],
        "revoca presso il servizio (RFC 7009)"
    );
    let (_, tools) = env.json("GET", "/api/tools", None).await;
    assert!(!tools.to_string().contains("connector.profilo."));
    let (_, audit) = env.json("GET", "/api/audit?limit=100", None).await;
    let audit = audit.to_string();
    assert!(
        audit.contains("connector.oauth.start")
            && audit.contains("connector.oauth.denied")
            && audit.contains("connector.revoke")
    );
    let _ = std::fs::remove_dir_all(&data);
}
