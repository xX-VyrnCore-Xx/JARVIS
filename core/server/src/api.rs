//! API HTTP locale per i client desktop e mobili.
//!
//! Difese del prototipo: ascolto solo su loopback; controllo dell'header `Host`
//! contro il DNS rebinding; token di sessione casuale richiesto da ogni `/api/*`
//! (eccetto `/api/health`) e nessun CORS.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::orchestrator::{CoreError, Jarvis};

pub struct AppState {
    pub core: Arc<Mutex<Jarvis>>,
    pub obs: Arc<crate::observability::Observability>,
    pub token: String,
    pub port: u16,
    pub last_health: Mutex<Value>,
    cloud_slots: Arc<tokio::sync::Semaphore>,
    /// Host pubblicati da un proxy fidato (es. `tailscale serve`) per i dispositivi associati (RF-11).
    pub remote_hosts: std::sync::RwLock<Vec<String>>,
    /// Assegnazioni ai nodi esecutori: le loro richieste non passano dal blocco del Core.
    pub nodes: Arc<crate::nodes::NodeHub>,
    device_db: std::path::PathBuf,
    /// Configurazione guidata dei modelli: stato letto senza il blocco del Core.
    pub setup: Arc<crate::setup::Setup>,
    n8n: Arc<crate::n8n::Manager>,
    /// Sfide WebAuthn in corso (ADR-0042): sfida -> dispositivo, scopo e scadenza.
    challenges: Mutex<std::collections::HashMap<String, Challenge>>,
    /// Autenticazioni remote fallite nella finestra corrente (ADR-0099): inizio della finestra e conteggio.
    remote_failures: Mutex<(std::time::Instant, u32)>,
    /// Flussi di notifiche aperti contemporaneamente (ADR-0098).
    live_streams: Arc<std::sync::atomic::AtomicUsize>,
    /// Lavori cloud in background (ADR-0144): gli ultimi, solo in memoria; la risposta è comunque nella chat.
    jobs: Mutex<Vec<Value>>,
    /// Per annullare un lavoro in background (ADR-0150).
    job_handles: Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>,
}

/// Dopo `REMOTE_FAILURE_LIMIT` autenticazioni remote fallite in `REMOTE_FAILURE_WINDOW` le richieste remote con
/// un token ben formato ma sconosciuto ricevono 429 senza prendere il blocco del Core: una pioggia di token a
/// caso da Internet non può togliere la parola al proprietario (che usa il canale locale).
const REMOTE_FAILURE_LIMIT: u32 = 30;
const REMOTE_FAILURE_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
/// Flussi di notifiche aperti al massimo contemporaneamente.
const MAX_LIVE_STREAMS: usize = 16;

fn note_remote_failure(state: &AppState) {
    let mut failures = state
        .remote_failures
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if failures.0.elapsed() >= REMOTE_FAILURE_WINDOW {
        *failures = (std::time::Instant::now(), 0);
    }
    failures.1 = failures.1.saturating_add(1);
}

fn remote_throttled(state: &AppState) -> bool {
    let failures = state
        .remote_failures
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    failures.0.elapsed() < REMOTE_FAILURE_WINDOW && failures.1 >= REMOTE_FAILURE_LIMIT
}

