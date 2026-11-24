//! Registrazioni FCM per notifiche in background.
//!
//! I token sono cifrati con `SecretStore`; SQLite conserva soltanto la loro impronta,
//! il proprietario e la piattaforma. Gli invii non bloccano l'Orchestrator e non
//! includono il contenuto della notifica.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const FCM_HOST: &str = "https://fcm.googleapis.com/v1/projects";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// Registrazioni push al massimo per principal: le più vecchie vengono sostituite (ADR-0098).
const MAX_PER_OWNER: usize = 5;
/// Tentativi di consegna per messaggio e attesa massima fra due tentativi.
const MAX_ATTEMPTS: u32 = 3;
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);
/// Il token OAuth di Google dura un'ora: si riusa per 50 minuti invece di firmarne uno a ogni notifica.
const BEARER_TTL: Duration = Duration::from_secs(50 * 60);

static BEARER: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// Registrazione pronta all'invio: impronta, token in chiaro e piattaforma.
type Registration = (String, String, String);

pub struct PushSubscriptions {
    conn: Connection,
    db_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Android,
    Web,
}

impl Platform {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "android" => Some(Self::Android),
            "web" => Some(Self::Web),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ConfigError {
    Missing,
    Invalid,
}

struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key_id: Option<String>,
    private_key_der: Vec<u8>,
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn validate_registration_token(token: &str) -> bool {
    (20..=4096).contains(&token.len())
        && token.is_ascii()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
}

fn valid_project_id(project: &str) -> bool {
    (6..=30).contains(&project.len())
        && project.is_ascii()
        && project
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && project.as_bytes()[0].is_ascii_alphanumeric()
        && project.as_bytes()[project.len() - 1].is_ascii_alphanumeric()
}

fn service_account_from_json(value: &Value) -> Result<ServiceAccount, ConfigError> {
    let object = value.as_object().ok_or(ConfigError::Invalid)?;
    let project_id = object
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|s| valid_project_id(s))
        .ok_or(ConfigError::Invalid)?;
    if object.get("type").and_then(Value::as_str) != Some("service_account")
        || object.get("token_uri").and_then(Value::as_str) != Some(TOKEN_URI)
    {
        return Err(ConfigError::Invalid);
    }
    let client_email = object
        .get("client_email")
        .and_then(Value::as_str)
        .filter(|email| {
            let Some(name_and_project) = email.strip_suffix(".iam.gserviceaccount.com") else {
                return false;
            };
            let Some((name, email_project)) = name_and_project.split_once('@') else {
                return false;
            };
            email.len() <= 254
                && !name.is_empty()
                && email_project == project_id
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        .ok_or(ConfigError::Invalid)?;
    let private_key = object
        .get("private_key")
        .and_then(Value::as_str)
        .ok_or(ConfigError::Invalid)?;
    let private_key_der = decode_private_key(private_key).ok_or(ConfigError::Invalid)?;
    ring::signature::RsaKeyPair::from_pkcs8(&private_key_der).map_err(|_| ConfigError::Invalid)?;
    let private_key_id = object
        .get("private_key_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    if private_key_id.as_ref().is_some_and(|id| {
        id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }) {
        return Err(ConfigError::Invalid);
    }
    Ok(ServiceAccount {
        project_id: project_id.to_string(),
        client_email: client_email.to_string(),
        private_key_id,
        private_key_der,
    })
}

fn decode_private_key(pem: &str) -> Option<Vec<u8>> {
    let pem = pem.trim();
    let body = pem
        .strip_prefix("-----BEGIN PRIVATE KEY-----")?
        .strip_suffix("-----END PRIVATE KEY-----")?;
    let compact: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .ok()
}

/// Nome del file dell'account di servizio nella cartella dati: così basta copiarlo lì, senza variabili
/// d'ambiente (che un'app avviata dal Finder o dal menu Start non eredita dalla shell).
pub const SERVICE_ACCOUNT_FILE: &str = "fcm-service-account.json";

/// Percorso dell'account di servizio: prima `JARVIS_FCM_SERVICE_ACCOUNT_FILE`, poi il file nella cartella dati.
fn service_account_path(data_dir: &Path) -> Option<PathBuf> {
    if let Some(path) =
        std::env::var_os("JARVIS_FCM_SERVICE_ACCOUNT_FILE").filter(|p| !p.is_empty())
    {
        return Some(path.into());
    }
    let default = data_dir.join(SERVICE_ACCOUNT_FILE);
    default.is_file().then_some(default)
}

fn load_service_account(data_dir: &Path) -> Result<ServiceAccount, ConfigError> {
    let path = service_account_path(data_dir).ok_or(ConfigError::Missing)?;
    let raw = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ConfigError::Missing
        } else {
            ConfigError::Invalid
        }
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| ConfigError::Invalid)?;
    service_account_from_json(&value)
}

