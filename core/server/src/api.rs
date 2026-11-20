//! API HTTP locale e interfaccia Web.
//!
//! Difese del prototipo: ascolto solo su loopback; controllo dell'header `Host`
//! contro il DNS rebinding; token di sessione casuale richiesto da ogni `/api/*`
//! (eccetto `/api/health`) e consegnato solo nella pagina servita dal Core, che
//! un sito di terze parti non può leggere (nessun CORS); CSP restrittiva.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::orchestrator::{CoreError, Jarvis};

const INDEX: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const APP_CSS: &str = include_str!("../static/app.css");
const VOICE_WORKLET: &str = include_str!("../static/voice-worklet.js");
const MANIFEST: &str = include_str!("../static/manifest.webmanifest");
const SERVICE_WORKER: &str = include_str!("../static/sw.js");
const ICON: &str = include_str!("../static/icon.svg");
const PAIR_HTML: &str = include_str!("../static/pair.html");
const PAIR_JS: &str = include_str!("../static/pair.js");
const BOOT_JS: &str = include_str!("../static/boot.js");

/// Asset statico servito con ETag (revalida con 304) e, se il client la accetta, compressione gzip calcolata
/// una sola volta: il bundle passa da ~320 KB a ~100 KB per telefoni e tablet in rete locale.
struct Asset {
    body: &'static str,
    ctype: &'static str,
    prepared: std::sync::LazyLock<(String, Vec<u8>)>,
}

fn prepare(body: &str) -> (String, Vec<u8>) {
    use std::io::Write;
    let digest = ring::digest::digest(&ring::digest::SHA256, body.as_bytes());
    let etag = format!(
        "\"{}\"",
        digest.as_ref()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = gz.write_all(body.as_bytes());
    (etag, gz.finish().unwrap_or_default())
}

static JS_ASSET: Asset = Asset {
    body: APP_JS,
    ctype: "text/javascript; charset=utf-8",
    prepared: std::sync::LazyLock::new(|| prepare(APP_JS)),
};
static CSS_ASSET: Asset = Asset {
    body: APP_CSS,
    ctype: "text/css; charset=utf-8",
    prepared: std::sync::LazyLock::new(|| prepare(APP_CSS)),
};

fn asset(headers: &axum::http::HeaderMap, a: &'static Asset) -> Response {
    let (etag, gz) = &*a.prepared;
    let common = [
        (header::ETAG, etag.as_str()),
        (header::CACHE_CONTROL, "no-cache"),
        (header::VARY, "Accept-Encoding"),
    ];
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    if matches {
        return (StatusCode::NOT_MODIFIED, common).into_response();
    }
    let gzip = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|e| e.trim().starts_with("gzip")));
    if gzip && !gz.is_empty() {
        (
            common,
            [
                (header::CONTENT_TYPE, a.ctype),
                (header::CONTENT_ENCODING, "gzip"),
            ],
            gz.clone(),
        )
            .into_response()
    } else {
        (common, [(header::CONTENT_TYPE, a.ctype)], a.body).into_response()
    }
}

pub struct AppState {
    pub core: Arc<Mutex<Jarvis>>,
    pub obs: Arc<crate::observability::Observability>,
    pub token: String,
    pub port: u16,
    pub last_health: Mutex<Value>,
    /// Host pubblicati da un proxy fidato (es. `tailscale serve`) per i dispositivi associati (RF-11).
    pub remote_hosts: Vec<String>,
    /// Assegnazioni ai nodi esecutori: le loro richieste non passano dal blocco del Core.
    pub nodes: Arc<crate::nodes::NodeHub>,
    device_db: std::path::PathBuf,
    /// Configurazione guidata dei modelli: stato letto senza il blocco del Core.
    pub setup: Arc<crate::setup::Setup>,
    n8n: Arc<crate::n8n::Manager>,
    /// Sfide WebAuthn in corso (ADR-0042): sfida -> dispositivo, scopo e scadenza.
    challenges: Mutex<std::collections::HashMap<String, Challenge>>,
}

/// Sfida monouso di una passkey: registrazione o approvazione di una conferma precisa.
struct Challenge {
    device_id: String,
    purpose: String,
    expires: std::time::Instant,
}

/// Dispositivo associato autenticato dal middleware (assente per il proprietario sul computer del Core).
#[derive(Clone)]
struct DeviceCtx(Value);

/// Nome RP e origini ammesse per una passkey: l'host della richiesta, senza porta per l'RP.
fn webauthn_site(headers: &axum::http::HeaderMap) -> (String, Vec<String>) {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let rp = host
        .rsplit_once(':')
        .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
        .map_or(host.as_str(), |(h, _)| h)
        .trim_matches(['[', ']'])
        .to_string();
    (
        rp,
        vec![format!("https://{host}"), format!("http://{host}")],
    )
}

/// Nuova sfida casuale (32 byte) registrata per 5 minuti.
fn new_challenge(state: &Shared, device_id: &str, purpose: &str) -> String {
    let mut raw = [0u8; 32];
    getrandom::getrandom(&mut raw).expect("generatore casuale");
    let challenge = crate::passkeys::b64url(&raw);
    let mut map = state.challenges.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    map.retain(|_, c| c.expires > now);
    map.insert(
        challenge.clone(),
        Challenge {
            device_id: device_id.into(),
            purpose: purpose.into(),
            expires: now + std::time::Duration::from_secs(300),
        },
    );
    challenge
}

/// Consuma una sfida se appartiene al dispositivo e allo scopo indicati e non è scaduta.
fn take_challenge(state: &Shared, challenge: &str, device_id: &str, purpose: &str) -> bool {
    let mut map = state.challenges.lock().unwrap_or_else(|e| e.into_inner());
    match map.remove(challenge) {
        Some(c) => {
            c.device_id == device_id
                && c.purpose == purpose
                && c.expires > std::time::Instant::now()
        }
        None => false,
    }
}

/// Sfida contenuta in un clientDataJSON in base64url (per ritrovare lo scopo prima della verifica completa).
fn client_challenge(client_data_b64: &str) -> Option<String> {
    let raw = crate::passkeys::from_b64url(client_data_b64).ok()?;
    let v: Value = serde_json::from_slice(&raw).ok()?;
    v["challenge"].as_str().map(str::to_string)
}