/// Confronto a tempo costante del token del proprietario.
fn same_token(given: Option<&str>, expected: &str) -> bool {
    given.is_some_and(|given| {
        let (a, b) = (given.as_bytes(), expected.as_bytes());
        // La lunghezza del token non è segreta; il contenuto si confronta senza uscire al primo byte diverso.
        a.len() == b.len()
            && std::hint::black_box(a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y))) == 0
    })
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
    let origins = url::Url::parse(&format!("https://{host}"))
        .ok()
        .map(|url| url.origin().ascii_serialization())
        .into_iter()
        .collect();
    (rp.to_ascii_lowercase(), origins)
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
    let remote = !local
        && host
            .parse::<axum::http::uri::Authority>()
            .is_ok_and(|authority| {
                !host.contains('@')
                    && !host.ends_with(':')
                    && (authority.as_str() == authority.host()
                        || authority.port_u16().is_some_and(|port| port != 0))
                    && state
                        .remote_hosts
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .iter()
                        .any(|h| h.eq_ignore_ascii_case(authority.host()))
            });
    if !local && !remote {
        return error(StatusCode::FORBIDDEN, "forbidden_host", "host non ammesso");
    }
    let path = req.uri().path().to_string();
    if path == "/" && !local {
        return error(StatusCode::NOT_FOUND, "not_found", "risorsa non trovata");
    }
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "non trovato".into());
    let method = req.method().to_string();
    if remote {
        // Dispositivo associato (RF-11): token del dispositivo, solo le rotte dei suoi permessi.
        let public = !path.starts_with("/api/")
            || path == "/api/connectivity"
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
            // I token dei dispositivi sono 64 caratteri esadecimali: il resto si scarta senza interrogare il Core.
            if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                note_remote_failure(&state);
                return error(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "dispositivo non associato o revocato",
                );
            }
            if remote_throttled(&state) {
                let mut res = error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "too_many_failures",
                    "troppi tentativi di accesso falliti: riprova fra poco",
                );
                res.headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("30"));
                return res;
            }
            let device = with_core(&state, move |j| j.device_for_token(&token)).await;
            let Ok(Some(device)) = device else {
                note_remote_failure(&state);
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
        && path != "/api/connectivity"
        && path != "/api/devices/pair"
        && !path.starts_with("/api/hooks/")
        && !path.starts_with("/api/nodes/")
        // Ritorno da OAuth (ADR-0027): navigazione del browser senza token, autenticata dallo `state` monouso.
        && path != "/api/connectors/oauth/callback"
    {
        // I client MCP inviano il segreto come «Authorization: Bearer»: stesso token del proprietario.
        let token = req
            .headers()
            .get("x-jarvis-token")
            .and_then(|t| t.to_str().ok())
            .or_else(|| {
                req.headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|t| t.to_str().ok())
                    .and_then(|t| t.strip_prefix("Bearer "))
            });
        if !same_token(token, &state.token) {
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
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static(
            "camera=(), geolocation=(), payment=(), usb=(), microphone=(self)",
        ),
    );
    h.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    h.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
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
        ("POST", "/api/cloud/chat"),
        ("POST", "/api/prompt/check"),
        ("POST", "/api/chat/stream"),
        ("GET", "/api/tasks"),
        ("GET", "/api/confirmations"),
        ("GET", "/api/chat/sync"),
        ("GET", "/api/notifications"),
        ("GET", "/api/notifications/stream"),
        ("POST", "/api/notifications/read-all"),
        ("POST", "/api/notifications/{id}/read"),
        ("POST", "/api/voice/transcribe"),
        ("POST", "/api/voice/speak"),
        ("GET", "/api/devices/me"),
        ("POST", "/api/push/subscriptions"),
        ("DELETE", "/api/push/subscriptions"),
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
        .route("/api/health", get(health))
        .route(
            "/api/connectivity",
            get(|| async { Json(json!({"service": "jarvis-core", "protocol": 1})) }),
        )
        .route("/api/chat", post(chat))
        .route("/api/security", get(security_status))
        .route("/api/n8n", get(n8n_status).post(n8n_runtime))
        .route("/api/n8n/jobs", post(n8n_create))
        .route("/api/n8n/jobs/{id}", delete(n8n_remove))
        .route("/api/cloud", get(cloud_status).post(configure_cloud))
        .route("/api/cloud/chat", post(cloud_chat))
        .route("/api/mcp", post(mcp_endpoint))
        .route("/api/prompt/check", post(prompt_check))
        .route("/api/cloud/chat/stream", post(cloud_chat_stream))
        .route("/api/cloud/test", post(test_cloud_key))
        .route("/api/cloud/feedback", post(rate_cloud_reply))
        .route("/api/chat/stream", post(chat_stream))
        .route("/api/updates", get(updates_get).post(updates_set))
        .route("/api/updates/check", post(updates_check))
        .route("/api/updates/download", post(updates_download))
        .route("/api/updates/install", post(updates_install))
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
        .route("/api/chat/sync", get(chat_sync))
        .route("/api/conversation", get(conversation))
        .route(
            "/api/conversations",
            get(conversations_list).post(conversation_new),
        )
        .route("/api/cloud/background", post(cloud_background))
        .route("/api/cloud/jobs", get(cloud_jobs))
        .route("/api/cloud/jobs/{id}/cancel", post(cloud_job_cancel))
        .route("/api/conversations/search", get(conversation_search))
        .route("/api/conversations/export", get(conversation_export))
        .route("/api/conversations/select", post(conversation_select))
        .route("/api/conversations/delete", post(conversation_delete))
        .route("/api/memory", get(memory))
        .route("/api/memory/graph", get(memory_graph))
        .route("/api/memory/stats", get(memory_stats))
        .route("/api/system/stats", get(system_stats))
        .route("/api/memory/duplicates", get(memory_duplicates))
        .route("/api/memory/reindex", post(memory_reindex))
        .route("/api/memory/{id}/pin", post(memory_pin))
        .route("/api/memory/{id}", delete(forget).patch(memory_correct))
        .route("/api/audit", get(audit))
        .route("/api/workspace", get(workspace))
        .route("/api/tools", get(tool_catalog))
        .route("/api/tools/registry", get(tool_registry))
        .route("/api/tools/health", get(tool_health))
        .route("/api/tools/history", get(tool_history))
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
        .route(
            "/api/voice/wake",
            post(voice_wake).layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/api/voice/speak", post(voice_speak))
        .route(
            "/api/voice/system",
            get(voice_system_get).patch(voice_system_patch),
        )
        .route("/api/voice/say", post(voice_say))
        .route("/api/voice/stop", post(voice_stop))
        .route("/api/devices", get(devices))
        .route(
            "/api/push/subscriptions",
            post(push_subscription_register).delete(push_subscription_delete),
        )
        .route("/api/devices/pairing", post(create_pairing))
        .route(
            "/api/devices/access",
            get(devices_access_get)
                .put(devices_access_put)
                .post(devices_access_put),
        )
        .route("/api/openapi.json", get(openapi_document))
        .route("/api/emergency", get(emergency_get).post(emergency_post))
        .route(
            "/api/mcp/servers",
            get(mcp_servers_list).post(mcp_server_save),
        )
        .route("/api/mcp/servers/remove", post(mcp_server_remove))
        .route("/api/mcp/servers/test", post(mcp_server_test))
        .route("/api/watches", get(watches_list).post(watch_add))
        .route("/api/watches/remove", post(watch_remove))
        .route("/api/twin", get(twin_get))
        .route("/api/agents", get(agents_registry))
        .route("/api/agents/route", get(agents_route))
        .route("/api/models/route", get(models_route))
        .route("/api/plans", get(plans_list).post(plan_create))
        .route("/api/plans/{id}", get(plan_get))
        .route("/api/plans/{id}/run", post(plan_run))
        .route("/api/plans/{id}/approve", post(plan_approve))
        .route("/api/plans/{id}/cancel", post(plan_cancel))
        .route("/api/plans/{id}/rollback", post(plan_rollback))
        .route("/api/plans/{id}/replay", post(plan_replay))
        .route("/api/tailscale", get(tailscale_status))
        .route("/api/tailscale/connect", post(tailscale_connect))
        .route("/api/devices/pairing/cancel", post(cancel_pairing))
        .route("/api/devices/pair", post(pair_device))
        .route("/api/devices/me", get(device_me))
        .route("/api/setup", get(setup_status))
        .route("/api/setup/start", post(setup_start))
        .route("/api/nodes", get(list_nodes))
        .route("/api/nodes/next", get(node_next))
        .route("/api/nodes/result/{id}", post(node_result))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        .route("/api/connectors", get(connectors).post(install_connector))
        .route("/api/proactive", get(proactive_get).patch(proactive_put))
        .route("/api/proactive/briefing", post(proactive_briefing))
        .route("/api/proactive/weekly", post(proactive_weekly))
        .route("/api/sandbox", get(sandbox_get).patch(sandbox_put))
        .route("/api/docker", get(docker_status))
        .route("/api/docker/cleanup", post(docker_cleanup))
        .route(
            "/api/permissions",
            get(permissions_get).patch(permissions_patch),
        )
        .route("/api/persona", get(persona_get).patch(persona_patch))
        .route("/api/profile", get(profile_get).patch(profile_patch))
        .route("/api/missions", get(missions_list).post(mission_create))
        .route("/api/missions/{id}/{action}", post(mission_action))
        .route("/api/skills", get(skills_list).post(skill_create))
        .route(
            "/api/skills/{id}",
            axum::routing::put(skill_update).delete(skill_delete),
        )
        .route("/api/connectors/templates", get(connector_templates))
        .route("/api/connectors/templates/{id}", post(install_template))
        .route("/api/connectors/new", post(install_new_connector))
        .route("/api/connectors/{id}", delete(uninstall_connector))
        .route("/api/connectors/{id}/authorize", post(authorize_connector))
        .route("/api/connectors/{id}/revoke", post(revoke_connector))
        .route(
            "/api/connectors/{id}/oauth/import",
            post(google_oauth_import).layer(axum::extract::DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/api/connectors/{id}/oauth/start", post(oauth_start))
        .route("/api/connectors/oauth/callback", get(oauth_callback))
        .route("/", get(oauth_loopback_callback))
        .route(
            "/api/hooks/{id}",
            post(connector_hook).layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/notifications", get(notifications))
        .route("/api/notifications/stream", get(notifications_stream))
        .route("/api/push/status", get(push_status))
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

async fn test_cloud_key(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(provider) = crate::cloud::Provider::parse(&body["provider"]) else {
        return CoreError::BadRequest("Seleziona il provider da verificare.".into())
            .into_response();
    };
    let Ok(permit) = state.cloud_slots.clone().try_acquire_owned() else {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "cloud.busy",
            "JARVIS sta elaborando altre richieste cloud.",
        );
    };
    let prepared = with_core(&state, move |j| {
        j.prepare_cloud_chat(&json!({
            "provider": provider.id(), "message": "Rispondi soltanto con OK."
        }))
    })
    .await;
    let request = match prepared {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    // Prima le credenziali (elenco dei modelli, nessuna chat), poi una risposta di prova (ADR-0119).
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let models = match crate::cloud::list_models(provider, &request.target.key) {
            Ok(models) => Some(models),
            // L'elenco può non essere offerto dal servizio: si prosegue con la chat di prova.
            Err(crate::cloud::Failure::Rejected(_) | crate::cloud::Failure::Unavailable) => None,
            Err(failure) => return Err(failure),
        };
        Ok((request.verify(), models))
    })
    .await
    {
        Ok(Err(failure)) => error(
            StatusCode::BAD_GATEWAY,
            failure.code(),
            &failure.user_message(provider).0,
        ),
        Ok(Ok((completed, models))) => match completed.answer {
            Ok(crate::cloud::Answer::Text(_)) => Json(json!({
                "provider": provider.id(), "model": completed.model, "ok": true,
                "credentials": "valid",
                "models_available": models.as_ref().map(Vec::len),
                "model_listed": models.as_ref().map(|m| m.contains(&completed.model)),
            }))
            .into_response(),
            Ok(crate::cloud::Answer::Call(_)) => error(
                StatusCode::BAD_GATEWAY,
                "cloud.invalid_tool",
                "Risposta di prova inattesa dal provider.",
            ),
            Err(failure) => error(
                StatusCode::BAD_GATEWAY,
                failure.code(),
                &failure.user_message(provider).0,
            ),
        },
        Err(_) => CoreError::Internal("Verifica cloud interrotta.".into()).into_response(),
    }
}

async fn rate_cloud_reply(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.rate_cloud_reply(&body)).await)
}

/// Server MCP (ADR-0138): JSON-RPC 2.0 su HTTP; solo il proprietario, con il suo token.
async fn mcp_endpoint(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(message): Json<Value>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    if std::env::var("JARVIS_MCP").is_ok_and(|value| value == "off") {
        return CoreError::Unavailable("Il server MCP è disattivato (JARVIS_MCP=off).".into())
            .into_response();
    }
    match with_core(&state, move |j| crate::mcp::handle_message(j, &message)).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Corregge il campo `message` del corpo (ADR-0135) e restituisce le modifiche, se ce ne sono.
fn apply_correction(body: &mut Value) -> Option<Value> {
    let original = body["message"].as_str()?.to_owned();
    let fixed = crate::promptfix::fix_with(&original, false);
    if !fixed.changed(&original) {
        return None;
    }
    let report = fixed.to_json();
    body["message"] = json!(fixed.text);
    Some(report)
}

fn with_correction(mut out: Value, correction: Option<Value>) -> Value {
    if let (Some(report), Some(map)) = (correction, out.as_object_mut()) {
        map.insert("corrected".into(), report);
    }
    out
}

/// Controlla un testo senza inviarlo: restituisce la versione corretta e le parole cambiate.
async fn prompt_check(Json(body): Json<Value>) -> Response {
    let Some(text) = body["text"].as_str().filter(|t| t.chars().count() <= 8000) else {
        return CoreError::BadRequest("campo 'text' mancante o troppo lungo".into())
            .into_response();
    };
    let fixed = crate::promptfix::fix(text);
    let mut out = fixed.to_json();
    out["changed"] = json!(fixed.changed(text));
    Json(out).into_response()
}

/// Errori di una richiesta cloud: quelli del Core e la saturazione delle richieste in corso.
enum CloudChatError {
    Core(CoreError),
    Busy,
}

impl From<CoreError> for CloudChatError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

fn core_error_parts(error: CoreError) -> (&'static str, String) {
    match error {
        CoreError::BadRequest(m) => ("bad_request", m),
        CoreError::Unavailable(m) => ("unavailable", m),
        CoreError::NotFound(m) => ("not_found", m),
        CoreError::Gone(m) => ("expired", m),
        CoreError::Unauthorized(m) => ("unauthorized", m),
        CoreError::Internal(m) => ("internal", m),
    }
}

impl CloudChatError {
    fn into_response(self) -> Response {
        match self {
            Self::Core(error) => error.into_response(),
            Self::Busy => error(
                StatusCode::TOO_MANY_REQUESTS,
                "cloud.busy",
                "JARVIS sta elaborando altre richieste cloud.",
            ),
        }
    }

    fn parts(self) -> (&'static str, String) {
        match self {
            Self::Core(error) => core_error_parts(error),
            Self::Busy => (
                "cloud.busy",
                "JARVIS sta elaborando altre richieste cloud.".into(),
            ),
        }
    }
}

async fn cloud_chat(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    match run_cloud_chat(&state, device.is_some(), body, None).await {
        Ok(out) => Json(out).into_response(),
        Err(error) => error.into_response(),
    }
}

/// Chat cloud in streaming (ADR-0131): righe NDJSON `{"type":"delta","text":…}` mentre il modello genera, poi
/// `{"type":"done","result":…}` (lo stesso oggetto di `/api/cloud/chat`) oppure `{"type":"error",…}`.
async fn cloud_chat_stream(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let is_device = device.is_some();
    tokio::spawn(async move {
        let result = run_cloud_chat(&state, is_device, body, Some(tx.clone())).await;
        let line = match result {
            Ok(out) => json!({"type": "done", "result": out}),
            Err(error) => {
                let (code, message) = error.parts();
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

/// Lavoro cloud in background (ADR-0144): parte subito, prosegue nel Core anche se l'app passa in secondo piano
/// o si chiude, scrive la risposta nella chat di partenza e avvisa con una notifica (anche push).
async fn cloud_background(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(mut body): Json<Value>,
) -> Response {
    let Some(message) = body["message"]
        .as_str()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
    else {
        return CoreError::BadRequest("campo 'message' mancante".into()).into_response();
    };
    let is_device = device.is_some();
    let active = with_core(&state, |j| {
        Ok::<_, CoreError>(j.conversations()?["active"].clone())
    })
    .await
    .unwrap_or(Value::Null);
    if body["conversation_id"].as_str().is_none() || is_device {
        body["conversation_id"] = if is_device {
            Value::Null
        } else {
            active.clone()
        };
    }
    let job = json!({
        "id": uuid::Uuid::new_v4().simple().to_string(),
        "status": "running",
        "message": message.chars().take(120).collect::<String>(),
        "conversation_id": body["conversation_id"],
        "created_at": crate::automation::ts(chrono::Utc::now()),
    });
    {
        let mut jobs = state.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if jobs.iter().filter(|j| j["status"] == "running").count() >= 4 {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "busy",
                "Ci sono già quattro lavori in background: attendi che uno finisca.",
            );
        }
        jobs.push(job.clone());
        let extra = jobs.len().saturating_sub(30);
        jobs.drain(..extra);
    }
    let id = job["id"].as_str().unwrap_or_default().to_string();
    let worker = state.clone();
    let job_id = id.clone();
    let handle = tokio::spawn(async move {
        let result = run_cloud_chat(&worker, is_device, body, None).await;
        let finished_id = id.clone();
        worker
            .job_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&finished_id);
        let (status, summary, detail, outcome) = match result {
            Ok(out) => {
                let reply = out["reply"].as_str().unwrap_or("Fatto.").to_string();
                (
                    "done",
                    "JARVIS ha finito il lavoro in background".to_string(),
                    reply.clone(),
                    Ok(reply),
                )
            }
            Err(error) => {
                let (_, text) = error.parts();
                (
                    "error",
                    "Il lavoro in background non è riuscito".to_string(),
                    text.clone(),
                    Err(text),
                )
            }
        };
        {
            let mut jobs = worker.jobs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = jobs.iter_mut().find(|j| j["id"] == id.as_str()) {
                entry["status"] = json!(status);
                entry["finished_at"] = json!(crate::automation::ts(chrono::Utc::now()));
                match outcome {
                    Ok(reply) => entry["reply"] = json!(reply),
                    Err(text) => entry["error"] = json!(text),
                }
            }
        }
        let note = json!({
            "severity": if status == "done" { "success" } else { "warning" },
            "category": "mission",
            "title": summary,
            "body": detail.chars().take(500).collect::<String>(),
            "source": {"kind": "module", "id": "cloud-background"},
            "dedupe_key": format!("job:{id}"),
            "link": {"view": "chat"},
        });
        let _ = with_core(&worker, move |j| j.notify(note)).await;
    });
    state
        .job_handles
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(job_id, handle.abort_handle());
    (StatusCode::ACCEPTED, Json(said(job, "background", None))).into_response()
}

/// Annulla un lavoro in background ancora in corso: il risultato non viene scritto nella chat.
async fn cloud_job_cancel(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    let cancelled = {
        let mut jobs = state.jobs.lock().unwrap_or_else(|e| e.into_inner());
        match jobs.iter_mut().find(|j| j["id"] == id.as_str()) {
            None => return CoreError::NotFound("lavoro non trovato".into()).into_response(),
            Some(job) if job["status"] != "running" => {
                return error(
                    StatusCode::CONFLICT,
                    "not_running",
                    "Il lavoro è già terminato.",
                )
            }
            Some(job) => {
                job["status"] = json!("cancelled");
                job["finished_at"] = json!(crate::automation::ts(chrono::Utc::now()));
                job.clone()
            }
        }
    };
    if let Some(handle) = state
        .job_handles
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
    {
        handle.abort();
    }
    let _ = with_core(&state, |j| j.record_owner_action("cloud.job.cancel")).await;
    Json(said(cancelled, "job.cancel", None)).into_response()
}

async fn cloud_jobs(State(state): State<Shared>) -> Response {
    let jobs = state.jobs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Json(json!({"items": jobs.into_iter().rev().collect::<Vec<_>>()})).into_response()
}

async fn run_cloud_chat(
    state: &Shared,
    is_device: bool,
    mut body: Value,
    deltas: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> Result<Value, CloudChatError> {
    let correction = apply_correction(&mut body);
    let out = run_cloud_chat_inner(state, is_device, body, deltas).await?;
    Ok(with_correction(out, correction))
}

async fn run_cloud_chat_inner(
    state: &Shared,
    is_device: bool,
    mut body: Value,
    deltas: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> Result<Value, CloudChatError> {
    let Some(message) = body["message"].as_str() else {
        return Err(CoreError::BadRequest("campo 'message' mancante".into()).into());
    };
    if is_device
        && (crate::desktop::Target::from_request(message).is_some()
            || crate::appcontrol::is_app_request(message))
    {
        return Err(CoreError::Unauthorized("Azione riservata al proprietario.".into()).into());
    }
    if crate::cloud::contains_credentials(message) {
        return Err(CoreError::BadRequest("Rimuovi le credenziali dalla richiesta.".into()).into());
    }
    let local = message.to_owned();
    // Chat di destinazione: solo il proprietario può indicarla (lavori in background, ADR-0144).
    let conversation = if is_device {
        None
    } else {
        body["conversation_id"].as_str().map(str::to_owned)
    };
    let local_only = crate::orchestrator::is_source_followup(message)
        || crate::orchestrator::is_local_fact_request(message)
        || (crate::desktop::Target::from_request(message).is_some()
            || crate::appcontrol::is_app_request(message));
    if is_device {
        body["provider"] = json!("auto");
        body["use_context"] = json!(false);
    }
    let target = conversation.clone();
    match with_core(state, move |j| {
        j.in_conversation(target.as_deref(), |j| j.local_command(&local))
    })
    .await
    {
        Ok(Some(out)) => return Ok(out.to_json()),
        Ok(None) => (),
        Err(CoreError::Unavailable(_)) if !local_only => {
            state.obs.count("cloud.local_parser_fallback");
            state.obs.log(
                "warn",
                "cloud.local_parser_fallback",
                json!({"fallback": "provider"}),
            );
        }
        Err(error) => return Err(error.into()),
    }
    let Ok(permit) = state.cloud_slots.clone().try_acquire_owned() else {
        return Err(CloudChatError::Busy);
    };
    let target = conversation.clone();
    let request = with_core(state, move |j| {
        j.in_conversation(target.as_deref(), |j| j.prepare_cloud_chat(&body))
    })
    .await?;
    let timer = crate::observability::Timer::start();
    let completed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let completed = match deltas {
            Some(deltas) => request.execute_streaming(&mut |piece: &str| {
                let _ = deltas.send(json!({"type": "delta", "text": piece}).to_string() + "\n");
            }),
            None => request.execute(),
        };
        (request.message, completed)
    })
    .await;
    let Ok((message, completed)) = completed else {
        return Err(CoreError::Internal("richiesta cloud interrotta".into()).into());
    };
    let out = with_core(state, move |j| {
        j.in_conversation(conversation.as_deref(), |j| {
            j.finish_cloud_chat(message, completed, timer)
        })
    })
    .await?;
    Ok(out.to_json())
}

async fn chat(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(message) = body["message"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'message' mancante",
        );
    };
    if device.is_some()
        && (crate::desktop::Target::from_request(&message).is_some()
            || crate::appcontrol::is_app_request(&message))
    {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "Azione disponibile dal computer del proprietario",
        );
    }
    let (message, correction) = {
        let mut body = json!({"message": message});
        let correction = apply_correction(&mut body);
        (
            body["message"].as_str().unwrap_or_default().to_string(),
            correction,
        )
    };
    match with_core(&state, move |j| j.chat(&message)).await {
        Ok(out) => Json(with_correction(out.to_json(), correction)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Chat in streaming (ADR-0025): risposta NDJSON con righe `{"type":"delta","text":…}` mentre il modello
/// genera, poi `{"type":"done","result":…}` (lo stesso oggetto di `/api/chat`) oppure `{"type":"error",…}`.
async fn chat_stream(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(message) = body["message"].as_str().map(str::to_string) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'message' mancante",
        );
    };
    if device.is_some()
        && (crate::desktop::Target::from_request(&message).is_some()
            || crate::appcontrol::is_app_request(&message))
    {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "Azione disponibile dal computer del proprietario",
        );
    }
    let (message, correction) = {
        let mut body = json!({"message": message});
        let correction = apply_correction(&mut body);
        (
            body["message"].as_str().unwrap_or_default().to_string(),
            correction,
        )
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
            Ok(out) => {
                json!({"type": "done", "result": with_correction(out.to_json(), correction)})
            }
            Err(e) => {
                let (code, message) = core_error_parts(e);
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

async fn google_oauth_import(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(file): Json<Value>,
) -> Response {
    reply(with_core(&state, move |j| j.google_oauth_start(&id, &file)).await)
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

async fn oauth_loopback_callback(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if !q.contains_key("state") {
        return error(StatusCode::NOT_FOUND, "not_found", "risorsa non trovata");
    }
    oauth_callback(State(state), Query(q)).await
}

async fn updates_get(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.updates_state()).await)
}

async fn updates_set(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(enabled) = body["enabled"].as_bool() else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "campo 'enabled' booleano mancante",
        );
    };
    let channel = match body.get("channel") {
        Some(Value::String(channel)) => channel.clone(),
        Some(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "campo 'channel' non valido",
            )
        }
        None => "stable".into(),
    };
    let github_token = match body.get("github_token") {
        Some(Value::String(token)) => Some(token.clone()),
        Some(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "campo 'github_token' non valido",
            )
        }
        None => None,
    };
    let remove_github_token = match body.get("remove_github_token") {
        Some(Value::Bool(remove)) => *remove,
        Some(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "campo 'remove_github_token' non valido",
            )
        }
        None => false,
    };
    reply(
        with_core(&state, move |j| {
            j.updates_configure(
                enabled,
                &channel,
                github_token.as_deref(),
                remove_github_token,
            )
        })
        .await,
    )
}

