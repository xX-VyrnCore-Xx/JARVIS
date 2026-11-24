//! Notifiche sul telefono (RF-13, ADR-0026): canali esterni ntfy e Telegram.
//!
//! - Il proprietario sceglie canale, gravità minima e se includere il testo. Per impostazione si invia solo
//!   titolo e categoria, perché ogni messaggio esce dal computer verso un servizio esterno.
//! - Token e chiavi stanno nell'archivio dei segreti (portachiavi su macOS e Windows, ADR-0021); l'API non li
//!   restituisce mai.
//! - L'invio avviene in un thread separato, fuori dal blocco del Core, con un tentativo di ripetizione. Esiti
//!   e errori finiscono nei contatori `notify.external.*` e nel log, senza contenuti.
//! - Le conferme non si danno dal telefono tramite questi canali: il messaggio porta solo a JARVIS.

use std::time::Duration;

use serde_json::{json, Value};

use crate::observability::Observability;

pub const KINDS: [&str; 2] = ["ntfy", "telegram"];
pub const SEVERITIES: [&str; 4] = ["success", "info", "warning", "error"];
const TELEGRAM_API: &str = "https://api.telegram.org";
const NTFY_SERVER: &str = "https://ntfy.sh";

fn rank(severity: &str) -> usize {
    SEVERITIES.iter().position(|s| *s == severity).unwrap_or(1)
}

/// Valida e normalizza la configurazione di un canale ricevuta dall'interfaccia.
pub fn normalize(input: &Value) -> Result<Value, String> {
    let kind = input["kind"].as_str().unwrap_or_default();
    if !KINDS.contains(&kind) {
        return Err(format!(
            "tipo di canale sconosciuto: «{kind}» (ntfy o telegram)"
        ));
    }
    let target = input["target"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let valid_target = match kind {
        // Argomento ntfy: lettere, cifre, trattini e trattini bassi; lungo per non essere indovinato.
        "ntfy" => {
            target.len() >= 8
                && target.len() <= 64
                && target
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }
        // Chat Telegram: identificativo numerico (anche negativo per i gruppi).
        _ => {
            !target.is_empty()
                && target.len() <= 32
                && target
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit())
        }
    };
    if !valid_target {
        return Err(match kind {
            "ntfy" => "argomento ntfy non valido: 8–64 caratteri tra lettere, cifre, - e _".into(),
            _ => "chat Telegram non valida: serve l'identificativo numerico della chat".into(),
        });
    }
    let default_server = if kind == "ntfy" {
        NTFY_SERVER
    } else {
        TELEGRAM_API
    };
    let server = input["server"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_server)
        .trim_end_matches('/')
        .to_string();
    let parsed =
        url::Url::parse(&server).map_err(|_| "indirizzo del server non valido".to_string())?;
    let loopback = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        return Err("il server deve usare HTTPS (HTTP solo su questo computer)".into());
    }
    let min = input["min_severity"].as_str().unwrap_or("warning");
    if !SEVERITIES.contains(&min) {
        return Err("gravità minima non valida".into());
    }
    let name: String = input["name"]
        .as_str()
        .unwrap_or(kind)
        .trim()
        .chars()
        .take(60)
        .collect();
    Ok(json!({
        "kind": kind,
        "name": if name.is_empty() { kind.to_string() } else { name },
        "server": server,
        "target": target,
        "min_severity": min,
        "include_body": input["include_body"].as_bool().unwrap_or(false),
        "enabled": true,
    }))
}

/// Vero se la notifica va inviata sul canale.
pub fn matches(channel: &Value, notification: &Value) -> bool {
    channel["enabled"] != false
        && rank(notification["severity"].as_str().unwrap_or("info"))
            >= rank(channel["min_severity"].as_str().unwrap_or("warning"))
}