type Shared = Arc<AppState>;

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({"error": {"code": code, "message": message.into()}})),
    )
        .into_response()
}

impl IntoResponse for CoreError {
    fn into_response(self) -> Response {
        match self {
            CoreError::BadRequest(m) => error(StatusCode::BAD_REQUEST, "bad_request", m),
            CoreError::Unavailable(m) => error(StatusCode::SERVICE_UNAVAILABLE, "unavailable", m),
            CoreError::NotFound(m) => error(StatusCode::NOT_FOUND, "not_found", m),
            CoreError::Gone(m) => error(StatusCode::GONE, "expired", m),
            CoreError::Unauthorized(m) => error(StatusCode::UNAUTHORIZED, "unauthorized", m),
            CoreError::Internal(m) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
        }
    }
}

/// Esegue un'operazione bloccante sul Core (bridge e SQLite sono sincroni).
async fn with_core<T: Send + 'static>(
    state: &Shared,
    f: impl FnOnce(&mut Jarvis) -> T + Send + 'static,
) -> T {
    let core = state.core.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    })
    .await
    .expect("operazione del Core interrotta")
}

async fn guard(State(state): State<Shared>, mut req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let allowed = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
        format!("[::1]:{}", state.port),
    ];
    let local = allowed.iter().any(|a| a == host);
    let host_name = host.rsplit_once(':').map_or(host, |(h, p)| {
        if p.chars().all(|c| c.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    let remote = !local
        && state
            .remote_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host_name));
    if !local && !remote {
        return error(StatusCode::FORBIDDEN, "forbidden_host", "host non ammesso");
    }
    let path = req.uri().path().to_string();
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "non trovato".into());
    let method = req.method().to_string();
    if remote {
        // Dispositivo associato (RF-11): token del dispositivo, solo le rotte dei suoi permessi.
        let public = !path.starts_with("/api/")
            || path == "/api/devices/pair"
            || path.starts_with("/api/hooks/")
            || path.starts_with("/api/nodes/");
        if !public {
            let token = req
                .headers()
                .get("x-jarvis-device")
                .and_then(|t| t.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let device = with_core(&state, move |j| j.device_for_token(&token)).await;
            let Ok(Some(device)) = device else {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "dispositivo non associato o revocato",
                );
            };
            let scopes: Vec<&str> = device["scopes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !device_route_allowed(&method, &route, &scopes) {
                return error(
                    StatusCode::FORBIDDEN,
                    "owner_only",
                    "operazione disponibile solo dal computer del proprietario",
                );
            }
            req.extensions_mut().insert(DeviceCtx(device.clone()));
        }
    }
    // I webhook dei connettori si autenticano con la firma HMAC del corpo, non con il token di sessione.
    // I nodi si autenticano con il proprio token nell'hub; l'associazione con il codice monouso.
    if local
        && path.starts_with("/api/")
        && path != "/api/health"
        && path != "/api/devices/pair"
        && !path.starts_with("/api/hooks/")
        && !path.starts_with("/api/nodes/")
        // Ritorno da OAuth (ADR-0027): navigazione del browser senza token, autenticata dallo `state` monouso.
        && path != "/api/connectors/oauth/callback"
    {
        let token = req
            .headers()
            .get("x-jarvis-token")
            .and_then(|t| t.to_str().ok());
        if token != Some(state.token.as_str()) {
            return error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "token di sessione mancante o errato",
            );
        }
    }
    let timer = crate::observability::Timer::start();
    let mut res = next.run(req).await;
    let status = res.status().as_u16();
    state
        .obs
        .observe(&format!("http {method} {route}"), timer.ms(), status >= 500);
    if status >= 500 {
        state.obs.log(
            "error",
            "http.error",
            json!({"method": method, "route": route, "status": status}),
        );
    }
    let h = res.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("cache-control", HeaderValue::from_static("no-store"));
    h.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; media-src 'self' blob:; manifest-src 'self'; worker-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
    );
    res
}