pub fn delivery_config_status(data_dir: &Path) -> &'static str {
    match load_service_account(data_dir) {
        Ok(_) => "ready",
        Err(ConfigError::Missing) => "configuration_missing",
        Err(ConfigError::Invalid) => "configuration_invalid",
    }
}

impl PushSubscriptions {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|_| "database push non disponibile")?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|_| "database push non disponibile")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS push_subscriptions (
                token_hash TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                platform TEXT NOT NULL CHECK (platform IN ('android', 'web'))
             );
             CREATE INDEX IF NOT EXISTS push_subscriptions_owner
             ON push_subscriptions(owner_id);",
        )
        .map_err(|_| "schema push non disponibile")?;
        // Migrazione additiva: data di creazione, ultimo invio riuscito e segnalazione di token non più validi.
        for (column, definition) in [
            ("created_at", "TEXT"),
            ("last_ok_at", "TEXT"),
            ("invalid_at", "TEXT"),
        ] {
            let exists = conn
                .prepare(&format!("SELECT {column} FROM push_subscriptions LIMIT 0"))
                .is_ok();
            if !exists {
                conn.execute_batch(&format!(
                    "ALTER TABLE push_subscriptions ADD COLUMN {column} {definition};"
                ))
                .map_err(|_| "schema push non disponibile")?;
            }
        }
        Ok(Self {
            conn,
            db_path: db_path.to_path_buf(),
        })
    }

    /// Elimina le registrazioni che FCM ha dichiarato non più valide, con il loro token cifrato.
    pub fn purge_invalid(&self, secrets: &crate::secrets::SecretStore) -> usize {
        let Ok(mut stmt) = self
            .conn
            .prepare("SELECT token_hash FROM push_subscriptions WHERE invalid_at IS NOT NULL")
        else {
            return 0;
        };
        let hashes: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        drop(stmt);
        for hash in &hashes {
            let _ = secrets.delete(&format!("push:{hash}"));
            let _ = self.conn.execute(
                "DELETE FROM push_subscriptions WHERE token_hash = ?1",
                [hash],
            );
        }
        hashes.len()
    }

    fn data_dir(&self) -> &Path {
        self.db_path.parent().unwrap_or(Path::new("."))
    }

    /// Stato per il proprietario: nessun token, solo conteggi e ultimo invio riuscito.
    pub fn status(&self) -> Value {
        let count = |sql: &str| -> i64 { self.conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
        let last_ok: Option<String> = self
            .conn
            .query_row("SELECT MAX(last_ok_at) FROM push_subscriptions", [], |r| {
                r.get(0)
            })
            .unwrap_or(None);
        json!({
            "delivery": delivery_config_status(self.data_dir()),
            "registered": count("SELECT COUNT(*) FROM push_subscriptions WHERE invalid_at IS NULL"),
            "invalid_pending": count("SELECT COUNT(*) FROM push_subscriptions WHERE invalid_at IS NOT NULL"),
            "last_ok_at": last_ok,
            "max_per_device": MAX_PER_OWNER,
        })
    }

    pub fn register(
        &self,
        secrets: &crate::secrets::SecretStore,
        owner_id: &str,
        token: &str,
        platform: Platform,
    ) -> Result<(), String> {
        if !validate_registration_token(token) {
            return Err("token FCM non valido".into());
        }
        let hash = token_hash(token);
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT owner_id FROM push_subscriptions WHERE token_hash = ?1",
                [&hash],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "registrazione push non disponibile")?;
        if current
            .as_deref()
            .is_some_and(|current| current != owner_id)
        {
            return Err("registrazione push già associata a un altro principal".into());
        }

        secrets.put(&format!("push:{hash}"), token)?;
        if self
            .conn
            .execute(
                "INSERT INTO push_subscriptions (token_hash, owner_id, platform, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(token_hash) DO UPDATE SET
                    owner_id = excluded.owner_id, platform = excluded.platform, invalid_at = NULL",
                params![hash, owner_id, platform.as_str(), crate::now()],
            )
            .is_err()
        {
            let _ = secrets.delete(&format!("push:{hash}"));
            return Err("registrazione push non disponibile".into());
        }
        self.trim_owner(secrets, owner_id);
        Ok(())
    }

    /// Un principal conserva al massimo `MAX_PER_OWNER` registrazioni: le più vecchie vengono eliminate.
    /// Un dispositivo che reinstalla l'app o ruota il token non accumula registrazioni morte.
    fn trim_owner(&self, secrets: &crate::secrets::SecretStore, owner_id: &str) {
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT token_hash FROM push_subscriptions WHERE owner_id = ?1
             ORDER BY COALESCE(created_at, '') DESC, rowid DESC",
        ) else {
            return;
        };
        let hashes: Vec<String> = stmt
            .query_map([owner_id], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        drop(stmt);
        for hash in hashes.iter().skip(MAX_PER_OWNER) {
            let _ = secrets.delete(&format!("push:{hash}"));
            let _ = self.conn.execute(
                "DELETE FROM push_subscriptions WHERE token_hash = ?1",
                [hash],
            );
        }
    }

    pub fn remove_for_owner(
        &self,
        secrets: &crate::secrets::SecretStore,
        owner_id: &str,
    ) -> Result<usize, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT token_hash FROM push_subscriptions WHERE owner_id = ?1")
            .map_err(|_| "registrazioni push non disponibili")?;
        let hashes = stmt
            .query_map([owner_id], |row| row.get::<_, String>(0))
            .map_err(|_| "registrazioni push non disponibili")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "registrazioni push non disponibili")?;
        drop(stmt);

        for hash in &hashes {
            secrets.delete(&format!("push:{hash}"))?;
        }
        self.conn
            .execute(
                "DELETE FROM push_subscriptions WHERE owner_id = ?1",
                [owner_id],
            )
            .map_err(|_| "registrazioni push non disponibili")?;
        Ok(hashes.len())
    }

    fn registrations(
        &self,
        secrets: &crate::secrets::SecretStore,
    ) -> Result<(Vec<Registration>, usize), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT token_hash, platform FROM push_subscriptions WHERE invalid_at IS NULL")
            .map_err(|_| "registrazioni push non disponibili")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "registrazioni push non disponibili")?;
        let mut registrations = Vec::new();
        let mut missing_secrets = 0;
        for row in rows {
            let (hash, platform) = row.map_err(|_| "registrazioni push non disponibili")?;
            match secrets.get(&format!("push:{hash}")) {
                Ok(Some(token)) => registrations.push((hash, token, platform)),
                Ok(None) | Err(_) => missing_secrets += 1,
            }
        }
        Ok((registrations, missing_secrets))
    }

    pub fn dispatch(
        &self,
        secrets: &crate::secrets::SecretStore,
        notification: &Value,
        obs: std::sync::Arc<crate::observability::Observability>,
    ) {
        let purged = self.purge_invalid(secrets);
        if purged > 0 {
            obs.log("info", "notify.fcm.purged", json!({"count": purged}));
        }
        let (registrations, missing_secrets) = match self.registrations(secrets) {
            Ok(result) => result,
            Err(_) => {
                obs.count("notify.fcm.failed");
                obs.log(
                    "error",
                    "notify.fcm.failed",
                    json!({"reason": "registration_store"}),
                );
                return;
            }
        };
        if missing_secrets > 0 {
            obs.count("notify.fcm.failed");
            obs.log(
                "error",
                "notify.fcm.failed",
                json!({"reason": "encrypted_token_unavailable", "count": missing_secrets}),
            );
        }
        if registrations.is_empty() {
            return;
        }
        let jobs: Vec<(String, Value)> = registrations
            .into_iter()
            .map(|(hash, token, _platform)| (hash, message_payload(&token, notification)))
            .collect();
        let notification_id = notification["notification_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let category = notification["category"]
            .as_str()
            .unwrap_or("system")
            .to_string();
        let db_path = self.db_path.clone();
        let data_dir = self.data_dir().to_path_buf();
        let (worker_id, worker_category, worker_obs) =
            (notification_id.clone(), category, obs.clone());
        if std::thread::Builder::new()
            .name("jarvis-fcm".into())
            .spawn(move || {
                let account = match load_service_account(&data_dir) {
                    Ok(account) => account,
                    Err(error) => {
                        let reason = match error {
                            ConfigError::Missing => "configuration_missing",
                            ConfigError::Invalid => "configuration_invalid",
                        };
                        worker_obs.count(&format!("notify.fcm.{reason}"));
                        worker_obs.log(
                            "error",
                            "notify.fcm.failed",
                            json!({"reason": reason, "notification_id": worker_id}),
                        );
                        return;
                    }
                };
                let mut bearer = match cached_bearer(&account, false) {
                    Ok(bearer) => bearer,
                    Err(reason) => {
                        worker_obs.count("notify.fcm.failed");
                        worker_obs.log(
                            "error",
                            "notify.fcm.failed",
                            json!({"reason": reason, "notification_id": worker_id, "failed": jobs.len()}),
                        );
                        return;
                    }
                };
                let (mut sent, mut failed, mut removed) = (0usize, 0usize, 0usize);
                let mut reasons = std::collections::BTreeMap::new();
                let bookkeeping = Connection::open(&db_path).ok();
                if let Some(conn) = &bookkeeping {
                    let _ = conn.busy_timeout(Duration::from_secs(5));
                }
                for (hash, payload) in &jobs {
                    let outcome = deliver(&account, &mut bearer, payload);
                    match outcome {
                        Outcome::Sent => {
                            sent += 1;
                            if let Some(conn) = &bookkeeping {
                                let _ = conn.execute(
                                    "UPDATE push_subscriptions SET last_ok_at = ?2 WHERE token_hash = ?1",
                                    params![hash, crate::now()],
                                );
                            }
                        }
                        Outcome::Unregistered => {
                            failed += 1;
                            removed += 1;
                            *reasons.entry("fcm_token_unregistered").or_insert(0usize) += 1;
                            // Il token è morto: lo si segnala e il Core lo elimina al prossimo invio.
                            if let Some(conn) = &bookkeeping {
                                let _ = conn.execute(
                                    "UPDATE push_subscriptions SET invalid_at = ?2 WHERE token_hash = ?1",
                                    params![hash, crate::now()],
                                );
                            }
                        }
                        other => {
                            failed += 1;
                            *reasons.entry(other.reason()).or_insert(0usize) += 1;
                        }
                    }
                }
                if sent > 0 {
                    worker_obs.count("notify.fcm.sent");
                }
                if failed > 0 {
                    worker_obs.count("notify.fcm.failed");
                }
                worker_obs.log(
                    if failed > removed { "error" } else { "info" },
                    if failed > 0 { "notify.fcm.failed" } else { "notify.fcm.sent" },
                    json!({
                        "notification_id": worker_id, "category": worker_category,
                        "sent": sent, "failed": failed, "unregistered": removed,
                        "failure_reasons": reasons
                    }),
                );
            })
            .is_err()
        {
            obs.count("notify.fcm.failed");
            obs.log(
                "error",
                "notify.fcm.failed",
                json!({"reason": "worker_unavailable", "notification_id": notification_id}),
            );
        }
    }
}