/// Controllo manuale: la richiesta a GitHub avviene fuori dal blocco del Core.
async fn updates_check(State(state): State<Shared>) -> Response {
    let fetched = fetch_releases(&state).await;
    reply(with_core(&state, move |j| j.updates_record(fetched)).await)
}

/// Scarica in background l'installatore della versione trovata: solo il proprietario dal computer del Core.
async fn updates_download(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    let planned = with_core(&state, |j| j.updates_download_plan()).await;
    match planned {
        Ok((plan, token, dir)) => {
            match crate::selfupdate::start_download(&dir, plan, token) {
                Ok(()) => {}
                Err(message) => return error(StatusCode::CONFLICT, "conflict", &message),
            }
            reply(with_core(&state, |j| j.updates_state()).await)
        }
        Err(e) => reply(Err(e)),
    }
}

/// Applica l'aggiornamento verificato e riavvia l'app: solo il proprietario dal computer del Core.
async fn updates_install(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply(with_core(&state, |j| j.updates_install()).await)
}

async fn fetch_releases(state: &Shared) -> Result<Vec<Value>, String> {
    let (dir, token) = with_core(state, |j| j.updates_fetch_config())
        .await
        .map_err(|error| format!("configurazione aggiornamenti non disponibile: {error:?}"))?;
    tokio::task::spawn_blocking(move || {
        crate::updates::Updates::new(&dir).fetch_with_token(token.as_deref())
    })
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

async fn confirmations(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Json<Value> {
    let items = with_core(&state, move |j| {
        j.pending_confirmations_for(device.is_some())
    })
    .await;
    Json(json!({"items": items}))
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
    if device.is_some() {
        let id = id.clone();
        if with_core(&state, move |j| j.requires_owner_confirmation(&id)).await {
            return error(
                StatusCode::FORBIDDEN,
                "owner_only",
                "Conferma dal computer del proprietario",
            );
        }
    }
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
async fn proactive_get(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.proactive_settings()).await))
}