/// Rotte consentite a un dispositivo associato. Gestione di dispositivi, connettori, backup, audit,
/// workspace e automazioni resta al computer del proprietario.
fn device_route_allowed(method: &str, route: &str, scopes: &[&str]) -> bool {
    let chat = [
        ("GET", "/api/conversation"),
        ("POST", "/api/chat"),
        ("POST", "/api/chat/stream"),
        ("GET", "/api/tasks"),
        ("GET", "/api/confirmations"),
        ("GET", "/api/notifications"),
        ("POST", "/api/notifications/read-all"),
        ("POST", "/api/notifications/{id}/read"),
        ("POST", "/api/voice/transcribe"),
        ("POST", "/api/voice/speak"),
        ("GET", "/api/devices/me"),
        ("GET", "/api/passkeys"),
        ("POST", "/api/passkeys/register/options"),
        ("POST", "/api/passkeys/register"),
        ("POST", "/api/passkeys/assert/options"),
        ("GET", "/api/health"),
    ];
    let allowed = chat.iter().any(|(m, r)| *m == method && *r == route);
    let confirm =
        method == "POST" && route == "/api/confirmations/{id}" && scopes.contains(&"confirm");
    (allowed && scopes.contains(&"chat")) || confirm
}

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/", get(index))
        .route(
            "/app.js",
            get(|h: axum::http::HeaderMap| async move { asset(&h, &JS_ASSET) }),
        )
        .route(
            "/app.css",
            get(|h: axum::http::HeaderMap| async move { asset(&h, &CSS_ASSET) }),
        )
        .route(
            "/voice-worklet.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    VOICE_WORKLET,
                )
            }),
        )
        .route("/api/health", get(health))
        .route("/api/chat", post(chat))
        .route("/api/security", get(security_status))
        .route("/api/n8n", get(n8n_status).post(n8n_runtime))
        .route("/api/n8n/jobs", post(n8n_create))
        .route("/api/n8n/jobs/{id}", delete(n8n_remove))
        .route("/api/cloud", get(cloud_status).post(configure_cloud))
        .route("/api/cloud/chat", post(cloud_chat))
        .route("/api/chat/stream", post(chat_stream))
        .route("/api/updates", get(updates_get).post(updates_set))
        .route("/api/updates/check", post(updates_check))
        .route("/api/notify/channels", get(channels_list).post(channel_add))
        .route("/api/notify/channels/{id}", delete(channel_delete))
        .route("/api/notify/channels/{id}/test", post(channel_test))
        .route("/api/confirmations", get(confirmations))
        .route("/api/confirmations/{id}", post(confirm))
        .route("/api/passkeys", get(passkeys_list))
        .route(
            "/api/passkeys/register/options",
            post(passkey_register_options),
        )
        .route("/api/passkeys/register", post(passkey_register))
        .route("/api/passkeys/assert/options", post(passkey_assert_options))
        .route("/api/passkeys/{id}", axum::routing::delete(passkey_delete))
        .route("/api/tasks", get(tasks))
        .route("/api/tasks/{id}/trace", get(task_trace))
        .route("/api/conversation", get(conversation))
        .route("/api/memory", get(memory))
        .route("/api/memory/graph", get(memory_graph))
        .route("/api/memory/reindex", post(memory_reindex))
        .route("/api/memory/{id}", delete(forget).patch(memory_correct))
        .route("/api/audit", get(audit))
        .route("/api/workspace", get(workspace))
        .route("/api/tools", get(tool_catalog))
        .route("/api/metrics", get(metrics))
        .route("/api/database", get(database))
        .route("/api/metrics/history", get(metrics_history))
        .route("/api/metrics/export", get(metrics_export))
        .route("/api/backups", get(backups))
        .route("/api/backup", post(create_backup))
        .route("/api/backups/verify", post(verify_backup))
        .route(
            "/api/voice/transcribe",
            post(voice_transcribe).layer(axum::extract::DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route("/api/voice/speak", post(voice_speak))
        .route("/api/devices", get(devices))
        .route("/api/devices/pairing", post(create_pairing))
        .route("/api/devices/pair", post(pair_device))
        .route("/api/devices/me", get(device_me))
        .route("/api/setup", get(setup_status))
        .route("/api/setup/start", post(setup_start))
        .route("/api/nodes", get(list_nodes))
        .route("/api/nodes/next", get(node_next))
        .route("/api/nodes/result/{id}", post(node_result))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        .route(
            "/manifest.webmanifest",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/manifest+json")],
                    MANIFEST,
                )
            }),
        )
        .route(
            "/sw.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    SERVICE_WORKER,
                )
            }),
        )
        .route(
            "/pair",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    PAIR_HTML,
                )
            }),
        )
        .route(
            "/boot.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    BOOT_JS,
                )
            }),
        )
        .route(
            "/pair.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    PAIR_JS,
                )
            }),
        )
        .route(
            "/icon.svg",
            get(|| async { ([(header::CONTENT_TYPE, "image/svg+xml")], ICON) }),
        )
        .route("/api/connectors", get(connectors).post(install_connector))
        .route("/api/skills", get(skills_list).post(skill_create))
        .route(
            "/api/skills/{id}",
            axum::routing::put(skill_update).delete(skill_delete),
        )
        .route("/api/connectors/templates", get(connector_templates))
        .route("/api/connectors/templates/{id}", post(install_template))
        .route("/api/connectors/{id}", delete(uninstall_connector))
        .route("/api/connectors/{id}/authorize", post(authorize_connector))
        .route("/api/connectors/{id}/revoke", post(revoke_connector))
        .route("/api/connectors/{id}/oauth/start", post(oauth_start))
        .route("/api/connectors/oauth/callback", get(oauth_callback))
        .route(
            "/api/hooks/{id}",
            post(connector_hook).layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/notifications", get(notifications))
        .route("/api/notifications/read-all", post(read_all))
        .route("/api/notifications/{id}/read", post(read_one))
        .route("/api/workflows", get(workflows).post(create_workflow))
        .route(
            "/api/workflows/{id}",
            delete(delete_workflow).put(update_workflow),
        )
        .route("/api/workflows/{id}/versions", get(workflow_versions))
        .route("/api/workflows/{id}/enabled", post(set_enabled))
        .route("/api/workflows/{id}/run", post(run_now))
        .route("/api/workflows/{id}/runs", get(workflow_runs))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

async fn index(State(state): State<Shared>, headers: axum::http::HeaderMap) -> Response {
    // Il token di sessione va solo al browser del computer del Core; un dispositivo remoto si associa.
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let local = ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|h| host == format!("{h}:{}", state.port));
    let html = INDEX.replace("{{JARVIS_TOKEN}}", if local { &state.token } else { "" });
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

async fn health(State(state): State<Shared>) -> Json<Value> {
    let core = state.core.clone();
    let fresh = tokio::task::spawn_blocking(move || core.try_lock().ok().map(|mut j| j.health()))
        .await
        .ok()
        .flatten();
    let mut cached = state.last_health.lock().unwrap_or_else(|e| e.into_inner());
    match fresh {
        Some(h) => {
            *cached = h.clone();
            Json(h)
        }
        None => {
            let mut h = cached.clone();
            h["busy"] = true.into();
            Json(h)
        }
    }
}

async fn n8n_action(
    state: &Shared,
    audit: Option<&'static str>,
    action: impl FnOnce(&crate::n8n::Manager) -> Result<Value, String> + Send + 'static,
) -> Response {
    let manager = state.n8n.clone();
    let result = tokio::task::spawn_blocking(move || action(&manager)).await;
    match result {
        Ok(Ok(view)) => {
            if let Some(action) = audit {
                if let Err(error) = with_core(state, move |j| j.record_owner_action(action)).await {
                    return error.into_response();
                }
            }
            Json(view).into_response()
        }
        Ok(Err(error)) => CoreError::BadRequest(error).into_response(),
        Err(_) => CoreError::Internal("operazione n8n interrotta".into()).into_response(),
    }
}

async fn n8n_status(State(state): State<Shared>) -> Response {
    n8n_action(&state, None, |manager| Ok(manager.status())).await
}

async fn n8n_runtime(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    if body["enable"] == true {
        return n8n_action(&state, Some("n8n.enable"), |manager| manager.enable()).await;
    }
    let Some(running) = body["running"].as_bool() else {
        return CoreError::BadRequest("stato del container mancante".into()).into_response();
    };
    let action = if running { "n8n.start" } else { "n8n.stop" };
    n8n_action(&state, Some(action), move |manager| {
        manager.set_running(running)
    })
    .await
}

async fn n8n_create(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    n8n_action(&state, Some("n8n.create"), move |manager| {
        manager.add(&body)
    })
    .await
}

async fn n8n_remove(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    n8n_action(&state, Some("n8n.remove"), move |manager| {
        manager.remove(&id)
    })
    .await
}

async fn security_status(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.security_status()).await)
}

