//! OAuth 2.0 per i connettori (RF-14, ADR-0027): codice di autorizzazione con PKCE (RFC 7636) e redirect
//! sul loopback del Core (RFC 8252), adatto a un'app desktop senza segreto del client.
//!
//! - Il Core genera `state` e `code_verifier` casuali, validi 10 minuti e una sola volta.
//! - Il proprietario apre la pagina di consenso nel proprio browser; Google Desktop usa il redirect
//!   loopback `http://127.0.0.1:<porta>`, mentre gli altri provider usano la route callback API.
//! - Il Core scambia il codice con i token sull'endpoint dichiarato dal manifest, il cui host deve essere
//!   tra gli `allowed_hosts`, con le stesse protezioni dei connettori: HTTPS, niente redirect, indirizzi
//!   privati bloccati.
//! - Access token e refresh token sono cifrati nell'archivio dei segreti. Il rinnovo è automatico a 60
//!   secondi dalla scadenza.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Value};
use sha2::Digest;
use url::Url;

use crate::web::WebConfig;

const PENDING_TTL: Duration = Duration::from_secs(600);

pub struct Pending {
    pub connector: String,
    pub verifier: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    created: Instant,
}

/// Autorizzazioni in corso, indicizzate per `state`.
#[derive(Default)]
pub struct Flows {
    pending: HashMap<String, Pending>,
}

fn random(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::getrandom(&mut b).expect("generatore casuale del sistema");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Sfida PKCE S256: base64url(SHA-256(verifier)).
pub fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()))
}

impl Flows {
    /// Avvia un'autorizzazione e restituisce l'URL della pagina di consenso.
    pub fn start(
        &mut self,
        manifest: &Value,
        port: u16,
        client_secret: Option<String>,
    ) -> Result<String, String> {
        self.pending
            .retain(|_, p| p.created.elapsed() < PENDING_TTL);
        let auth = &manifest["auth"];
        let mut url = Url::parse(auth["authorize_url"].as_str().unwrap_or_default())
            .map_err(|_| "authorize_url non valido".to_string())?;
        let google_oauth = url.host_str() == Some("accounts.google.com");
        let state = random(32);
        let verifier = random(48);
        let redirect_uri = if google_oauth {
            format!("http://127.0.0.1:{port}")
        } else {
            format!("http://localhost:{port}/api/connectors/oauth/callback")
        };
        let scopes: Vec<&str> = auth["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", "code")
                .append_pair("client_id", auth["client_id"].as_str().unwrap_or_default())
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("state", &state)
                .append_pair("code_challenge", &challenge(&verifier))
                .append_pair("code_challenge_method", "S256");
            if !scopes.is_empty() {
                q.append_pair("scope", &scopes.join(" "));
            }
            if google_oauth {
                q.append_pair("access_type", "offline")
                    .append_pair("prompt", "consent");
            }
        }
        let connector = manifest["id"].as_str().unwrap_or_default().to_string();
        self.cancel(&connector);
        self.pending.insert(
            state,
            Pending {
                connector,
                verifier,
                client_secret,
                redirect_uri,
                created: Instant::now(),
            },
        );
        Ok(url.to_string())
    }

    pub fn cancel(&mut self, id: &str) {
        self.pending.retain(|_, pending| pending.connector != id);
    }

    /// Consuma lo `state`: valido una sola volta e solo entro 10 minuti.
    pub fn take(&mut self, state: &str) -> Option<Pending> {
        let p = self.pending.remove(state)?;
        (p.created.elapsed() < PENDING_TTL).then_some(p)
    }
}

fn form(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish().into_bytes()
}

fn allowed_hosts(manifest: &Value) -> Vec<String> {
    manifest["allowed_hosts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str().map(str::to_string))
        .collect()
}

/// Richiesta all'endpoint dei token; restituisce lo stato dei token da salvare (`access_token`,
/// `refresh_token`, `expires_at` in secondi Unix, `client_secret` se fornito).
fn token_request(
    manifest: &Value,
    web: &WebConfig,
    pairs: &[(&str, &str)],
    keep: &Value,
) -> Result<Value, String> {
    let url = Url::parse(manifest["auth"]["token_url"].as_str().unwrap_or_default())
        .map_err(|_| "token_url non valido".to_string())?;
    let headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        (
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        ),
    ];
    let (status, _, data) = crate::web::request(
        web,
        "POST",
        &url,
        &headers,
        Some(&form(pairs)),
        &allowed_hosts(manifest),
    )
    .map_err(|e| e.message)?;
    let body: Value = serde_json::from_slice(&data).unwrap_or(Value::Null);
    if !(200..300).contains(&status)
        || !body["access_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    {
        return Err(format!("il servizio ha rifiutato i token (HTTP {status})"));
    }
    let expires_in = body["expires_in"].as_u64().unwrap_or(3600);
    let now = chrono::Utc::now().timestamp() as u64;
    Ok(json!({
        "access_token": body["access_token"],
        // Alcuni servizi non restituiscono un nuovo refresh token al rinnovo: si conserva il precedente.
        "refresh_token": body["refresh_token"].as_str().filter(|token| !token.is_empty()).map(Value::from).unwrap_or_else(|| keep["refresh_token"].clone()),
        "expires_at": now.saturating_add(expires_in),
        "client_secret": keep["client_secret"],
        "scope": body["scope"],
    }))
}