async fn proactive_put(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply_said(
        with_core(&state, move |j| j.proactive_update(&body)).await,
        "proactive",
    )
}

async fn persona_get(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.persona()).await))
}

/// Personalità e forma di rivolgersi: solo il proprietario dal computer del Core (ADR-0120).
async fn persona_patch(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply_said(
        with_core(&state, move |j| j.persona_update(&body)).await,
        "persona",
    )
}

/// Profilo del proprietario (ADR-0133): lettura e modifica solo dal computer del Core.
async fn profile_get(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    reply(Ok(with_core(&state, |j| j.profile()).await))
}

async fn profile_patch(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    reply_said(
        with_core(&state, move |j| j.profile_update(&body)).await,
        "profile",
    )
}

/// Permessi del proprietario: lettura e modifica solo dal computer del Core (ADR-0116).
async fn permissions_get(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply(Ok(with_core(&state, |j| j.permissions()).await))
}

async fn permissions_patch(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply(with_core(&state, move |j| j.permissions_update(&body)).await)
}

async fn sandbox_get(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.sandbox_settings()).await))
}

/// Stato del collegamento a Docker e del sandbox: motore, versione, immagine, profili e sessioni (ADR-0147).
async fn docker_status(State(state): State<Shared>) -> Response {
    let dir = with_core(&state, |j| j.data_dir()).await;
    let status = tokio::task::spawn_blocking(move || crate::sandbox::terminal_status(&dir))
        .await
        .unwrap_or_else(|_| json!({"available": false}));
    Json(status).into_response()
}