async fn cloud_status(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.cloud_status()).await)
}

async fn configure_cloud(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.configure_cloud(&body)).await)
}

async fn cloud_chat(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let request = match with_core(&state, move |j| j.prepare_cloud_chat(&body)).await {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let timer = crate::observability::Timer::start();
    let completed = tokio::task::spawn_blocking(move || {
        let completed = request.execute();
        (request.message, completed)
    })
    .await;
    let Ok((message, completed)) = completed else {
        return CoreError::Internal("richiesta cloud interrotta".into()).into_response();
    };
    match with_core(&state, move |j| {
        j.finish_cloud_chat(message, completed, timer)
    })
    .await
    {
        Ok(out) => Json(out.to_json()).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn chat(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(message) = body["message"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'message' mancante",
        );
    };
    match with_core(&state, move |j| j.chat(&message)).await {
        Ok(out) => Json(out.to_json()).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Chat in streaming (ADR-0025): risposta NDJSON con righe `{"type":"delta","text":…}` mentre il modello
/// genera, poi `{"type":"done","result":…}` (lo stesso oggetto di `/api/chat`) oppure `{"type":"error",…}`.
async fn chat_stream(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(message) = body["message"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'message' mancante",
        );
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let core = state.core.clone();
    let deltas = tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
        let sink: crate::orchestrator::StreamSink = Box::new(move |d: &str| {
            let _ = deltas.send(json!({"type": "delta", "text": d}).to_string() + "\n");
        });
        let line = match guard.chat_streaming(&message, sink) {
            Ok(out) => json!({"type": "done", "result": out.to_json()}),
            Err(e) => {
                let (code, message) = match e {
                    CoreError::BadRequest(m) => ("bad_request", m),
                    CoreError::Unavailable(m) => ("unavailable", m),
                    CoreError::NotFound(m) => ("not_found", m),
                    CoreError::Gone(m) => ("expired", m),
                    CoreError::Unauthorized(m) => ("unauthorized", m),
                    CoreError::Internal(m) => ("internal", m),
                };
                json!({"type": "error", "error": {"code": code, "message": message}})
            }
        };
        let _ = tx.send(line.to_string() + "\n");
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|line| (Ok::<_, std::convert::Infallible>(line), rx))
    });
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

async fn oauth_start(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let secret = body["client_secret"].as_str().map(str::to_string);
    match with_core(&state, move |j| j.oauth_start(&id, secret)).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn oauth_callback(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let st = q.get("state").cloned().unwrap_or_default();
    let code = q.get("code").cloned();
    let denied = q.get("error").cloned();
    let result = with_core(&state, move |j| {
        j.oauth_callback(&st, code.as_deref(), denied.as_deref())
    })
    .await;
    let (status, html) = match result {
        Ok(name) => (
            StatusCode::OK,
            crate::oauth::page(true, &format!("{name} è collegato a JARVIS.")),
        ),
        Err(CoreError::BadRequest(m) | CoreError::NotFound(m)) => {
            (StatusCode::BAD_REQUEST, crate::oauth::page(false, &m))
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::oauth::page(false, "errore interno"),
        ),
    };
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

async fn updates_get(State(state): State<Shared>) -> Json<Value> {
    Json(with_core(&state, |j| j.updates_state()).await)
}

async fn updates_set(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let enabled = body["enabled"].as_bool().unwrap_or(false);
    let channel = body["channel"].as_str().unwrap_or("stable").to_string();
    reply(with_core(&state, move |j| j.updates_configure(enabled, &channel)).await)
}

/// Controllo manuale: la richiesta a GitHub avviene fuori dal blocco del Core.
async fn updates_check(State(state): State<Shared>) -> Response {
    let fetched = fetch_releases(&state).await;
    reply(with_core(&state, move |j| j.updates_record(fetched)).await)
}

async fn fetch_releases(state: &Shared) -> Result<Vec<Value>, String> {
    let dir = with_core(state, |j| j.data_dir()).await;
    tokio::task::spawn_blocking(move || crate::updates::Updates::new(&dir).fetch())
        .await
        .unwrap_or_else(|_| Err("controllo interrotto".into()))
}

async fn channels_list(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.channels()).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn channel_add(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    match with_core(&state, move |j| j.channel_add(&body)).await {
        Ok(doc) => (StatusCode::CREATED, Json(doc)).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn channel_delete(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    match with_core(&state, move |j| j.channel_delete(&id)).await {
        Ok(()) => Json(json!({"deleted": true})).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Prova d'invio: il messaggio parte fuori dal blocco del Core e l'esito torna all'interfaccia.
async fn channel_test(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    let (channel, secret) = match with_core(&state, move |j| j.channel_for_test(&id)).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let n = json!({"severity": "info", "category": "system", "title": "Prova di notifica",
                   "body": "Se leggi questo messaggio, JARVIS può raggiungerti sul telefono."});
    let sent = tokio::task::spawn_blocking(move || {
        let url = std::env::var("JARVIS_PUBLIC_URL")
            .ok()
            .filter(|u| u.starts_with("https://"));
        crate::channels::send(&channel, secret.as_deref(), &n, url.as_deref())
    })
    .await
    .unwrap_or_else(|_| Err("invio interrotto".into()));
    match sent {
        Ok(()) => Json(json!({"sent": true})).into_response(),
        Err(m) => error(StatusCode::BAD_GATEWAY, "notify.failed", m),
    }
}

async fn confirmations(State(state): State<Shared>) -> Json<Value> {
    Json(json!({"items": with_core(&state, |j| j.pending_confirmations()).await}))
}

async fn confirm(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let Some(approve) = body["approve"].as_bool() else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'approve' booleano mancante",
        );
    };
    // Passkey (ADR-0042): da un dispositivo con passkey registrata, approvare richiede un'asserzione legata
    // a questa conferma. Rifiutare resta sempre possibile senza passkey.
    if let (true, Some(axum::Extension(DeviceCtx(device)))) = (approve, device) {
        let device_id = device["device_id"].as_str().unwrap_or_default().to_string();
        let did = device_id.clone();
        let has = with_core(&state, move |j| j.passkeys(&did)).await;
        if has.is_ok_and(|k| !k.is_empty()) {
            let p = &body["passkey"];
            let (Some(cid), Some(cd), Some(ad), Some(sig)) = (
                p["id"].as_str(),
                p["clientDataJSON"].as_str(),
                p["authenticatorData"].as_str(),
                p["signature"].as_str(),
            ) else {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "passkey_required",
                    "conferma con la passkey di questo dispositivo",
                );
            };
            let challenge = client_challenge(cd).unwrap_or_default();
            if !take_challenge(&state, &challenge, &device_id, &format!("confirm:{id}")) {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "passkey_invalid",
                    "sfida della passkey scaduta o non valida per questa conferma",
                );
            }
            let decoded = (
                crate::passkeys::from_b64url(cd),
                crate::passkeys::from_b64url(ad),
                crate::passkeys::from_b64url(sig),
            );
            let (Ok(cd), Ok(ad), Ok(sig)) = decoded else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "asserzione non codificata in base64url",
                );
            };
            let (rp, origins) = webauthn_site(&headers);
            let cid = cid.to_string();
            let checked = with_core(&state, move |j| {
                j.passkey_check(&device_id, &cid, &cd, &ad, &sig, &challenge, &origins, &rp)
            })
            .await;
            if let Err(e) = checked {
                let msg = match e {
                    CoreError::Unauthorized(m) => m,
                    other => format!("{other:?}"),
                };
                return error(StatusCode::UNAUTHORIZED, "passkey_invalid", &msg);
            }
        }
    }
    match with_core(&state, move |j| j.confirm(&id, approve)).await {
        Ok(out) => Json(out.to_json()).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Passkey: del dispositivo che chiede, oppure di tutti i dispositivi per il proprietario.
async fn passkeys_list(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    match device {
        Some(axum::Extension(DeviceCtx(d))) => {
            let id = d["device_id"].as_str().unwrap_or_default().to_string();
            reply(
                with_core(&state, move |j| {
                    j.passkeys(&id).map(|items| json!({"items": items}))
                })
                .await,
            )
        }
        None => reply(
            with_core(&state, |j| {
                let devices = j.devices()?;
                let mut items = Vec::new();
                for d in devices {
                    let id = d["device_id"].as_str().unwrap_or_default().to_string();
                    for mut k in j.passkeys(&id)? {
                        k["device_id"] = id.clone().into();
                        k["device_name"] = d["name"].clone();
                        items.push(k);
                    }
                }
                Ok(json!({"items": items}))
            })
            .await,
        ),
    }
}

fn device_only(device: Option<axum::Extension<DeviceCtx>>) -> Option<Value> {
    device.map(|axum::Extension(DeviceCtx(d))| d)
}

fn not_a_device() -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "device_only",
        "le passkey si registrano dal dispositivo associato",
    )
}

/// Opzioni per `navigator.credentials.create` (ES256, verifica dell'utente obbligatoria).
async fn passkey_register_options(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(d) = device_only(device) else {
        return not_a_device();
    };
    let id = d["device_id"].as_str().unwrap_or_default();
    let (rp, _) = webauthn_site(&headers);
    let challenge = new_challenge(&state, id, "register");
    let did = id.to_string();
    let existing = with_core(&state, move |j| j.passkeys(&did))
        .await
        .unwrap_or_default();
    Json(json!({
        "challenge": challenge,
        "rp": {"id": rp, "name": "JARVIS"},
        "user": {"id": crate::passkeys::b64url(id.as_bytes()), "name": d["name"], "displayName": d["name"]},
        "pubKeyCredParams": [{"type": "public-key", "alg": -7}],
        "authenticatorSelection": {"userVerification": "required", "residentKey": "preferred"},
        "excludeCredentials": existing.iter().map(|k| json!({"type": "public-key", "id": k["credential_id"]})).collect::<Vec<_>>(),
        "attestation": "none",
        "timeout": 120000,
    }))
    .into_response()
}

/// Verifica e salva la passkey creata dal dispositivo.
async fn passkey_register(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(d) = device_only(device) else {
        return not_a_device();
    };
    let id = d["device_id"].as_str().unwrap_or_default().to_string();
    let (Some(cd), Some(att)) = (
        body["clientDataJSON"].as_str(),
        body["attestationObject"].as_str(),
    ) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "risposta della passkey incompleta",
        );
    };
    let challenge = client_challenge(cd).unwrap_or_default();
    if !take_challenge(&state, &challenge, &id, "register") {
        return error(
            StatusCode::UNAUTHORIZED,
            "passkey_invalid",
            "sfida di registrazione scaduta o non valida",
        );
    }
    let (Ok(cd), Ok(att)) = (
        crate::passkeys::from_b64url(cd),
        crate::passkeys::from_b64url(att),
    ) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "risposta non codificata in base64url",
        );
    };
    let (rp, origins) = webauthn_site(&headers);
    let reg = match crate::passkeys::verify_registration(&cd, &att, &challenge, &origins, &rp) {
        Ok(r) => r,
        Err(e) => return error(StatusCode::BAD_REQUEST, "passkey_invalid", &e),
    };
    reply(with_core(&state, move |j| j.passkey_register(&id, &reg)).await)
}

