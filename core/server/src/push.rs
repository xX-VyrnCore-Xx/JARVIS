//! Registrazioni FCM per notifiche in background.
//!
//! I token sono cifrati con `SecretStore`; SQLite conserva soltanto la loro impronta,
//! il proprietario e la piattaforma. Gli invii non bloccano l'Orchestrator e non
//! includono il contenuto della notifica.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const FCM_HOST: &str = "https://fcm.googleapis.com/v1/projects";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

pub struct PushSubscriptions {
    conn: Connection,
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

fn load_service_account() -> Result<ServiceAccount, ConfigError> {
    let path = std::env::var_os("JARVIS_FCM_SERVICE_ACCOUNT_FILE")
        .filter(|p| !p.is_empty())
        .ok_or(ConfigError::Missing)?;
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

pub fn delivery_config_status() -> &'static str {
    match load_service_account() {
        Ok(_) => "ready",
        Err(ConfigError::Missing) => "configuration_missing",
        Err(ConfigError::Invalid) => "configuration_invalid",
    }
}

impl PushSubscriptions {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|_| "database push non disponibile")?;
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
        Ok(Self { conn })
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
                "INSERT INTO push_subscriptions (token_hash, owner_id, platform)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(token_hash) DO UPDATE SET
                    owner_id = excluded.owner_id, platform = excluded.platform",
                params![hash, owner_id, platform.as_str()],
            )
            .is_err()
        {
            let _ = secrets.delete(&format!("push:{hash}"));
            return Err("registrazione push non disponibile".into());
        }
        Ok(())
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
    ) -> Result<(Vec<(String, String)>, usize), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT token_hash, platform FROM push_subscriptions")
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
                Ok(Some(token)) => registrations.push((token, platform)),
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
        let payloads: Vec<_> = registrations
            .into_iter()
            .map(|(token, _platform)| message_payload(&token, notification))
            .collect();
        let notification_id = notification["notification_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let category = notification["category"]
            .as_str()
            .unwrap_or("system")
            .to_string();
        let worker_notification_id = notification_id.clone();
        let worker_category = category.clone();
        let worker_obs = obs.clone();
        if std::thread::Builder::new()
            .name("jarvis-fcm".into())
            .spawn(move || {
                let account = match load_service_account() {
                    Ok(account) => account,
                    Err(ConfigError::Missing) => {
                        worker_obs.count("notify.fcm.configuration_missing");
                        worker_obs.log(
                            "error",
                            "notify.fcm.failed",
                            json!({"reason": "configuration_missing", "notification_id": worker_notification_id}),
                        );
                        return;
                    }
                    Err(ConfigError::Invalid) => {
                        worker_obs.count("notify.fcm.configuration_invalid");
                        worker_obs.log(
                            "error",
                            "notify.fcm.failed",
                            json!({"reason": "configuration_invalid", "notification_id": worker_notification_id}),
                        );
                        return;
                    }
                };
                let bearer = match access_token(&account) {
                    Ok(bearer) => bearer,
                    Err(reason) => {
                        worker_obs.count("notify.fcm.failed");
                        worker_obs.log(
                            "error",
                            "notify.fcm.failed",
                            json!({
                                "reason": reason,
                                "notification_id": worker_notification_id,
                                "failed": payloads.len()
                            }),
                        );
                        return;
                    }
                };
                let mut sent = 0usize;
                let mut failed = 0usize;
                let mut reasons = std::collections::BTreeMap::new();
                for payload in &payloads {
                    match send_message(&account, &bearer, payload) {
                        Ok(()) => sent += 1,
                        Err(reason) => {
                            failed += 1;
                            *reasons.entry(reason).or_insert(0usize) += 1;
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
                    if failed > 0 { "error" } else { "info" },
                    if failed > 0 {
                        "notify.fcm.failed"
                    } else {
                        "notify.fcm.sent"
                    },
                    json!({
                        "notification_id": worker_notification_id,
                        "category": worker_category,
                        "sent": sent,
                        "failed": failed,
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

fn message_payload(token: &str, notification: &Value) -> Value {
    json!({
        "message": {
            "token": token,
            "notification": {
                "title": "JARVIS",
                "body": "Apri JARVIS per i dettagli."
            },
            "data": {
                "notification_id": notification["notification_id"].as_str().unwrap_or_default(),
                "category": notification["category"].as_str().unwrap_or("system"),
                "severity": notification["severity"].as_str().unwrap_or("info")
            }
        }
    })
}

fn send_message(
    account: &ServiceAccount,
    bearer: &str,
    payload: &Value,
) -> Result<(), &'static str> {
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
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(_, _)) => Err("fcm_http_error"),
        Err(ureq::Error::Transport(_)) => Err("fcm_transport_error"),
    }
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
        assert_eq!(payload["message"]["notification"]["title"], "JARVIS");
        assert_eq!(
            payload["message"]["notification"]["body"],
            "Apri JARVIS per i dettagli."
        );
        assert_eq!(
            payload["message"]["data"]["notification_id"],
            "notification-1"
        );
        assert!(!serialized.contains("private title"));
        assert!(!serialized.contains("private notification body"));
    }
}
