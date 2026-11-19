//! Passkey dei dispositivi associati (ADR-0042) con il worker Python reale in modalità offline e un
//! autenticatore sintetico: registrazione, approvazione delle conferme solo con la passkey legata a quella
//! conferma, sfide monouso, rifiuto sempre possibile, gestione dal computer del proprietario.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use jarvis_core::passkeys::testing::Authenticator;
use serde_json::{json, Value};
use tower::ServiceExt;

const PORT: u16 = 18781;
const PHONE: &str = "jarvis.test";

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    let data =
        std::env::temp_dir().join(format!("jarvis-passkey-{}", uuid::Uuid::new_v4().simple()));
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
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let state = api::state_with_remote(
        Jarvis::new(cfg).unwrap(),
        "t".into(),
        PORT,
        vec![PHONE.into()],
    );
    Env {
        app: api::router(state),
        data,
    }
}

impl Env {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        device: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        req = match device {
            Some(t) => req.header("host", PHONE).header("x-jarvis-device", t),
            None => req
                .header("host", format!("127.0.0.1:{PORT}"))
                .header("x-jarvis-token", "t"),
        };
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

    async fn pair_phone(&self) -> String {
        let (_, code) = self
            .call(
                "POST",
                "/api/devices/pairing",
                Some(json!({"scopes": ["chat", "confirm"]})),
                None,
            )
            .await;
        let req = Request::builder()
            .method("POST")
            .uri("/api/devices/pair")
            .header("host", PHONE)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"code": code["code"], "name": "Telefono", "platform": "android"})
                    .to_string(),
            ))
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let out: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        out["token"].as_str().unwrap().to_string()
    }

    /// Chiede una scrittura dal telefono e restituisce l'id della conferma.
    async fn pending_write(&self, phone: &str, file: &str) -> String {
        let (status, out) = self
            .call(
                "POST",
                "/api/chat",
                Some(json!({"message": format!("scrivi {file}: ciao")})),
                Some(phone),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        out["confirmation"]["confirmation_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn assertion(&self, phone: &str, key: &mut Authenticator, confirmation: &str) -> Value {
        let (status, opts) = self
            .call(
                "POST",
                "/api/passkeys/assert/options",
                Some(json!({"confirmation_id": confirmation})),
                Some(phone),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{opts}");
        assert_eq!(opts["rpId"], PHONE);
        assert_eq!(opts["userVerification"], "required");
        let (cd, ad, sig) = key.assert(opts["challenge"].as_str().unwrap(), true);
        json!({"id": jarvis_core::passkeys::b64url(&key.credential_id), "clientDataJSON": cd, "authenticatorData": ad, "signature": sig})
    }
}

#[tokio::test]
async fn approvals_from_a_device_with_a_passkey_require_a_fresh_assertion() {
    let e = env();
    let phone = e.pair_phone().await;

    // Registrazione dal telefono: opzioni ES256 con verifica dell'utente, poi risposta dell'autenticatore.
    let (status, opts) = e
        .call("POST", "/api/passkeys/register/options", None, Some(&phone))
        .await;
    assert_eq!(status, StatusCode::OK, "{opts}");
    assert_eq!(opts["rp"]["id"], PHONE);
    assert_eq!(opts["pubKeyCredParams"][0]["alg"], -7);
    let mut key = Authenticator::new(PHONE, &format!("http://{PHONE}"));
    let (cd, att) = key.register(opts["challenge"].as_str().unwrap());
    let (status, out) = e
        .call(
            "POST",
            "/api/passkeys/register",
            Some(json!({"clientDataJSON": cd, "attestationObject": att})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    // La stessa sfida non registra una seconda volta.
    let (status, _) = e
        .call(
            "POST",
            "/api/passkeys/register",
            Some(json!({"clientDataJSON": cd, "attestationObject": att})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Approvare senza passkey non basta più; il file non viene scritto.
    let conf = e.pending_write(&phone, "pk.txt").await;
    let (status, out) = e
        .call(
            "POST",
            &format!("/api/confirmations/{conf}"),
            Some(json!({"approve": true})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(out["error"]["code"], "passkey_required");
    assert!(!e.data.join("workspace/pk.txt").exists());

    // Con l'asserzione legata a questa conferma l'azione viene eseguita.
    let passkey = e.assertion(&phone, &mut key, &conf).await;
    let (status, out) = e
        .call(
            "POST",
            &format!("/api/confirmations/{conf}"),
            Some(json!({"approve": true, "passkey": passkey})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert!(e.data.join("workspace/pk.txt").exists());

    // Un'asserzione non si riusa e non vale per un'altra conferma.
    let other = e.pending_write(&phone, "altro.txt").await;
    let (status, out) = e
        .call(
            "POST",
            &format!("/api/confirmations/{other}"),
            Some(json!({"approve": true, "passkey": passkey})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{out}");
    assert_eq!(out["error"]["code"], "passkey_invalid");
    let wrong_target = e.assertion(&phone, &mut key, &conf).await;
    let (status, _) = e
        .call(
            "POST",
            &format!("/api/confirmations/{other}"),
            Some(json!({"approve": true, "passkey": wrong_target})),
            Some(&phone),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "sfida di un'altra conferma"
    );
    assert!(!e.data.join("workspace/altro.txt").exists());

    // Una passkey di un altro autenticatore (stesso id, chiave diversa) viene rifiutata.
    let mut forged = Authenticator::new(PHONE, &format!("http://{PHONE}"));
    forged.credential_id = key.credential_id.clone();
    let fake = e.assertion(&phone, &mut forged, &other).await;
    let (status, _) = e
        .call(
            "POST",
            &format!("/api/confirmations/{other}"),
            Some(json!({"approve": true, "passkey": fake})),
            Some(&phone),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "firma con un'altra chiave"
    );

    // Rifiutare resta possibile senza passkey.
    let (status, out) = e
        .call(
            "POST",
            &format!("/api/confirmations/{other}"),
            Some(json!({"approve": false})),
            Some(&phone),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");

    // Il proprietario vede e gestisce le passkey; il telefono non può eliminarle.
    let (_, list) = e.call("GET", "/api/passkeys", None, None).await;
    let item = &list["items"][0];
    assert_eq!(item["device_name"], "Telefono");
    assert!(item["last_used_at"].is_string());
    assert!(
        !list.to_string().contains("public_key"),
        "le chiavi non escono dal Core"
    );
    let id = item["credential_id"].as_str().unwrap();
    let (status, _) = e
        .call("DELETE", &format!("/api/passkeys/{id}"), None, Some(&phone))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = e
        .call("DELETE", &format!("/api/passkeys/{id}"), None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, audit) = e.call("GET", "/api/audit?limit=100", None, None).await;
    let audit = audit.to_string();
    assert!(
        audit.contains("passkey.register")
            && audit.contains("passkey.assert")
            && audit.contains("passkey.delete")
    );
    let _ = std::fs::remove_dir_all(&e.data);
}

#[tokio::test]
async fn owner_on_the_core_computer_confirms_without_passkey() {
    let e = env();
    let (_, out) = e
        .call(
            "POST",
            "/api/chat",
            Some(json!({"message": "scrivi locale.txt: ciao"})),
            None,
        )
        .await;
    let conf = out["confirmation"]["confirmation_id"].as_str().unwrap();
    let (status, _) = e
        .call("POST", "/api/passkeys/register/options", None, None)
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "le passkey si registrano dal dispositivo"
    );
    let (status, out) = e
        .call(
            "POST",
            &format!("/api/confirmations/{conf}"),
            Some(json!({"approve": true})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let _ = std::fs::remove_dir_all(&e.data);
}