/// Opzioni per `navigator.credentials.get`, legate a una conferma in attesa.
async fn passkey_assert_options(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(d) = device_only(device) else {
        return not_a_device();
    };
    let Some(conf) = body["confirmation_id"].as_str() else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "confirmation_id mancante",
        );
    };
    let id = d["device_id"].as_str().unwrap_or_default().to_string();
    let challenge = new_challenge(&state, &id, &format!("confirm:{conf}"));
    let (rp, _) = webauthn_site(&headers);
    let keys = with_core(&state, move |j| j.passkeys(&id))
        .await
        .unwrap_or_default();
    Json(json!({
        "challenge": challenge, "rpId": rp, "userVerification": "required", "timeout": 120000,
        "allowCredentials": keys.iter().map(|k| json!({"type": "public-key", "id": k["credential_id"]})).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// Eliminazione di una passkey: solo il proprietario dal computer del Core.
async fn passkey_delete(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Path(id): Path<String>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply(
        with_core(&state, move |j| {
            j.passkey_delete(&id).map(|_| json!({"deleted": true}))
        })
        .await,
    )
}

/// Catalogo dei connettori pronti (ADR-0047).
async fn skills_list(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.skills()).await)
}

async fn skill_create(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.skill_save(None, &body)).await)
}

async fn skill_update(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    reply(with_core(&state, move |j| j.skill_save(Some(&id), &body)).await)
}