/// Esito di un tentativo di consegna a FCM.
#[derive(Debug, PartialEq)]
enum Outcome {
    Sent,
    /// Il token non è più valido (app disinstallata o token ruotato): va eliminato.
    Unregistered,
    /// Errore temporaneo: si può riprovare, eventualmente dopo l'attesa indicata dal servizio.
    Retry(Option<Duration>),
    /// Credenziale rifiutata: si rinnova il token OAuth e si riprova una volta.
    Auth,
    /// Messaggio rifiutato in modo definitivo.
    Rejected,
    Transport,
}

impl Outcome {
    fn reason(&self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Unregistered => "fcm_token_unregistered",
            Self::Retry(_) => "fcm_unavailable",
            Self::Auth => "fcm_auth_error",
            Self::Rejected => "fcm_http_error",
            Self::Transport => "fcm_transport_error",
        }
    }
}

/// Interpreta la risposta di FCM v1: 404 (o 400 con `UNREGISTERED`) = token morto; 429 e 5xx = riprova;
/// 401 e 403 = credenziale da rinnovare; altri 4xx = rifiuto definitivo.
fn classify(status: u16, body: &str, retry_after: Option<&str>) -> Outcome {
    match status {
        200..=299 => Outcome::Sent,
        404 => Outcome::Unregistered,
        400 if body.contains("UNREGISTERED") => Outcome::Unregistered,
        401 | 403 => Outcome::Auth,
        429 | 500 | 502 | 503 | 504 => Outcome::Retry(
            retry_after
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|s| Duration::from_secs(s.max(1)).min(MAX_RETRY_WAIT)),
        ),
        _ => Outcome::Rejected,
    }
}