/// Rimuove i container del sandbox rimasti dopo un arresto brusco.
async fn docker_cleanup(State(state): State<Shared>) -> Response {
    let removed = tokio::task::spawn_blocking(crate::sandbox::cleanup_orphans)
        .await
        .unwrap_or(0);
    let _ = with_core(&state, |j| j.record_owner_action("docker.cleanup")).await;
    let removed_text = removed.to_string();
    Json(said(
        json!({"removed": removed}),
        "docker.cleanup",
        Some(&removed_text),
    ))
    .into_response()
}

/// Interruttore del terminale sandbox: solo il proprietario dal computer del Core.
async fn sandbox_put(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return error(
            StatusCode::FORBIDDEN,
            "owner_only",
            "operazione disponibile solo dal computer del proprietario",
        );
    }
    reply_said(
        with_core(&state, move |j| j.sandbox_update(&body)).await,
        "sandbox",
    )
}

async fn proactive_briefing(State(state): State<Shared>) -> Response {
    reply_said(with_core(&state, |j| j.briefing_now()).await, "briefing")
}

async fn proactive_weekly(State(state): State<Shared>) -> Response {
    reply_said(
        with_core(&state, |j| j.weekly_briefing_now()).await,
        "briefing",
    )
}

async fn missions_list(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.missions()).await)
}

async fn mission_create(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.mission_create(&body)).await)
}

/// `pause`, `resume`, `cancel`, `delete`, `run` (esegue subito, in background).
async fn mission_action(
    State(state): State<Shared>,
    Path((id, action)): Path<(String, String)>,
) -> Response {
    let run_now = action == "run";
    let target = id.clone();
    let result = with_core(&state, move |j| j.mission_action(&id, &action)).await;
    if run_now && result.is_ok() {
        start_mission(&state, &target).await;
    }
    reply(result)
}

/// Prenota la missione e ne avvia l'esecuzione in background (ADR-0096).
async fn start_mission(state: &Shared, id: &str) {
    let id = id.to_string();
    match with_core(state, move |j| j.mission_begin(&id)).await {
        Ok(Some(run)) => {
            tokio::spawn(drive_mission(state.clone(), run));
        }
        Ok(None) => {}
        Err(e) => eprintln!("[missioni] avvio non riuscito: {e:?}"),
    }
}

/// Esegue una missione. Con un modello cloud ogni richiesta al provider avviene fuori dal blocco del Core,
/// così chat e automazioni non attendono; ogni strumento rientra nel Core per ambito, policy e audit.
async fn drive_mission(state: Shared, run: crate::orchestrator::MissionRun) {
    use crate::orchestrator::Next;
    let mut run = run;
    if !run.uses_cloud() {
        let outcome = with_core(&state, move |j| {
            let mut run = run;
            j.mission_run_local(&mut run)
        })
        .await;
        if let Err(e) = outcome {
            eprintln!("[missioni] esecuzione locale non riuscita: {e:?}");
        }
        return;
    }
    let mut answer = None;
    loop {
        let (back, next) = with_core(&state, move |j| {
            let next = j.mission_advance(&mut run, answer);
            (run, next)
        })
        .await;
        run = back;
        match next {
            Ok(Next::Ask(request)) => {
                answer = Some(
                    tokio::task::spawn_blocking(move || request.execute())
                        .await
                        .unwrap_or(Err(crate::cloud::Failure::Unavailable)),
                );
            }
            Ok(Next::Done(_)) => break,
            Err(e) => {
                eprintln!("[missioni] passo non riuscito: {e:?}");
                let _ = with_core(&state, move |j| {
                    j.mission_end(&mut run, Err("errore interno del Core".into()))
                })
                .await;
                break;
            }
        }
    }
}

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
    reply(with_core(&state, move |j| j.install_new_connector(&manifest)).await)
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

async fn chat_sync(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    reply(
        with_core(&state, move |j| {
            let confirmations = j.pending_confirmations_for(device.is_some());
            Ok(json!({"messages": j.conversation(50)?, "confirmations": confirmations}))
        })
        .await,
    )
}

async fn conversation(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let n = limit(&json!(q), 100);
    match with_core(&state, move |j| j.owner_conversation(n)).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn conversations_list(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.conversations()).await)
}

async fn conversation_new(State(state): State<Shared>) -> Response {
    reply_said(
        with_core(&state, |j| j.conversation_new()).await,
        "chat.new",
    )
}

async fn conversation_search(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let query = q.get("q").cloned().unwrap_or_default();
    reply(with_core(&state, move |j| j.conversation_search(&query)).await)
}

async fn conversation_export(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let id = q.get("id").cloned().unwrap_or_else(|| "main".into());
    reply(with_core(&state, move |j| j.conversation_export(&id)).await)
}

async fn conversation_select(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let id = body["id"].as_str().unwrap_or_default().to_string();
    reply_said(
        with_core(&state, move |j| j.conversation_select(&id)).await,
        "chat.select",
    )
}