/// Testo inviato: titolo e categoria, più il corpo solo se il proprietario lo ha scelto.
fn text_of(channel: &Value, n: &Value) -> (String, String) {
    let title = n["title"]
        .as_str()
        .unwrap_or("Notifica")
        .chars()
        .take(200)
        .collect::<String>();
    let category = n["category"].as_str().unwrap_or("system");
    let body = if channel["include_body"] == true {
        n["body"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(1000)
            .collect::<String>()
    } else {
        String::new()
    };
    let body = if body.is_empty() {
        format!("Categoria: {category}. Apri JARVIS per i dettagli.")
    } else {
        body
    };
    (title, body)
}

/// Invio sincrono a un canale. `secret` è il token (bot Telegram, obbligatorio) o l'accesso ntfy (facoltativo).
pub fn send(
    channel: &Value,
    secret: Option<&str>,
    n: &Value,
    public_url: Option<&str>,
) -> Result<(), String> {
    let (title, body) = text_of(channel, n);
    let server = channel["server"].as_str().unwrap_or_default();
    let target = channel["target"].as_str().unwrap_or_default();
    let agent = crate::net::builder(Duration::from_secs(10)).build();
    let result = match channel["kind"].as_str() {
        Some("ntfy") => {
            let priority = match n["severity"].as_str() {
                Some("error") => 5,
                Some("warning") => 4,
                Some("success") => 2,
                _ => 3,
            };
            // Pubblicazione JSON: titolo e testo in UTF-8, che le intestazioni HTTP non ammettono.
            let mut msg = json!({
                "topic": target,
                "title": format!("JARVIS · {title}"),
                "message": body,
                "priority": priority,
                "tags": [n["category"].as_str().unwrap_or("system")],
            });
            if let Some(url) = public_url {
                msg["click"] = url.into();
            }
            let mut req = agent
                .post(&format!("{server}/"))
                .set("Content-Type", "application/json");
            if let Some(token) = secret.filter(|t| !t.is_empty()) {
                req = req.set("Authorization", &format!("Bearer {token}"));
            }
            req.send_string(&msg.to_string())
        }
        Some("telegram") => {
            let token = secret
                .filter(|t| !t.is_empty())
                .ok_or("token del bot Telegram mancante")?;
            let mut text = format!("JARVIS · {title}\n{body}");
            if let Some(url) = public_url {
                text.push_str(&format!("\n{url}"));
            }
            agent
                .post(&format!("{server}/bot{token}/sendMessage"))
                .set("Content-Type", "application/json")
                .send_string(
                    &json!({"chat_id": target, "text": text, "disable_web_page_preview": true})
                        .to_string(),
                )
        }
        _ => return Err("tipo di canale sconosciuto".into()),
    };
    match result {
        Ok(_) => Ok(()),
        // Il messaggio d'errore non deve contenere l'URL, che per Telegram include il token.
        Err(ureq::Error::Status(code, _)) => Err(format!("il servizio ha risposto HTTP {code}")),
        Err(ureq::Error::Transport(t)) => Err(format!("servizio non raggiungibile ({})", t.kind())),
    }
}

/// Invio in background con un nuovo tentativo dopo 2 secondi; esiti nei contatori, senza contenuti.
pub fn dispatch(
    targets: Vec<(Value, Option<String>)>,
    n: Value,
    obs: std::sync::Arc<Observability>,
) {
    if targets.is_empty() {
        return;
    }
    let public_url = std::env::var("JARVIS_PUBLIC_URL")
        .ok()
        .filter(|u| u.starts_with("https://"));
    std::thread::spawn(move || {
        for (channel, secret) in targets {
            let mut outcome = send(&channel, secret.as_deref(), &n, public_url.as_deref());
            if outcome.is_err() {
                std::thread::sleep(Duration::from_secs(2));
                outcome = send(&channel, secret.as_deref(), &n, public_url.as_deref());
            }
            match outcome {
                Ok(()) => obs.count("notify.external.sent"),
                Err(e) => {
                    obs.count("notify.external.failed");
                    obs.log("warn", "notify.external.failed", json!({"channel_id": channel["channel_id"], "kind": channel["kind"], "error": e}));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_rejects_unsafe_or_ambiguous_settings() {
        assert!(normalize(&json!({"kind": "ntfy", "target": "jarvis-casa-7f3k2"})).is_ok());
        assert!(
            normalize(&json!({"kind": "ntfy", "target": "corto"})).is_err(),
            "argomento indovinabile"
        );
        assert!(normalize(&json!({"kind": "ntfy", "target": "jarvis-casa-7f3k2", "server": "http://ntfy.example.com"})).is_err(), "HTTP remoto");
        assert!(normalize(
            &json!({"kind": "ntfy", "target": "jarvis-casa-7f3k2", "server": "http://127.0.0.1:9"})
        )
        .is_ok());
        assert!(normalize(&json!({"kind": "telegram", "target": "-100123"})).is_ok());
        assert!(normalize(&json!({"kind": "telegram", "target": "@utente"})).is_err());
        assert!(normalize(&json!({"kind": "sms", "target": "123"})).is_err());
        let c = normalize(&json!({"kind": "ntfy", "target": "jarvis-casa-7f3k2"})).unwrap();
        assert_eq!(
            (c["min_severity"].as_str(), c["include_body"].as_bool()),
            (Some("warning"), Some(false))
        );
    }

    #[test]
    fn severity_filter_and_private_text() {
        let c = normalize(&json!({"kind": "ntfy", "target": "jarvis-casa-7f3k2"})).unwrap();
        let n = json!({"severity": "warning", "category": "confirmation", "title": "Conferma richiesta", "body": "testo privato"});
        assert!(matches(&c, &n));
        assert!(!matches(&c, &json!({"severity": "info"})));
        let (_, body) = text_of(&c, &n);
        assert!(
            !body.contains("privato"),
            "senza consenso il corpo non esce dal computer"
        );
        let mut with_body = c.clone();
        with_body["include_body"] = true.into();
        assert_eq!(text_of(&with_body, &n).1, "testo privato");
    }
}