async fn skill_delete(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.skill_delete(&id)).await)
}

async fn connector_templates() -> Json<Value> {
    Json(crate::connector_catalog::index())
}

/// Installa un connettore del catalogo con l'ID client OAuth del proprietario.
async fn install_template(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let manifest = match crate::connector_catalog::manifest(&id, body["client_id"].as_str()) {
        Ok(m) => m,
        Err(e) => return error(StatusCode::BAD_REQUEST, "bad_request", &e),
    };
    reply(with_core(&state, move |j| j.install_connector(&manifest)).await)
}

fn limit(q: &Value, default: u32) -> u32 {
    q["limit"]
        .as_str()
        .and_then(|l| l.parse().ok())
        .unwrap_or(default)
        .clamp(1, 500)
}

async fn tasks(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let n = limit(&json!(q), 50);
    match with_core(&state, move |j| j.tasks(n)).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Traccia di un task (ADR-0039): intervalli W3C con durata ed esito, senza contenuti.
async fn task_trace(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.task_trace(&id)).await)
}

async fn conversation(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let n = limit(&json!(q), 100);
    match with_core(&state, move |j| j.conversation(n)).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn memory(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let kind = q.get("kind").cloned().filter(|k| !k.is_empty());
    let query = q.get("q").cloned();
    match with_core(&state, move |j| {
        j.memory_query(kind.as_deref(), query.as_deref())
    })
    .await
    {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn memory_graph(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.memory_graph()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn memory_reindex(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.memory_reindex()).await {
        Ok(n) => Json(json!({"reindexed": n})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn memory_correct(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let text = body["text"].as_str().unwrap_or_default().to_string();
    match with_core(&state, move |j| j.memory_correct(&id, &text)).await {
        Ok(()) => Json(json!({"updated": true})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn forget(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    match with_core(&state, move |j| j.forget(&id)).await {
        Ok(()) => Json(json!({"deleted": true})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn audit(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let n = limit(&json!(q), 100) as usize;
    match with_core(&state, move |j| j.audit_log(n)).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn workspace(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.workspace_listing()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn backups(State(state): State<Shared>) -> Json<Value> {
    Json(with_core(&state, |j| j.backups()).await)
}

async fn create_backup(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(pass) = body["passphrase"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'passphrase' mancante",
        );
    };
    reply(with_core(&state, move |j| j.create_backup(&pass)).await)
}

async fn verify_backup(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some((name, pass)) = body["file"].as_str().zip(body["passphrase"].as_str()) else {
        return CoreError::BadRequest("file e passphrase sono obbligatori".into()).into_response();
    };
    let (name, pass) = (name.to_owned(), pass.to_owned());
    reply(
        with_core(&state, move |j| {
            crate::backup::verify_saved(&j.backup_directory(), &name, &pass)
                .map_err(CoreError::BadRequest)
        })
        .await,
    )
}

async fn metrics(State(state): State<Shared>) -> Json<Value> {
    Json(state.obs.snapshot())
}

/// Stato del database locale (ADR-0037): integrità, versione dello schema, dimensione e righe per tabella.
async fn database(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.database_health()).await)
}

async fn metrics_history(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let hours = q.get("hours").and_then(|h| h.parse().ok()).unwrap_or(24);
    reply(
        with_core(&state, move |j| {
            j.metrics_history(hours)
                .map(|items| json!({"items": items}))
        })
        .await,
    )
}

/// Esportazione: `format=prometheus` (testo 0.0.4) oppure JSON con stato attuale e storico di 7 giorni.
async fn metrics_export(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let snap = state.obs.snapshot();
    if q.get("format").map(String::as_str) == Some("prometheus") {
        return (
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            crate::observability::prometheus(&snap),
        )
            .into_response();
    }
    let history = with_core(&state, |j| j.metrics_history(24 * 7)).await;
    match history {
        Ok(items) => (
            [(header::CONTENT_DISPOSITION, "attachment; filename=\"jarvis-metrics.json\"")],
            Json(json!({"format": "jarvis-metrics/1", "exported_at": crate::now(), "current": snap, "history": items})),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn tool_catalog(State(state): State<Shared>) -> Json<Value> {
    Json(json!({"items": with_core(&state, |j| j.tool_catalog()).await}))
}

fn reply(result: Result<Value, CoreError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn voice_transcribe(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    let wake = q.get("mode").is_some_and(|m| m == "wake_word");
    let audio = body.to_vec();
    reply(with_core(&state, move |j| j.voice_transcribe(&audio, wake)).await)
}

async fn voice_speak(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(text) = body["text"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'text' mancante",
        );
    };
    let with_visemes = body["with_visemes"] == true;
    match with_core(&state, move |j| j.voice_speak(&text)).await {
        // Labiale (ADR-0029): audio e visemi insieme, così l'avatar li sincronizza con la riproduzione.
        Ok((audio, mut meta)) if with_visemes => {
            use base64::Engine as _;
            meta["audio_b64"] = base64::engine::general_purpose::STANDARD
                .encode(&audio)
                .into();
            Json(meta).into_response()
        }
        Ok((audio, meta)) => {
            let engine = format!(
                "{}/{}",
                meta["engine"]["name"].as_str().unwrap_or("?"),
                meta["voice"].as_str().unwrap_or("?")
            );
            let mut res = ([(header::CONTENT_TYPE, "audio/wav")], audio).into_response();
            if let Ok(v) = HeaderValue::from_str(&engine) {
                res.headers_mut().insert("x-jarvis-voice", v);
            }
            res
        }
        Err(e) => e.into_response(),
    }
}

async fn notifications(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let n = limit(&json!(q), 50);
    let unread = q.get("unread").is_some_and(|v| v == "1" || v == "true");
    let since = q.get("since").cloned();
    reply(
        with_core(&state, move |j| {
            j.notifications(n, unread, since.as_deref())
        })
        .await,
    )
}

async fn read_one(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.mark_notification_read(&id)).await)
}

async fn read_all(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.mark_all_notifications_read()).await)
}

async fn workflows(State(state): State<Shared>) -> Response {
    reply(
        with_core(&state, |j| {
            j.workflows().map(|items| json!({"items": items}))
        })
        .await,
    )
}

async fn devices(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.devices().map(|items| json!({"items": items}))).await)
}

async fn create_pairing(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let scopes: Vec<String> = body["scopes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    reply(with_core(&state, move |j| j.create_pairing(&scopes)).await)
}

async fn pair_device(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let code = body["code"].as_str().unwrap_or_default().to_string();
    let name = body["name"].as_str().unwrap_or_default().to_string();
    let platform = body["platform"].as_str().unwrap_or("other").to_string();
    let capabilities: Vec<String> = body["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect();
    let cert = body["cert_pem"].as_str().map(str::to_string);
    reply(
        with_core(&state, move |j| {
            j.pair_device(&code, &name, &platform, &capabilities, cert.as_deref())
        })
        .await,
    )
}

async fn setup_status(State(state): State<Shared>) -> Json<Value> {
    let setup = state.setup.clone();
    Json(
        tokio::task::spawn_blocking(move || setup.status())
            .await
            .unwrap_or(Value::Null),
    )
}

async fn setup_start(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let plan = match crate::setup::Plan::from_json(&body) {
        Ok(p) => p,
        Err(e) => return error(StatusCode::BAD_REQUEST, "bad_request", &e),
    };
    match state.setup.start(plan) {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({"started": true}))).into_response(),
        Err(e) => error(StatusCode::CONFLICT, "conflict", &e),
    }
}

async fn list_nodes(State(state): State<Shared>, headers: axum::http::HeaderMap) -> Response {
    // Elenco riservato al proprietario: la rotta è esente dal controllo generale, quindi lo si ripete qui.
    if headers.get("x-jarvis-token").and_then(|t| t.to_str().ok()) != Some(state.token.as_str()) {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "token di sessione mancante o errato",
        );
    }
    reply(with_core(&state, |j| j.nodes().map(|items| json!({"items": items}))).await)
}

fn node_id(
    state: &Shared,
    headers: &axum::http::HeaderMap,
    peer: Option<&crate::tls::PeerCert>,
) -> Option<String> {
    let token = headers
        .get("x-jarvis-device")
        .and_then(|t| t.to_str().ok())?;
    state
        .nodes
        .authenticate_peer(token, peer.and_then(|p| p.0.as_deref()))
}

/// Router della porta TLS dei nodi (ADR-0041): solo le rotte dell'hub, autenticate da certificato e token.
pub fn node_tls_router(state: Shared) -> Router {
    Router::new()
        .route("/api/nodes/next", get(node_next))
        .route("/api/nodes/result/{id}", post(node_result))
        .with_state(state)
}

/// Serve la porta TLS dei nodi finché il processo è attivo.
pub async fn serve_node_tls(
    state: Shared,
    listener: tokio::net::TcpListener,
    identity: crate::tls::Identity,
) -> Result<(), String> {
    let hub = state.nodes.clone();
    let config = crate::tls::server_config(&identity, Arc::new(move |fp| hub.cert_allowed(fp)))?;
    let tls = crate::tls::TlsListener {
        tcp: listener,
        acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
    };
    axum::serve(
        tls,
        node_tls_router(state).into_make_service_with_connect_info::<crate::tls::PeerCert>(),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Attesa lunga del nodo (al massimo 25 s): 200 con l'assegnazione oppure 204.
async fn node_next(
    State(state): State<Shared>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<crate::tls::PeerCert>>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(id) = node_id(&state, &headers, peer.as_ref().map(|p| &p.0 .0)) else {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "nodo non associato o revocato",
        );
    };
    let wait = q
        .get("wait")
        .and_then(|w| w.parse().ok())
        .unwrap_or(20u64)
        .min(25);
    let hub = state.nodes.clone();
    // Risorse dichiarate dal nodo (ADR-0031): servono alla scelta automatica.
    hub.report(
        &id,
        crate::nodes::Resources {
            cpus: q
                .get("cpus")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .clamp(1, 1024),
            load: q
                .get("load")
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|l| l.is_finite() && *l >= 0.0),
            mem_free_mb: q.get("mem_free_mb").and_then(|v| v.parse().ok()),
        },
    );
    let next =
        tokio::task::spawn_blocking(move || hub.next(&id, std::time::Duration::from_secs(wait)))
            .await
            .ok()
            .flatten();
    match next {
        Some(a) => Json(a.to_json()).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn node_result(
    State(state): State<Shared>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<crate::tls::PeerCert>>>,
    Path(assignment): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(id) = node_id(&state, &headers, peer.as_ref().map(|p| &p.0 .0)) else {
        return error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "nodo non associato o revocato",
        );
    };
    let outcome = if body["ok"] == true {
        Ok(body["output"].clone())
    } else {
        Err((
            body["code"]
                .as_str()
                .unwrap_or("node.tool_failed")
                .chars()
                .take(64)
                .collect(),
            body["message"]
                .as_str()
                .unwrap_or("errore sul nodo")
                .chars()
                .take(500)
                .collect(),
        ))
    };
    if state.nodes.complete(&id, &assignment, outcome) {
        Json(json!({"accepted": true})).into_response()
    } else {
        error(
            StatusCode::NOT_FOUND,
            "not_found",
            "assegnazione sconosciuta o scaduta",
        )
    }
}

async fn device_me(State(state): State<Shared>, headers: axum::http::HeaderMap) -> Response {
    let token = headers
        .get("x-jarvis-device")
        .and_then(|t| t.to_str().ok())
        .unwrap_or_default()
        .to_string();
    reply(
        with_core(&state, move |j| {
            j.device_for_token(&token)?.ok_or_else(|| {
                CoreError::NotFound("nessun dispositivo per questa richiesta".into())
            })
        })
        .await,
    )
}

async fn revoke_device(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    // Il Core può avere il lock mentre attende l'esito proprio da questo nodo.
    let result = tokio::task::spawn_blocking(move || {
        let (doc, changed) =
            crate::store::Store::revoke_device(&state.device_db, &id, &crate::now())
                .map_err(|e| CoreError::Internal(e.to_string()))?
                .ok_or_else(|| CoreError::NotFound(format!("nessun dispositivo {id}")))?;
        if doc["slug"].is_string() {
            state.nodes.revoke(&id);
        }
        if changed {
            state
                .core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .record_device_revocation(&id, &doc)?;
        }
        Ok(doc)
    })
    .await
    .unwrap_or_else(|_| {
        Err(CoreError::Internal(
            "operazione di revoca interrotta".into(),
        ))
    });
    reply(result)
}

async fn connectors(State(state): State<Shared>) -> Response {
    reply(
        with_core(&state, |j| {
            j.connectors().map(|items| json!({"items": items}))
        })
        .await,
    )
}

async fn install_connector(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.install_connector(&body)).await)
}

async fn authorize_connector(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let credential = body["credential"].as_str().unwrap_or_default().to_string();
    reply(with_core(&state, move |j| j.authorize_connector(&id, &credential)).await)
}

async fn revoke_connector(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.revoke_connector(&id, false)).await)
}

async fn uninstall_connector(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.revoke_connector(&id, true)).await)
}

async fn connector_hook(
    State(state): State<Shared>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let signature = headers
        .get("x-jarvis-signature")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    reply(
        with_core(&state, move |j| {
            j.connector_webhook(&id, &body, signature.as_deref())
        })
        .await,
    )
}

async fn create_workflow(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.create_workflow(&body)).await)
}