/// Consegna un messaggio con al massimo `MAX_ATTEMPTS` tentativi: attesa esponenziale (o `Retry-After`)
/// per gli errori temporanei, un solo rinnovo della credenziale per 401/403.
fn deliver(account: &ServiceAccount, bearer: &mut String, payload: &Value) -> Outcome {
    let mut refreshed = false;
    let mut last = Outcome::Transport;
    for attempt in 0..MAX_ATTEMPTS {
        let outcome = send_once(account, bearer, payload);
        match outcome {
            Outcome::Auth if !refreshed => {
                refreshed = true;
                match cached_bearer(account, true) {
                    Ok(fresh) => *bearer = fresh,
                    Err(_) => return Outcome::Auth,
                }
                last = Outcome::Auth;
            }
            Outcome::Retry(_) | Outcome::Transport if attempt + 1 < MAX_ATTEMPTS => {
                let wait = match &outcome {
                    Outcome::Retry(Some(wait)) => *wait,
                    _ => Duration::from_secs(1 << attempt),
                };
                std::thread::sleep(wait.min(MAX_RETRY_WAIT));
                last = outcome;
            }
            other => return other,
        }
    }
    last
}

/// Struttura del messaggio: dati generici e nessun contenuto privato. Gli avvisi che richiedono attenzione
/// hanno priorità alta e durano un giorno; briefing e suggerimenti si sostituiscono a vicenda (`collapse_key`)
/// e scadono prima, perché un briefing di ieri non serve più.
fn message_payload(token: &str, notification: &Value) -> Value {
    let category = notification["category"].as_str().unwrap_or("system");
    let severity = notification["severity"].as_str().unwrap_or("info");
    let urgent = matches!(severity, "warning" | "error")
        || matches!(category, "confirmation" | "security" | "reminder");
    let mut android = json!({
        "priority": if urgent { "HIGH" } else { "NORMAL" },
        "ttl": if urgent { "86400s" } else { "21600s" },
    });
    if matches!(category, "briefing" | "suggestion") {
        android["collapse_key"] = format!("jarvis-{category}").into();
    }
    json!({
        "message": {
            "token": token,
            "android": android,
            "data": {
                "title": "JARVIS",
                "body": "Apri JARVIS per i dettagli.",
                "notification_id": notification["notification_id"].as_str().unwrap_or_default(),
                "category": category,
                "severity": severity
            }
        }
    })
}