async fn conversation_delete(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let id = body["id"].as_str().unwrap_or_default().to_string();
    reply_said(
        with_core(&state, move |j| j.conversation_delete(&id)).await,
        "chat.delete",
    )
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

/// Carico del computer per la Plancia (ADR-0126); letto fuori dal blocco del Core.
async fn system_stats() -> Response {
    match tokio::task::spawn_blocking(crate::sysstats::stats).await {
        Ok(stats) => Json(stats).into_response(),
        Err(_) => CoreError::Internal("Lettura del carico interrotta.".into()).into_response(),
    }
}

async fn memory_stats(State(state): State<Shared>) -> Response {
    match with_core(&state, |j| j.memory_stats()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Ricordi quasi uguali; `?threshold=0.9` (da 0,6 a 0,99). Non elimina nulla.
async fn memory_duplicates(
    State(state): State<Shared>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let threshold = query
        .get("threshold")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.9);
    match with_core(&state, move |j| j.memory_duplicates(threshold)).await {
        Ok(v) => Json(v).into_response(),
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

async fn memory_pin(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let pinned = body["pinned"].as_bool().unwrap_or(true);
    reply_said(
        with_core(&state, move |j| j.memory_pin(&id, pinned)).await,
        if pinned { "memory.pin" } else { "memory.unpin" },
    )
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

async fn tool_registry(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    reply(with_core(&state, move |j| j.tool_registry(&q)).await)
}

async fn tool_health(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.tool_health()).await)
}

async fn tool_history(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let tool = q.get("tool").cloned().filter(|t| !t.is_empty());
    let n = limit(&json!(q), 50);
    reply(with_core(&state, move |j| j.tool_history(tool.as_deref(), n)).await)
}

async fn tool_catalog(State(state): State<Shared>) -> Json<Value> {
    Json(json!({"items": with_core(&state, |j| j.tool_catalog()).await}))
}

/// Aggiunge a una risposta riuscita la frase di conferma nello stile della personalità (ADR-0148): dice cosa è
/// successo davvero, non un generico «operazione completata».
fn said(mut value: Value, key: &str, detail: Option<&str>) -> Value {
    static SEED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seed = SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if value.is_object() && value.get("message").is_none() {
        value["message"] = json!(crate::persona::confirmation(
            &crate::persona::active(),
            key,
            detail,
            seed
        ));
    }
    value
}

fn reply_said(result: Result<Value, CoreError>, key: &str) -> Response {
    reply(result.map(|value| said(value, key, None)))
}

fn reply(result: Result<Value, CoreError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Ascolto «hey Jarvis» (ADR-0134): solo il proprietario, e la risposta è un sì o un no.
async fn voice_wake(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    body: axum::body::Bytes,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    let audio = body.to_vec();
    reply(with_core(&state, move |j| j.voice_wake(&audio)).await)
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

fn owner_only_response() -> Response {
    error(
        StatusCode::FORBIDDEN,
        "owner_only",
        "operazione disponibile solo dal computer del proprietario",
    )
}

/// Voce di sistema maschile (ADR-0123): stato e impostazione «leggi le risposte».
async fn voice_system_get(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    let dir = with_core(&state, |j| j.data_dir()).await;
    Json(crate::speech::status(&dir)).into_response()
}

async fn voice_system_patch(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    let dir = with_core(&state, |j| j.data_dir()).await;
    match crate::speech::update(&dir, &body) {
        Ok(_) => Json(said(crate::speech::status(&dir), "voice", None)).into_response(),
        Err(message) => CoreError::BadRequest(message).into_response(),
    }
}

/// Legge un testo con la voce di sistema maschile: solo dal computer del proprietario.
async fn voice_say(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    let _ = &state;
    let Some(text) = body["text"].as_str().map(str::to_owned) else {
        return CoreError::BadRequest("campo 'text' mancante".into()).into_response();
    };
    match tokio::task::spawn_blocking(move || crate::speech::speak(&text)).await {
        Ok(Ok(doc)) => Json(doc).into_response(),
        Ok(Err(e)) => error(StatusCode::UNPROCESSABLE_ENTITY, e.code, &e.message),
        Err(_) => CoreError::Internal("Lettura interrotta.".into()).into_response(),
    }
}

async fn voice_stop(device: Option<axum::Extension<DeviceCtx>>) -> Response {
    if device.is_some() {
        return owner_only_response();
    }
    crate::speech::stop();
    Json(json!({"stopped": true})).into_response()
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

async fn push_status(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.push_status()).await))
}

/// Posto occupato da un flusso aperto: si libera quando il flusso termina o il client si disconnette.
struct StreamSlot(Arc<std::sync::atomic::AtomicUsize>);

impl StreamSlot {
    fn take(counter: &Arc<std::sync::atomic::AtomicUsize>) -> Option<Self> {
        use std::sync::atomic::Ordering;
        if counter.fetch_add(1, Ordering::AcqRel) >= MAX_LIVE_STREAMS {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self(counter.clone()))
    }
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Flusso in tempo reale delle notifiche (Server-Sent Events, ADR-0098): ogni notifica creata dal Core arriva
/// subito ai client collegati, senza attendere il giro successivo di polling. Un commento ogni 20 secondi
/// tiene viva la connessione; se il client resta indietro riceve `resync` e rilegge l'elenco.
async fn notifications_stream(State(state): State<Shared>) -> Response {
    let Some(slot) = StreamSlot::take(&state.live_streams) else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_streams",
            "troppi flussi in tempo reale aperti",
        );
    };
    let rx = with_core(&state, |j| j.subscribe_notifications()).await;
    let first = "retry: 3000\nevent: ready\ndata: {}\n\n".to_string();
    let stream = futures_util::stream::unfold(
        (rx, Some(first), slot),
        |(mut rx, pending, slot)| async move {
            if let Some(chunk) = pending {
                return Some((Ok::<_, std::convert::Infallible>(chunk), (rx, None, slot)));
            }
            let next = tokio::time::timeout(std::time::Duration::from_secs(20), rx.recv()).await;
            let chunk = match next {
                Err(_) => ": ping\n\n".to_string(),
                Ok(Ok(doc)) => format!(
                    "id: {}\nevent: notification\ndata: {}\n\n",
                    doc["notification_id"].as_str().unwrap_or_default(),
                    doc
                ),
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                    "event: resync\ndata: {}\n\n".to_string()
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => return None,
            };
            Some((Ok(chunk), (rx, None, slot)))
        },
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Accel-Buffering", "no")
        .body(axum::body::Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn push_subscription_register(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = body["token"].as_str().map(str::to_string) else {
        return CoreError::BadRequest("campo token mancante".into()).into_response();
    };
    if !crate::push::validate_registration_token(&token) {
        return CoreError::BadRequest("token FCM non valido".into()).into_response();
    }
    let Some(platform) = body["platform"]
        .as_str()
        .and_then(crate::push::Platform::parse)
    else {
        return CoreError::BadRequest("platform deve essere android o web".into()).into_response();
    };
    let principal = device
        .map(|axum::Extension(DeviceCtx(d))| {
            d["device_id"].as_str().unwrap_or_default().to_string()
        })
        .unwrap_or_else(|| "owner".into());
    reply(
        with_core(&state, move |j| {
            j.push_register(&principal, &token, platform)
        })
        .await,
    )
}

async fn push_subscription_delete(
    State(state): State<Shared>,
    device: Option<axum::Extension<DeviceCtx>>,
) -> Response {
    let principal = device
        .map(|axum::Extension(DeviceCtx(d))| {
            d["device_id"].as_str().unwrap_or_default().to_string()
        })
        .unwrap_or_else(|| "owner".into());
    reply(with_core(&state, move |j| j.push_remove(&principal)).await)
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

/// Un host pubblicabile: nome completo (con almeno un punto), senza caratteri jolly né porte.
pub fn valid_remote_host(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn pairing_urls(state: &Shared) -> Vec<String> {
    state
        .remote_hosts
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|host| format!("https://{host}"))
        .collect()
}

/// Codifica per un parametro di URL: lascia solo i caratteri sicuri.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Collegamento di associazione (ADR-0137): un solo tocco compila indirizzo e codice sul dispositivo.
pub fn pairing_link(origin: &str, code: &str) -> String {
    format!(
        "jarvis://pair?origin={}&code={}",
        percent_encode(origin),
        percent_encode(code)
    )
}

/// Indirizzi pubblici del Core (ADR-0137): il proprietario li gestisce dall'app, senza file né variabili.
async fn devices_access_get(State(state): State<Shared>) -> Response {
    let hosts = state
        .remote_hosts
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(json!({"hosts": hosts, "pairing_urls": pairing_urls(&state)})).into_response()
}

async fn devices_access_put(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let dir = with_core(&state, |j| j.data_dir()).await;
    let mut hosts = state
        .remote_hosts
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(host) = body["add"].as_str() {
        // Accetta anche un indirizzo copiato per intero: https://nome.esempio.net/ → nome.esempio.net
        let host = host
            .trim()
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if !valid_remote_host(&host) {
            return CoreError::BadRequest(
                "Indirizzo non valido: serve un nome completo come casa.tailnet.ts.net, senza porta né caratteri jolly".into(),
            )
            .into_response();
        }
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    } else if let Some(host) = body["remove"].as_str() {
        hosts.retain(|h| !h.eq_ignore_ascii_case(host.trim()));
    } else {
        return CoreError::BadRequest("indica «add» o «remove»".into()).into_response();
    }
    hosts.sort_unstable();
    hosts.dedup();
    if std::fs::write(dir.join("remote-hosts.txt"), hosts.join("\n")).is_err() {
        return CoreError::Internal("impossibile salvare gli indirizzi".into()).into_response();
    }
    let current = hosts.clone();
    drop(hosts);
    let key = if body["add"].is_string() {
        "host.add"
    } else {
        "host.remove"
    };
    Json(said(
        json!({"hosts": current, "pairing_urls": pairing_urls(&state)}),
        key,
        None,
    ))
    .into_response()
}

/// Salva l'elenco degli indirizzi pubblici (ADR-0137) e lo rende subito attivo.
fn add_remote_host(state: &Shared, dir: &std::path::Path, host: &str) -> Result<(), String> {
    let mut hosts = state
        .remote_hosts
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if !hosts.iter().any(|h| h == host) {
        hosts.push(host.to_string());
    }
    hosts.sort_unstable();
    hosts.dedup();
    std::fs::write(dir.join("remote-hosts.txt"), hosts.join("\n"))
        .map_err(|_| "impossibile salvare gli indirizzi".to_string())
}

fn tailscale_view(state: &Shared, mut status: Value) -> Value {
    let allowed = status["host"].as_str().is_some_and(|host| {
        state
            .remote_hosts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|h| h == host)
    });
    status["allowed"] = json!(allowed);
    status["pairing_urls"] = json!(pairing_urls(state));
    status
}

/// Server MCP autorizzati (ADR-0156): solo dal computer del proprietario; il token non viene mai restituito.
async fn mcp_servers_list(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.mcp_servers()).await))
}

async fn mcp_server_save(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply_said(
        with_core(&state, move |j| j.mcp_server_save(&body)).await,
        "mcp.server",
    )
}

async fn mcp_server_remove(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let name = body["name"].as_str().unwrap_or_default().to_string();
    reply_said(
        with_core(&state, move |j| j.mcp_server_remove(&name)).await,
        "mcp.server.remove",
    )
}

/// Prova un server: elenca i suoi strumenti (fuori dal blocco del Core).
async fn mcp_server_test(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let name = body["name"].as_str().unwrap_or_default().to_string();
    let target = name.clone();
    let info = with_core(&state, move |j| {
        let url = crate::mcp_client::url_of(&j.data_dir(), &target);
        let token = j.mcp_token(&target);
        Ok::<_, CoreError>((url, token))
    })
    .await;
    let Ok((Some(url), token)) = info else {
        return CoreError::NotFound("server MCP non trovato".into()).into_response();
    };
    match tokio::task::spawn_blocking(move || crate::mcp_client::list_tools(&url, token.as_deref()))
        .await
    {
        Ok(Ok(tools)) => Json(tools).into_response(),
        Ok(Err(message)) => CoreError::Unavailable(message).into_response(),
        Err(_) => CoreError::Internal("prova interrotta".into()).into_response(),
    }
}

/// Regole di osservazione dei file (ADR-0158), solo proprietario.
async fn watches_list(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.watches()).await))
}

