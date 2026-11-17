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

pub struct AppState {
    pub core: Arc<Mutex<Jarvis>>,
    pub token: String,
    pub port: u16,
    pub last_health: Mutex<Value>,
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
            CoreError::NotFound(m) => error(StatusCode::NOT_FOUND, "not_found", m),
            CoreError::Gone(m) => error(StatusCode::GONE, "expired", m),
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
    if !allowed.iter().any(|a| a == host) {
        return error(StatusCode::FORBIDDEN, "forbidden_host", "host non ammesso");
    }
    let path = req.uri().path();
    if path.starts_with("/api/") && path != "/api/health" {
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
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("cache-control", HeaderValue::from_static("no-store"));
    h.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
    );
    res
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
        .route("/api/health", get(health))
        .route("/api/chat", post(chat))
        .route("/api/confirmations", get(confirmations))
        .route("/api/confirmations/{id}", post(confirm))
        .route("/api/tasks", get(tasks))
        .route("/api/conversation", get(conversation))
        .route("/api/memory", get(memory))
        .route("/api/memory/{id}", delete(forget))
        .route("/api/audit", get(audit))
        .route("/api/workspace", get(workspace))
        .route("/api/tools", get(tool_catalog))
        .route("/api/workflows", get(workflows).post(create_workflow))
        .route("/api/workflows/{id}", delete(delete_workflow))
        .route("/api/workflows/{id}/enabled", post(set_enabled))
        .route("/api/workflows/{id}/run", post(run_now))
        .route("/api/workflows/{id}/runs", get(workflow_runs))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

async fn index(State(state): State<Shared>) -> Response {
    let html = INDEX.replace("{{JARVIS_TOKEN}}", &state.token);
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

async fn memory(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.memory()).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
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

async fn tool_catalog(State(state): State<Shared>) -> Json<Value> {
    Json(json!({"items": with_core(&state, |j| j.tool_catalog()).await}))
}

fn reply(result: Result<Value, CoreError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn workflows(State(state): State<Shared>) -> Response {
    reply(
        with_core(&state, |j| {
            j.workflows().map(|items| json!({"items": items}))
        })
        .await,
    )
}

async fn create_workflow(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.create_workflow(&body)).await)
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
            let result = with_core(&state, |j| j.run_due(chrono::Utc::now())).await;
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
    Arc::new(AppState {
        core: Arc::new(Mutex::new(core)),
        token,
        port,
        last_health: Mutex::new(json!({})),
    })
}