fn send_once(account: &ServiceAccount, bearer: &str, payload: &Value) -> Outcome {
    let endpoint = format!("{FCM_HOST}/{}/messages:send", account.project_id);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .build();
    let response = agent
        .post(&endpoint)
        .set("Authorization", &format!("Bearer {bearer}"))
        .set("Content-Type", "application/json")
        .send_string(&payload.to_string());
    match response {
        Ok(_) => Outcome::Sent,
        Err(ureq::Error::Status(status, response)) => {
            let retry_after = response.header("Retry-After").map(str::to_string);
            let mut body = String::new();
            let _ = response.into_reader().take(4096).read_to_string(&mut body);
            classify(status, &body, retry_after.as_deref())
        }
        Err(ureq::Error::Transport(_)) => Outcome::Transport,
    }
}

/// Token OAuth riusato fino a 50 minuti; `force` lo rinnova (credenziale rifiutata).
fn cached_bearer(account: &ServiceAccount, force: bool) -> Result<String, &'static str> {
    let mut cache = BEARER.lock().unwrap_or_else(|e| e.into_inner());
    if !force {
        if let Some((token, at)) = cache.as_ref() {
            if at.elapsed() < BEARER_TTL {
                return Ok(token.clone());
            }
        }
    }
    let token = access_token(account)?;
    *cache = Some((token.clone(), Instant::now()));
    Ok(token)
}