async fn watch_add(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply_said(
        with_core(&state, move |j| j.watch_add(&body)).await,
        "watch.add",
    )
}

async fn watch_remove(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let id = body["id"].as_str().unwrap_or_default().to_string();
    reply_said(
        with_core(&state, move |j| j.watch_remove(&id)).await,
        "watch.remove",
    )
}

/// Digital Environment Twin (ADR-0157): inventario dell'ambiente a cui JARVIS ha accesso.
async fn twin_get(State(state): State<Shared>) -> Response {
    let twin = with_core(&state, |j| j.digital_twin()).await;
    reply(twin)
}

/// Registro degli agenti (ADR-0153).
async fn agents_registry(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.agent_registry()).await))
}

/// Spiega a quale agente andrebbe una richiesta e perché (parametro `q`).
async fn agents_route(Query(q): Query<std::collections::HashMap<String, String>>) -> Response {
    let message = q.get("q").cloned().unwrap_or_default();
    Json(json!({
        "agent": crate::agents::explain(&message),
        "role": crate::prompts::role_label(crate::prompts::role_for(&message)),
    }))
    .into_response()
}

/// Spiega come una richiesta verrebbe instradata ai modelli cloud (parametro `q`): rotta per compito, modello per
/// provider e regole di riservatezza. Non invia nulla ai provider.
async fn models_route(
    State(state): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let message = q.get("q").cloned().unwrap_or_default();
    reply(Ok(with_core(&state, move |j| j.model_route(&message)).await))
}

/// Piani di esecuzione (ADR-0152): creazione, esecuzione per passi, conferme, annullamento, rollback e replay.
async fn plans_list(State(state): State<Shared>) -> Response {
    reply(with_core(&state, |j| j.plan_list()).await)
}

async fn plan_get(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply(with_core(&state, move |j| j.plan_get(&id)).await)
}

/// Avvia (se non c'è già) il ciclo che esegue un passo alla volta fuori dal blocco del Core.
fn spawn_plan_runner(state: &Shared, id: String) {
    let key = format!("plan:{id}");
    let mut handles = state.job_handles.lock().unwrap_or_else(|e| e.into_inner());
    if handles.contains_key(&key) {
        return;
    }
    let worker = state.clone();
    let (runner_id, runner_key) = (id.clone(), key.clone());
    let handle = tokio::spawn(async move {
        let final_doc = loop {
            let step_id = runner_id.clone();
            match with_core(&worker, move |j| j.plan_step(&step_id)).await {
                Ok(doc) if doc["status"] == "running" => tokio::task::yield_now().await,
                Ok(doc) => break Some(doc),
                Err(_) => break None,
            }
        };
        worker
            .job_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&runner_key);
        if let Some(doc) = final_doc {
            let status = doc["status"].as_str().unwrap_or("").to_string();
            if matches!(
                status.as_str(),
                "succeeded" | "failed" | "budget_exceeded" | "waiting"
            ) {
                let waiting = status == "waiting";
                let note = json!({
                    "severity": if status == "succeeded" { "success" } else { "warning" },
                    "category": "automation",
                    "title": if waiting { "Un piano attende la tua conferma".to_string() }
                             else { format!("Piano concluso: {status}") },
                    "body": doc["goal"].as_str().unwrap_or_default().chars().take(300).collect::<String>(),
                    "source": {"kind": "module", "id": "plans"},
                    "dedupe_key": format!("plan:{runner_id}:{status}"), "link": {"view": "plans"},
                });
                let _ = with_core(&worker, move |j| j.notify(note)).await;
            }
        }
    });
    handles.insert(key, handle.abort_handle());
}

async fn plan_create(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let run = body["run"].as_bool().unwrap_or(false);
    let created = with_core(&state, move |j| {
        let doc = j.plan_create(&body)?;
        if run {
            let id = doc["id"].as_str().unwrap_or_default().to_string();
            return j.plan_start(&id);
        }
        Ok(doc)
    })
    .await;
    if let Ok(doc) = &created {
        if doc["status"] == "running" {
            spawn_plan_runner(&state, doc["id"].as_str().unwrap_or_default().to_string());
        }
    }
    reply(created.map(|doc| said(doc, if run { "plan.started" } else { "plan.created" }, None)))
}