/// Scambio del codice di autorizzazione con i token.
pub fn exchange(
    manifest: &Value,
    web: &WebConfig,
    code: &str,
    pending: &Pending,
) -> Result<Value, String> {
    let client_id = manifest["auth"]["client_id"].as_str().unwrap_or_default();
    let mut pairs = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", pending.redirect_uri.as_str()),
        ("client_id", client_id),
        ("code_verifier", pending.verifier.as_str()),
    ];
    if let Some(s) = pending.client_secret.as_deref() {
        pairs.push(("client_secret", s));
    }
    token_request(
        manifest,
        web,
        &pairs,
        &json!({"client_secret": pending.client_secret}),
    )
}

/// Vero se il token scade entro 60 secondi.
pub fn expiring(state: &Value) -> bool {
    state["expires_at"]
        .as_u64()
        .is_some_and(|t| t <= chrono::Utc::now().timestamp() as u64 + 60)
}

/// Rinnovo con il refresh token.
pub fn refresh(manifest: &Value, web: &WebConfig, state: &Value) -> Result<Value, String> {
    let refresh = state["refresh_token"]
        .as_str()
        .ok_or("nessun refresh token: autorizza di nuovo il connettore")?;
    let client_id = manifest["auth"]["client_id"].as_str().unwrap_or_default();
    let mut pairs = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", client_id),
    ];
    if let Some(s) = state["client_secret"].as_str() {
        pairs.push(("client_secret", s));
    }
    token_request(manifest, web, &pairs, state)
}

/// Revoca presso il servizio (RFC 7009): prima il refresh token, che invalida anche gli access token
/// collegati, altrimenti l'access token. Restituisce un esito leggibile; i token locali vengono cancellati
/// comunque dal chiamante.
pub fn revoke(
    manifest: &Value,
    web: &WebConfig,
    state: &Value,
    access: Option<&str>,
) -> Result<(), String> {
    let Some(raw) = manifest["auth"]["revoke_url"].as_str() else {
        return Err("il servizio non dichiara un endpoint di revoca".into());
    };
    let url = Url::parse(raw).map_err(|_| "revoke_url non valido".to_string())?;
    let (token, hint) = match (state["refresh_token"].as_str(), access) {
        (Some(r), _) => (r.to_string(), "refresh_token"),
        (None, Some(a)) => (a.to_string(), "access_token"),
        (None, None) => return Err("nessun token da revocare".into()),
    };
    let client_id = manifest["auth"]["client_id"].as_str().unwrap_or_default();
    let mut pairs = vec![
        ("token", token.as_str()),
        ("token_type_hint", hint),
        ("client_id", client_id),
    ];
    if let Some(s) = state["client_secret"].as_str() {
        pairs.push(("client_secret", s));
    }
    let headers = vec![(
        "Content-Type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    )];
    let (status, _, _) = crate::web::request(
        web,
        "POST",
        &url,
        &headers,
        Some(&form(&pairs)),
        &allowed_hosts(manifest),
    )
    .map_err(|e| e.message)?;
    // RFC 7009: 200 anche per un token già scaduto o sconosciuto.
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(format!("il servizio ha risposto HTTP {status} alla revoca"))
    }
}

/// Pagina mostrata nel browser al ritorno dal servizio.
pub fn page(ok: bool, message: &str) -> String {
    let esc = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><html lang=\"it\"><meta charset=\"utf-8\"><title>JARVIS</title><body style=\"font-family:system-ui;background:#030a14;color:#dff4ff;display:grid;place-items:center;min-height:100vh;margin:0\"><main style=\"text-align:center;max-width:32rem;padding:1rem\"><h1>{}</h1><p>{esc}</p><p>Puoi chiudere questa scheda e tornare a JARVIS.</p></main></body></html>",
        if ok { "Connettore autorizzato" } else { "Autorizzazione non riuscita" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc_7636_example() {
        // Appendice B della RFC 7636.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn state_is_single_use_and_bound_to_the_connector() {
        let manifest = json!({"id": "posta", "auth": {"type": "oauth2", "authorize_url": "https://auth.example/authorize",
            "token_url": "https://auth.example/token", "client_id": "jarvis", "scopes": ["read", "mail.send"]}});
        let mut flows = Flows::default();
        let url = Url::parse(&flows.start(&manifest, 8765, None).unwrap()).unwrap();
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(
            q["redirect_uri"],
            "http://localhost:8765/api/connectors/oauth/callback"
        );
        assert_eq!(
            (q["code_challenge_method"].as_str(), q["scope"].as_str()),
            ("S256", "read mail.send")
        );
        assert!(!q.contains_key("access_type"));
        assert!(!q.contains_key("prompt"));
        let p = flows.take(&q["state"]).expect("state valido");
        assert_eq!(p.connector, "posta");
        assert_eq!(challenge(&p.verifier), q["code_challenge"]);
        assert!(flows.take(&q["state"]).is_none(), "state monouso");
        assert!(flows.take("inventato").is_none());
    }

    #[test]
    fn google_oauth_requests_a_refresh_token() {
        let manifest = json!({"id": "gmail", "auth": {"type": "oauth2",
            "authorize_url": "https://accounts.google.com/o/oauth2/v2/auth",
            "token_url": "https://oauth2.googleapis.com/token", "client_id": "jarvis",
            "scopes": ["mail.read"]}});
        let url = Url::parse(&Flows::default().start(&manifest, 8765, None).unwrap()).unwrap();
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:8765");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["prompt"], "consent");
    }
}