async fn update_workflow(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    reply(with_core(&state, move |j| j.update_workflow(&id, &body)).await)
}

async fn workflow_versions(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(
        with_core(&state, move |j| {
            j.workflow_versions(&id)
                .map(|items| json!({"items": items}))
        })
        .await,
    )
}

async fn delete_workflow(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(
        with_core(&state, move |j| {
            j.delete_workflow(&id).map(|()| json!({"deleted": true}))
        })
        .await,
    )
}

async fn set_enabled(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let Some(enabled) = body["enabled"].as_bool() else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'enabled' booleano mancante",
        );
    };
    reply(with_core(&state, move |j| j.set_workflow_enabled(&id, enabled)).await)
}

async fn run_now(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.run_workflow_now(&id)).await)
}

async fn workflow_runs(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(
        with_core(&state, move |j| {
            j.workflow_runs(&id, 20)
                .map(|items| json!({"items": items}))
        })
        .await,
    )
}

/// Pianificatore: a ogni intervallo esegue le automazioni scadute (con la policy vigente).
pub fn spawn_scheduler(state: Shared, tick: std::time::Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let result = with_core(&state, |j| {
                let runs = j.run_due(chrono::Utc::now());
                if let Err(e) = j.monitor() {
                    eprintln!("[monitor] errore: {e:?}");
                }
                runs
            })
            .await;
            if state.n8n.enabled() {
                let manager = state.n8n.clone();
                if let Ok(completed) =
                    tokio::task::spawn_blocking(move || manager.completed()).await
                {
                    if !completed.is_empty() {
                        let notices = completed.clone();
                        if with_core(&state, move |j| j.n8n_notices(&notices))
                            .await
                            .is_ok()
                        {
                            let manager = state.n8n.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                manager.acknowledge(&completed)
                            })
                            .await;
                        }
                    }
                }
            }
            // Aggiornamenti (ADR-0034): una volta al giorno, solo se attivato, con la rete fuori dal blocco.
            if with_core(&state, |j| j.updates_due()).await {
                let fetched = fetch_releases(&state).await;
                if let Err(e) = with_core(&state, move |j| j.updates_record(fetched)).await {
                    eprintln!("[aggiornamenti] errore: {e:?}");
                }
            }
            match result {
                Ok(runs) => {
                    for run in runs {
                        println!(
                            "[automazioni] {} → {}",
                            run["workflow_id"].as_str().unwrap_or("?"),
                            run["status"].as_str().unwrap_or("?")
                        );
                    }
                }
                Err(e) => eprintln!("[automazioni] errore del pianificatore: {e:?}"),
            }
        }
    })
}

/// Usato dai test per costruire lo stato senza aprire una porta.
pub fn state(core: Jarvis, token: String, port: u16) -> Shared {
    state_with_remote(core, token, port, vec![])
}

pub fn state_with_remote(
    core: Jarvis,
    token: String,
    port: u16,
    remote_hosts: Vec<String>,
) -> Shared {
    let obs = core.observability();
    let nodes = core.node_hub();
    let setup = core.setup();
    let n8n = core.n8n();
    let device_db = core.data_dir().join("jarvis.sqlite3");
    Arc::new(AppState {
        core: Arc::new(Mutex::new(core)),
        obs,
        token,
        port,
        last_health: Mutex::new(json!({})),
        remote_hosts,
        nodes,
        device_db,
        setup,
        n8n,
        challenges: Mutex::new(std::collections::HashMap::new()),
    })
}
