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
    /// Configurazione guidata dei modelli: stato letto senza il blocco del Core.
    pub setup: Arc<crate::setup::Setup>,
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

async fn guard(State(state): State<Shared>, req: Request, next: Next) -> Response {
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
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    APP_JS,
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS) }),
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
        .route("/api/chat/stream", post(chat_stream))
        .route("/api/notify/channels", get(channels_list).post(channel_add))
        .route("/api/notify/channels/{id}", delete(channel_delete))
        .route("/api/notify/channels/{id}/test", post(channel_test))
        .route("/api/confirmations", get(confirmations))
        .route("/api/confirmations/{id}", post(confirm))
        .route("/api/tasks", get(tasks))
        .route("/api/conversation", get(conversation))
        .route("/api/memory", get(memory))
        .route("/api/memory/graph", get(memory_graph))
        .route("/api/memory/reindex", post(memory_reindex))
        .route("/api/memory/{id}", delete(forget).patch(memory_correct))
        .route("/api/audit", get(audit))
        .route("/api/workspace", get(workspace))
        .route("/api/tools", get(tool_catalog))
        .route("/api/metrics", get(metrics))
        .route("/api/metrics/history", get(metrics_history))
        .route("/api/metrics/export", get(metrics_export))
        .route("/api/backups", get(backups))
        .route("/api/backup", post(create_backup))
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
    match with_core(&state, move |j| j.confirm(&id, approve)).await {
        Ok(out) => Json(out.to_json()).into_response(),
        Err(e) => e.into_response(),
    }
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

async fn metrics(State(state): State<Shared>) -> Json<Value> {
    Json(state.obs.snapshot())
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
            meta["audio_b64"] = base64::engine::general_purpose::STANDARD.encode(&audio).into();
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
    reply(
        with_core(&state, move |j| {
            j.pair_device(&code, &name, &platform, &capabilities)
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

fn node_id(state: &Shared, headers: &axum::http::HeaderMap) -> Option<String> {
    let token = headers
        .get("x-jarvis-device")
        .and_then(|t| t.to_str().ok())?;
    state.nodes.authenticate(token)
}

/// Attesa lunga del nodo (al massimo 25 s): 200 con l'assegnazione oppure 204.
async fn node_next(
    State(state): State<Shared>,
    headers: axum::http::HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(id) = node_id(&state, &headers) else {
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
    Path(assignment): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(id) = node_id(&state, &headers) else {
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
    reply(with_core(&state, move |j| j.revoke_device(&id)).await)
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
    Arc::new(AppState {
        core: Arc::new(Mutex::new(core)),
        obs,
        token,
        port,
        last_health: Mutex::new(json!({})),
        remote_hosts,
        nodes,
        setup,
    })
}
