//! Utenti e personalità (ADR-0169): registrazione su invito, accesso con limiti, sessioni, isolamento fra utenti e
//! dal proprietario, comandi personali e gestione da parte del proprietario. Nessun modello cloud: i comandi
//! personali non lo richiedono e senza di esso la chat risponde con un messaggio chiaro.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-del-proprietario";
const PORT: u16 = 18799;
const PASSWORD: &str = "una frase lunga e strana";

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    let data = std::env::temp_dir().join(format!("jarvis-users-{}", uuid::Uuid::new_v4().simple()));
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
    Env {
        app: api::router(api::state(
            Jarvis::new(cfg).expect("core"),
            TOKEN.into(),
            PORT,
        )),
        data,
    }
}

impl Env {
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", format!("127.0.0.1:{PORT}"))
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
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

    async fn owner(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send(method, path, body, &[("x-jarvis-token", TOKEN)])
            .await
    }

    async fn user(
        &self,
        token: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.send(method, path, body, &[("x-jarvis-user", token)])
            .await
    }

    async fn invite(&self) -> String {
        let (status, out) = self
            .owner("POST", "/api/users/invites", Some(json!({})))
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        out["code"].as_str().unwrap().to_string()
    }

    /// Registra un utente con un invito nuovo e restituisce il token di sessione.
    async fn register(&self, username: &str, display: &str) -> String {
        let code = self.invite().await;
        let (status, out) = self
            .send(
                "POST",
                "/api/auth/register",
                Some(json!({"username": username, "password": PASSWORD, "display_name": display, "invite": code})),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        out["token"].as_str().unwrap().to_string()
    }
}

#[tokio::test]
async fn registration_needs_a_single_use_invite_and_respects_the_owners_policy() {
    let e = env();
    let body = |invite: &str| json!({"username": "marta", "password": PASSWORD, "display_name": "Marta", "invite": invite});
    let (status, _) = e
        .send("POST", "/api/auth/register", Some(body("")), &[])
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "senza invito non si entra"
    );
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/register",
            Some(body(&"0".repeat(32))),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "invito inventato");

    let code = e.invite().await;
    let (status, out) = e
        .send("POST", "/api/auth/register", Some(body(&code)), &[])
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["token"].as_str().unwrap().len(), 64);
    assert_eq!(out["user"]["username"], "marta");
    assert!(out["user"].get("pw_hash").is_none() && !out.to_string().contains("argon2"));

    let again =
        json!({"username": "luca", "password": PASSWORD, "display_name": "Luca", "invite": code});
    let (status, _) = e.send("POST", "/api/auth/register", Some(again), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "l'invito vale una volta");

    // Nome già in uso e password deboli.
    let code = e.invite().await;
    let (status, out) = e
        .send("POST", "/api/auth/register", Some(body(&code)), &[])
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{out}");
    for weak in ["corta", "nuovo-nuovo-1", "aaaaaaaaaaaa", "password123"] {
        let (status, _) = e
            .send(
                "POST",
                "/api/auth/register",
                Some(json!({"username": "nuovo", "password": weak, "invite": code})),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{weak}");
    }
    // La registrazione fallita non ha consumato l'invito.
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/register",
            Some(json!({"username": "nuovo", "password": PASSWORD, "invite": code})),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Chiusa: nessuno entra nemmeno con un invito; aperta: basta il nome.
    e.owner(
        "POST",
        "/api/users/settings",
        Some(json!({"registration": "closed"})),
    )
    .await;
    let code = e.invite().await;
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/register",
            Some(json!({"username": "chiusa", "password": PASSWORD, "invite": code})),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    e.owner(
        "POST",
        "/api/users/settings",
        Some(json!({"registration": "open"})),
    )
    .await;
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/register",
            Some(json!({"username": "aperta", "password": PASSWORD})),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn login_hides_which_part_was_wrong_and_locks_after_repeated_failures() {
    let e = env();
    e.register("marta", "Marta").await;
    let login = |user: &str, password: &str| json!({"username": user, "password": password});
    let (status, wrong) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(login("marta", "sbagliata sbagliata")),
            &[],
        )
        .await;
    let (status2, unknown) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(login("nessuno", "sbagliata sbagliata")),
            &[],
        )
        .await;
    assert_eq!(
        (status, status2),
        (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED)
    );
    assert_eq!(
        wrong["error"]["message"], unknown["error"]["message"],
        "nessuna enumerazione degli account"
    );

    let (status, ok) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(login("MARTA", PASSWORD)),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["token"].as_str().unwrap().len(), 64);

    // Un accesso riuscito azzera il conto: servono cinque errori di fila per il blocco.
    for _ in 0..5 {
        e.send(
            "POST",
            "/api/auth/login",
            Some(login("marta", "sbagliata sbagliata")),
            &[],
        )
        .await;
    }
    // Dopo cinque errori l'account è bloccato per un po', anche con la password giusta.
    let (status, out) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(login("marta", PASSWORD)),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{out}");
    // Un altro account non è toccato.
    e.register("luca", "Luca").await;
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(login("luca", PASSWORD)),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sessions_and_owner_credentials_are_never_interchangeable() {
    let e = env();
    let token = e.register("marta", "Marta").await;
    // Senza sessione o con una sessione inventata non si entra.
    assert_eq!(
        e.send("GET", "/api/me", None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.user(&"a".repeat(64), "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.user("corto", "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Il token del proprietario non apre l'area dell'utente.
    assert_eq!(
        e.send("GET", "/api/me", None, &[("x-jarvis-token", TOKEN)])
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, me) = e.user(&token, "GET", "/api/me", None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["user"]["display_name"], "Marta");
    // Il token dell'utente non apre nulla del proprietario.
    for (method, path) in [
        ("GET", "/api/memory"),
        ("GET", "/api/users"),
        ("POST", "/api/users/invites"),
        ("GET", "/api/audit"),
        ("POST", "/api/chat"),
    ] {
        let (status, _) = e
            .user(&token, method, path, Some(json!({"message": "ciao"})))
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    // Uscire chiude la sessione.
    assert_eq!(
        e.user(&token, "POST", "/api/auth/logout", Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        e.user(&token, "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn each_user_has_their_own_memory_history_and_personality_apart_from_the_owner() {
    let e = env();
    let (marta, luca) = (
        e.register("marta", "Marta").await,
        e.register("luca", "Luca").await,
    );
    let (_, owner_before) = e.owner("GET", "/api/persona", None).await;

    let (status, out) = e
        .user(
            &marta,
            "POST",
            "/api/me/chat",
            Some(json!({"message": "Ricorda che amo il caffè lungo."})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["personal"], true);
    let (_, mine) = e.user(&marta, "GET", "/api/me/memory", None).await;
    assert_eq!(mine["items"][0]["text"], "amo il caffè lungo");
    let (_, theirs) = e.user(&luca, "GET", "/api/me/memory", None).await;
    assert!(
        theirs["items"].as_array().unwrap().is_empty(),
        "Luca non vede i ricordi di Marta"
    );
    let id = mine["items"][0]["id"].as_i64().unwrap();
    assert_eq!(
        e.user(&luca, "DELETE", &format!("/api/me/memory/{id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, history) = e.user(&luca, "GET", "/api/me/history", None).await;
    assert!(history["items"].as_array().unwrap().is_empty());
    let (_, hers) = e.user(&marta, "GET", "/api/me/history", None).await;
    assert_eq!(hers["items"].as_array().unwrap().len(), 2);

    // La personalità si cambia a mano o con le frasi di tutti i giorni, e non tocca quella del proprietario.
    let (status, out) = e
        .user(&marta, "PUT", "/api/me/personality", Some(json!({"persona": {"address": "Capitano", "formal": true}, "profile": {"tone": "cordiale", "interests": "vela e fotografia"}})))
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let summary = out["summary"].to_string();
    assert!(
        summary.contains("Capitano")
            && summary.contains("lei")
            && summary.contains("vela e fotografia"),
        "{summary}"
    );
    e.user(
        &marta,
        "POST",
        "/api/me/chat",
        Some(json!({"message": "chiamami Marta"})),
    )
    .await;
    e.user(
        &marta,
        "POST",
        "/api/me/chat",
        Some(json!({"message": "dammi del tu"})),
    )
    .await;
    let (_, me) = e.user(&marta, "GET", "/api/me", None).await;
    assert_eq!(me["personality"]["persona"]["address"], "Marta");
    assert_eq!(me["personality"]["persona"]["formal"], false);
    let (_, other) = e.user(&luca, "GET", "/api/me", None).await;
    assert_ne!(other["personality"]["persona"]["address"], "Marta");
    let (_, owner_after) = e.owner("GET", "/api/persona", None).await;
    assert_eq!(
        owner_before, owner_after,
        "la personalità del proprietario non cambia"
    );
    // Valori non validi vengono rifiutati.
    let (status, _) = e
        .user(
            &marta,
            "PUT",
            "/api/me/personality",
            Some(json!({"persona": {"address": "x@y.it"}})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(
        e.user(&marta, "DELETE", "/api/me/history", None).await.0,
        StatusCode::OK
    );
    let (_, cleared) = e.user(&marta, "GET", "/api/me/history", None).await;
    assert!(cleared["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_users_chat_needs_the_owners_consent_for_the_cloud_model() {
    let e = env();
    let token = e.register("marta", "Marta").await;
    let (status, out) = e
        .user(
            &token,
            "POST",
            "/api/me/chat",
            Some(json!({"message": "Che cos'è il DNS?"})),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{out}");
    assert!(out["error"]["message"]
        .as_str()
        .unwrap()
        .contains("amministratore"));
    // Con il consenso ma senza una chiave cloud il messaggio resta chiaro.
    e.owner("POST", "/api/users/settings", Some(json!({"cloud": true})))
        .await;
    let (status, out) = e
        .user(
            &token,
            "POST",
            "/api/me/chat",
            Some(json!({"message": "Che cos'è il DNS?"})),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{out}");
    assert!(out["error"]["message"]
        .as_str()
        .unwrap()
        .contains("modello cloud"));
    // Messaggi vuoti, troppo lunghi o con credenziali non partono.
    for message in [
        String::new(),
        "x".repeat(4001),
        "usa sk-abcdefghijklmnopqrstuvwxyz0123456789".into(),
    ] {
        let (status, _) = e
            .user(
                &token,
                "POST",
                "/api/me/chat",
                Some(json!({"message": message})),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    // Il limite di messaggi protegge dai ritmi eccessivi.
    for _ in 0..30 {
        e.user(
            &token,
            "POST",
            "/api/me/chat",
            Some(json!({"message": "ricorda che prova"})),
        )
        .await;
    }
    let (status, out) = e
        .user(
            &token,
            "POST",
            "/api/me/chat",
            Some(json!({"message": "ancora"})),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{out}");
}

#[tokio::test]
async fn the_owner_manages_users_without_seeing_their_messages_and_can_cut_access() {
    let e = env();
    let token = e.register("marta", "Marta").await;
    e.user(
        &token,
        "POST",
        "/api/me/chat",
        Some(json!({"message": "Ricorda che il mio segreto è il blu."})),
    )
    .await;
    let (status, list) = e.owner("GET", "/api/users", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let text = list.to_string();
    assert!(
        !text.contains("argon2") && !text.contains("segreto") && !text.contains("blu"),
        "{text}"
    );
    assert_eq!(list["items"][0]["messages"], 2);
    assert_eq!(list["settings"]["registration"], "invite");
    let id = list["items"][0]["id"].as_str().unwrap().to_string();
    assert!(e.data.join("users").join(&id).is_dir());

    // Disattivare chiude le sessioni e vieta il nuovo accesso; riattivare lo permette di nuovo.
    assert_eq!(
        e.owner("POST", &format!("/api/users/{id}/disable"), Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        e.user(&token, "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(json!({"username": "marta", "password": PASSWORD})),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        e.owner("POST", &format!("/api/users/{id}/enable"), Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    let (status, ok) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(json!({"username": "marta", "password": PASSWORD})),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    let fresh = ok["token"].as_str().unwrap().to_string();

    // Cambiare password: serve quella attuale; le altre sessioni si chiudono.
    let (status, _) = e
        .user(
            &fresh,
            "POST",
            "/api/me/password",
            Some(json!({"current": "sbagliata sbagliata", "new": "un'altra frase molto lunga"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, changed) = e
        .user(
            &fresh,
            "POST",
            "/api/me/password",
            Some(json!({"current": PASSWORD, "new": "un'altra frase molto lunga"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        e.user(&fresh, "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED,
        "la vecchia sessione si chiude"
    );
    let new_token = changed["token"].as_str().unwrap();
    assert_eq!(
        e.user(new_token, "GET", "/api/me", None).await.0,
        StatusCode::OK
    );
    let (status, _) = e
        .send(
            "POST",
            "/api/auth/login",
            Some(json!({"username": "marta", "password": PASSWORD})),
            &[],
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "la vecchia password non vale più"
    );

    // Eliminare rimuove l'account con i suoi dati.
    assert_eq!(
        e.owner("POST", &format!("/api/users/{id}/delete"), Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    assert!(!e.data.join("users").join(&id).exists());
    assert_eq!(
        e.user(new_token, "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.owner("POST", &format!("/api/users/{id}/delete"), Some(json!({})))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // Un utente può anche eliminare da solo il proprio account.
    let other = e.register("luca", "Luca").await;
    assert_eq!(
        e.user(&other, "DELETE", "/api/me", None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        e.user(&other, "GET", "/api/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn account_events_are_audited_without_secrets() {
    let e = env();
    let token = e.register("marta", "Marta").await;
    e.send(
        "POST",
        "/api/auth/login",
        Some(json!({"username": "marta", "password": "sbagliata sbagliata"})),
        &[],
    )
    .await;
    e.send(
        "POST",
        "/api/auth/login",
        Some(json!({"username": "marta", "password": PASSWORD})),
        &[],
    )
    .await;
    e.user(&token, "POST", "/api/auth/logout", Some(json!({})))
        .await;
    let (status, audit) = e.owner("GET", "/api/audit?limit=200", None).await;
    assert_eq!(status, StatusCode::OK, "{audit}");
    let events = audit["events"].as_array().cloned().unwrap_or_default();
    let actions: Vec<&str> = events
        .iter()
        .filter_map(|ev| ev["action"].as_str())
        .collect();
    for expected in ["users.invite", "user.register", "user.login", "user.logout"] {
        assert!(actions.contains(&expected), "{expected} in {actions:?}");
    }
    let denied = events.iter().any(|ev| {
        ev["action"] == "user.login"
            && ev["outcome"] == "failed"
            && ev["error_code"] == "auth.invalid_credentials"
    });
    assert!(denied, "il tentativo fallito è registrato");
    let text = audit.to_string();
    assert!(
        !text.contains(PASSWORD) && !text.contains(&token) && !text.contains("argon2"),
        "nessun segreto nell'audit"
    );
}