async fn plan_run(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    let target = id.clone();
    let started = with_core(&state, move |j| j.plan_start(&target)).await;
    if started.is_ok() {
        spawn_plan_runner(&state, id);
    }
    reply_said(started, "plan.started")
}

async fn plan_approve(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let steps: Vec<String> = body["steps"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let target = id.clone();
    let approved = with_core(&state, move |j| j.plan_approve(&target, &steps)).await;
    if approved.is_ok() {
        spawn_plan_runner(&state, id);
    }
    reply_said(approved, "plan.approved")
}

async fn plan_cancel(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    if let Some(handle) = state
        .job_handles
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&format!("plan:{id}"))
    {
        handle.abort();
    }
    reply_said(
        with_core(&state, move |j| j.plan_cancel(&id)).await,
        "plan.cancelled",
    )
}

async fn plan_rollback(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let confirm = body["confirm"].as_bool().unwrap_or(false);
    reply_said(
        with_core(&state, move |j| j.plan_rollback(&id, confirm)).await,
        "plan.rollback",
    )
}

async fn plan_replay(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    reply_said(
        with_core(&state, move |j| j.plan_replay(&id)).await,
        "plan.replayed",
    )
}

/// Arresto di emergenza (ADR-0151): stato e attivazione, solo dal computer del proprietario.
async fn emergency_get(State(state): State<Shared>) -> Response {
    reply(Ok(with_core(&state, |j| j.emergency_status()).await))
}

async fn emergency_post(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let active = body["active"].as_bool().unwrap_or(true);
    if active {
        // Ferma subito i lavori in background in corso.
        let handles: Vec<_> = state
            .job_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .collect();
        for (id, handle) in handles {
            handle.abort();
            let mut jobs = state.jobs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(job) = jobs.iter_mut().find(|j| j["id"] == id.as_str()) {
                job["status"] = json!("cancelled");
            }
        }
    }
    let result = with_core(&state, move |j| j.emergency_set(active)).await;
    reply(result.map(|doc| {
        said(
            doc,
            if active {
                "emergency.stop"
            } else {
                "emergency.resume"
            },
            None,
        )
    }))
}

/// Descrizione OpenAPI dell'API locale (ADR-0146).
async fn openapi_document() -> Response {
    Json(crate::openapi::document(env!("CARGO_PKG_VERSION"))).into_response()
}

/// Stato del collegamento con Tailscale (ADR-0141); solo per il proprietario.
async fn tailscale_status(State(state): State<Shared>) -> Response {
    let port = state.port;
    let status = tokio::task::spawn_blocking(move || crate::tailscale::status(port))
        .await
        .unwrap_or_else(|_| json!({"installed": false, "message": "controllo non riuscito"}));
    Json(tailscale_view(&state, status)).into_response()
}

/// Pubblica il Core nella rete privata Tailscale e ne autorizza l'indirizzo: un solo gesto, nessun file.
async fn tailscale_connect(State(state): State<Shared>) -> Response {
    let port = state.port;
    let outcome = tokio::task::spawn_blocking(move || {
        crate::tailscale::publish(port).map(|note| (note, crate::tailscale::status(port)))
    })
    .await
    .unwrap_or_else(|_| Err("operazione interrotta".into()));
    let (note, status) = match outcome {
        Ok(done) => done,
        Err(message) => {
            return CoreError::BadRequest(message).into_response();
        }
    };
    let Some(host) = status["host"]
        .as_str()
        .filter(|host| valid_remote_host(host))
        .map(str::to_string)
    else {
        return CoreError::BadRequest(
            "Tailscale non mostra un nome valido per questo computer: abilita MagicDNS e HTTPS nella console di Tailscale.".into(),
        )
        .into_response();
    };
    let dir = with_core(&state, |j| j.data_dir()).await;
    if let Err(message) = add_remote_host(&state, &dir, &host) {
        return CoreError::Internal(message).into_response();
    }
    let mut view = tailscale_view(&state, status);
    view["note"] = json!(note);
    Json(said(view, "tailscale.connect", None)).into_response()
}

async fn devices(State(state): State<Shared>) -> Response {
    let pairing_urls = pairing_urls(&state);
    reply(
        with_core(&state, move |j| {
            j.devices()
                .map(|items| json!({"items": items, "pairing_urls": pairing_urls}))
        })
        .await,
    )
}

async fn create_pairing(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let scopes: Vec<String> = body["scopes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    let origins = pairing_urls(&state);
    reply(
        with_core(&state, move |j| {
            let mut invite = j.create_pairing(&scopes)?;
            if let Some(previous) = body["replace"]
                .as_str()
                .filter(|old| invite["code"] != *old)
            {
                j.cancel_pairing(previous);
            }
            let code = invite["code"].as_str().unwrap_or_default().to_string();
            invite["links"] = json!(origins
                .iter()
                .map(|origin| pairing_link(origin, &code))
                .collect::<Vec<_>>());
            invite["link"] = invite["links"][0].clone();
            Ok(invite)
        })
        .await,
    )
}

async fn cancel_pairing(State(state): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let cancelled = with_core(&state, move |j| {
        j.cancel_pairing(body["code"].as_str().unwrap_or_default())
    })
    .await;
    Json(json!({"cancelled": cancelled}))
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

fn simulated_voice_setup() -> bool {
    std::env::var("JARVIS_SETUP_FAKE").as_deref() == Ok("1")
}

async fn setup_status(State(state): State<Shared>) -> Json<Value> {
    let setup = state.setup.clone();
    let mut status = tokio::task::spawn_blocking(move || setup.status())
        .await
        .unwrap_or_else(|_| json!({}));
    status["available"] = json!(simulated_voice_setup());
    Json(status)
}

async fn setup_start(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    if !simulated_voice_setup() {
        return CoreError::Unavailable("La voce richiede un provider audio cloud.".into())
            .into_response();
    }
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

async fn install_new_connector(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    reply(with_core(&state, move |j| j.install_new_connector(&body)).await)
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
                match tokio::task::spawn_blocking(move || manager.completed()).await {
                    Ok(Ok(completed)) if !completed.is_empty() => {
                        let notices = completed.clone();
                        match with_core(&state, move |j| j.n8n_notices(&notices)).await {
                            Ok(()) => {
                                let manager = state.n8n.clone();
                                match tokio::task::spawn_blocking(move || {
                                    manager.acknowledge(&completed)
                                })
                                .await
                                {
                                    Ok(Ok(())) => {}
                                    Ok(Err(error)) => {
                                        eprintln!("[n8n] conferma esecuzioni non riuscita: {error}")
                                    }
                                    Err(error) => {
                                        eprintln!("[n8n] conferma esecuzioni interrotta: {error}")
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!("[n8n] registrazione notifiche non riuscita: {error:?}")
                            }
                        }
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => eprintln!("[n8n] lettura esecuzioni non riuscita: {error}"),
                    Err(error) => eprintln!("[n8n] controllo esecuzioni interrotto: {error}"),
                }
            }
            // Missioni autonome (ADR-0096): una per giro, in background.
            let due = with_core(&state, |j| j.mission_due(chrono::Utc::now())).await;
            if let Some(id) = due {
                start_mission(&state, &id).await;
            }
            // Aggiornamenti (ADR-0034): al massimo ogni ora, con la rete fuori dal blocco.
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
        cloud_slots: Arc::new(tokio::sync::Semaphore::new(2)),
        remote_hosts: std::sync::RwLock::new(remote_hosts),
        nodes,
        device_db,
        setup,
        n8n,
        challenges: Mutex::new(std::collections::HashMap::new()),
        remote_failures: Mutex::new((std::time::Instant::now(), 0)),
        live_streams: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        jobs: Mutex::new(Vec::new()),
        job_handles: Mutex::new(std::collections::HashMap::new()),
    })
}