fn access_token(account: &ServiceAccount) -> Result<String, &'static str> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "clock_error")?
        .as_secs();
    let mut header = json!({"alg": "RS256", "typ": "JWT"});
    if let Some(key_id) = &account.private_key_id {
        header["kid"] = key_id.clone().into();
    }
    let claims = json!({
        "iss": account.client_email,
        "scope": FCM_SCOPE,
        "aud": TOKEN_URI,
        "iat": now,
        "exp": now + 3600
    });
    let encoded_header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header.to_string());
    let encoded_claims =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    let signing_input = format!("{encoded_header}.{encoded_claims}");
    let key_pair = ring::signature::RsaKeyPair::from_pkcs8(&account.private_key_der)
        .map_err(|_| "private_key_invalid")?;
    let mut signature = vec![0; key_pair.public().modulus_len()];
    key_pair
        .sign(
            &ring::signature::RSA_PKCS1_SHA256,
            &ring::rand::SystemRandom::new(),
            signing_input.as_bytes(),
            &mut signature,
        )
        .map_err(|_| "jwt_signing_failed")?;
    let assertion = format!(
        "{signing_input}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
    );
    let body = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .build();
    let response = agent
        .post(TOKEN_URI)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|error| match error {
            ureq::Error::Status(_, _) => "oauth_http_error",
            ureq::Error::Transport(_) => "oauth_transport_error",
        })?;
    let mut raw = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut raw)
        .map_err(|_| "oauth_response_invalid")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "oauth_response_invalid")?;
    let token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty() && token.len() <= 8192)
        .ok_or("oauth_response_invalid")?;
    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_tokens_are_limited_to_fcm_token_characters_and_size() {
        assert!(validate_registration_token(
            "fcm:android-token_123456789012345"
        ));
        assert!(!validate_registration_token("too-short"));
        assert!(!validate_registration_token(
            "fcm:token with whitespace 1234567890"
        ));
        assert!(!validate_registration_token(&"a".repeat(4097)));
        assert!(Platform::parse("android").is_some());
        assert!(Platform::parse("web").is_some());
        assert!(Platform::parse("ios").is_none());
    }

    #[test]
    fn service_account_rejects_untrusted_endpoint_and_invalid_project() {
        let mut account = json!({
            "type": "service_account",
            "project_id": "jarvis-prod-1",
            "client_email": "push@jarvis-prod-1.iam.gserviceaccount.com",
            "token_uri": TOKEN_URI,
            "private_key": "not a private key"
        });
        assert_eq!(
            service_account_from_json(&account).err(),
            Some(ConfigError::Invalid)
        );
        account["token_uri"] = "https://attacker.example/token".into();
        assert_eq!(
            service_account_from_json(&account).err(),
            Some(ConfigError::Invalid)
        );
        account["token_uri"] = TOKEN_URI.into();
        account["project_id"] = "bad/project".into();
        assert_eq!(
            service_account_from_json(&account).err(),
            Some(ConfigError::Invalid)
        );
        account["project_id"] = "jarvis-prod-1".into();
        account["type"] = "authorized_user".into();
        assert_eq!(
            service_account_from_json(&account).err(),
            Some(ConfigError::Invalid)
        );
    }

    #[test]
    fn fcm_payload_uses_generic_text_and_omits_notification_body() {
        let source = json!({
            "notification_id": "notification-1",
            "category": "automation",
            "severity": "warning",
            "title": "private title",
            "body": "private notification body"
        });
        let payload = message_payload("fcm:token_123456789012345", &source);
        let serialized = payload.to_string();
        assert_eq!(payload["message"]["android"]["priority"], "HIGH");
        assert_eq!(payload["message"]["data"]["title"], "JARVIS");
        assert_eq!(
            payload["message"]["data"]["body"],
            "Apri JARVIS per i dettagli."
        );
        assert!(payload["message"].get("notification").is_none());
        assert_eq!(
            payload["message"]["data"]["notification_id"],
            "notification-1"
        );
        assert!(!serialized.contains("private title"));
        assert!(!serialized.contains("private notification body"));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-push-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_service_account_is_found_in_the_data_directory() {
        // Il test non imposta variabili d'ambiente (sono globali): vale quando la variabile non c'è.
        if std::env::var_os("JARVIS_FCM_SERVICE_ACCOUNT_FILE").is_some() {
            return;
        }
        let dir = scratch("account");
        assert_eq!(delivery_config_status(&dir), "configuration_missing");
        std::fs::write(dir.join(SERVICE_ACCOUNT_FILE), "non è json").unwrap();
        assert_eq!(
            service_account_path(&dir),
            Some(dir.join(SERVICE_ACCOUNT_FILE))
        );
        assert_eq!(delivery_config_status(&dir), "configuration_invalid");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fcm_responses_are_classified() {
        assert_eq!(classify(200, "", None), Outcome::Sent);
        assert_eq!(
            classify(404, r#"{"error":{"status":"NOT_FOUND"}}"#, None),
            Outcome::Unregistered
        );
        assert_eq!(
            classify(
                400,
                r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#,
                None
            ),
            Outcome::Unregistered
        );
        assert_eq!(
            classify(400, r#"{"error":{"message":"payload non valido"}}"#, None),
            Outcome::Rejected
        );
        assert_eq!(classify(401, "", None), Outcome::Auth);
        assert_eq!(classify(403, "", None), Outcome::Auth);
        assert_eq!(classify(503, "", None), Outcome::Retry(None));
        assert_eq!(
            classify(429, "", Some("7")),
            Outcome::Retry(Some(Duration::from_secs(7)))
        );
        assert_eq!(
            classify(429, "", Some("9999")),
            Outcome::Retry(Some(MAX_RETRY_WAIT)),
            "l'attesa richiesta dal servizio ha un tetto"
        );
        assert_eq!(classify(429, "", Some("domani")), Outcome::Retry(None));
        assert_eq!(classify(422, "", None), Outcome::Rejected);
    }

    #[test]
    fn priority_and_expiry_follow_urgency_and_briefings_collapse() {
        let urgent = message_payload(
            "t",
            &json!({"category": "confirmation", "severity": "info"}),
        );
        assert_eq!(urgent["message"]["android"]["priority"], "HIGH");
        assert_eq!(urgent["message"]["android"]["ttl"], "86400s");
        assert!(urgent["message"]["android"].get("collapse_key").is_none());
        let routine = message_payload("t", &json!({"category": "mission", "severity": "info"}));
        assert_eq!(routine["message"]["android"]["priority"], "NORMAL");
        assert_eq!(routine["message"]["android"]["ttl"], "21600s");
        let briefing = message_payload("t", &json!({"category": "briefing", "severity": "info"}));
        assert_eq!(
            briefing["message"]["android"]["collapse_key"],
            "jarvis-briefing"
        );
        let failed = message_payload("t", &json!({"category": "system", "severity": "error"}));
        assert_eq!(failed["message"]["android"]["priority"], "HIGH");
    }

    #[test]
    fn registrations_are_capped_migrated_and_purged() {
        let dir = scratch("registry");
        let db = dir.join("jarvis.sqlite3");
        // Database creato prima delle nuove colonne: la migrazione le aggiunge senza perdere righe.
        {
            let old = Connection::open(&db).unwrap();
            old.execute_batch(
                "CREATE TABLE push_subscriptions (token_hash TEXT PRIMARY KEY, owner_id TEXT NOT NULL,
                    platform TEXT NOT NULL CHECK (platform IN ('android', 'web')));
                 INSERT INTO push_subscriptions VALUES ('vecchio', 'device-a', 'android');",
            )
            .unwrap();
        }
        let push = PushSubscriptions::open(&db).unwrap();
        PushSubscriptions::open(&db).unwrap();
        assert_eq!(push.status()["registered"], 1);
        let secrets = crate::secrets::SecretStore::open_with(&dir, None).unwrap();
        for i in 0..8 {
            let token = format!("fcm:token_sintetico_numero_{i:02}_abcdefghij");
            push.register(&secrets, "device-a", &token, Platform::Android)
                .unwrap();
        }
        let status = push.status();
        assert_eq!(status["registered"], MAX_PER_OWNER, "{status}");
        // Un altro principal non è toccato dal tetto.
        push.register(
            &secrets,
            "device-b",
            "fcm:token_di_un_altro_dispositivo_01",
            Platform::Android,
        )
        .unwrap();
        assert_eq!(push.status()["registered"], MAX_PER_OWNER + 1);
        // Un token dichiarato non valido viene eliminato (riga e segreto) al prossimo invio.
        let hash = token_hash("fcm:token_di_un_altro_dispositivo_01");
        push.conn
            .execute(
                "UPDATE push_subscriptions SET invalid_at = ?2 WHERE token_hash = ?1",
                params![hash, crate::now()],
            )
            .unwrap();
        assert_eq!(push.status()["invalid_pending"], 1);
        assert_eq!(push.purge_invalid(&secrets), 1);
        assert_eq!(push.status()["invalid_pending"], 0);
        assert_eq!(secrets.get(&format!("push:{hash}")).unwrap(), None);
        let (live, missing) = push.registrations(&secrets).unwrap();
        assert_eq!((live.len(), missing), (MAX_PER_OWNER, 0));
        let _ = std::fs::remove_dir_all(dir);
    }
}
