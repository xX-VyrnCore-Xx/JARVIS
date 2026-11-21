//! Controllo aggiornamenti con i canali Stable e Beta (ADR-0034).
//!
//! - Il controllo periodico è attivo per impostazione e si può disattivare dalla sezione Aggiornamenti.
//! - Controlla ogni ora, più il controllo manuale. La richiesta va fuori dal blocco del Core e non invia
//!   dati del proprietario; per repository privati usa un token opzionale custodito cifrato dal Core.
//! - Stable considera solo le release stabili; Beta include anche le prerelease (`v1.3.0-rc.1`).
//! - Una versione più recente diventa una notifica `system` deduplicata per versione, con il collegamento alla
//!   release. Nessuna installazione automatica: la release si scarica e si verifica a mano (ADR-0020).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

pub const DEFAULT_URL: &str =
    "https://api.github.com/repos/xX-VyrnCore-Xx/JARVIS/releases?per_page=20";
pub const CHANNELS: [&str; 2] = ["stable", "beta"];
const EVERY: chrono::Duration = chrono::Duration::hours(1);

/// Versione semantica minima: numeri principali e un eventuale suffisso di prerelease.
fn parse(v: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (v, None),
    };
    let mut it = core.split('.').map(|x| x.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??, pre))
}

/// Vero se `candidate` è più recente di `current` (una prerelease precede la stabile con gli stessi numeri).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (parse(candidate), parse(current)) else {
        return false;
    };
    match (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (&a.3, &b.3) {
            (None, Some(_)) => true,
            (Some(x), Some(y)) => x > y,
            _ => false,
        },
    }
}

/// La release più recente del canale, fra quelle dell'API di GitHub (bozze escluse).
pub fn latest(releases: &[Value], channel: &str) -> Option<Value> {
    releases
        .iter()
        .filter(|r| r["draft"] != true)
        .filter(|r| channel == "beta" || r["prerelease"] != true)
        .filter(|r| r["tag_name"].as_str().and_then(parse).is_some())
        .max_by(|a, b| {
            let (x, y) = (a["tag_name"].as_str().unwrap_or_default(), b["tag_name"].as_str().unwrap_or_default());
            if is_newer(x, y) {
                std::cmp::Ordering::Greater
            } else if is_newer(y, x) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .map(|r| json!({"tag": r["tag_name"], "url": r["html_url"], "published_at": r["published_at"], "prerelease": r["prerelease"] == true}))
}

/// Impostazioni e ultimo esito, salvati in `<dati>/updates.json`.
pub struct Updates {
    path: PathBuf,
    pub url: String,
}

impl Updates {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("updates.json"),
            url: std::env::var("JARVIS_UPDATE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string()),
        }
    }

    pub fn state(&self) -> Value {
        let mut s: Value = std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        if !s["enabled"].is_boolean() {
            s["enabled"] = true.into();
        }
        if !s["channel"].as_str().is_some_and(|c| CHANNELS.contains(&c)) {
            s["channel"] = "stable".into();
        }
        s["current"] = env!("CARGO_PKG_VERSION").into();
        s
    }

    fn save(&self, s: &Value) -> Result<(), String> {
        let mut s = s.clone();
        if let Some(o) = s.as_object_mut() {
            o.remove("current");
        }
        crate::backup::atomic_write(&self.path, s.to_string().as_bytes()).map_err(|e| e.to_string())
    }

    pub fn configure(&self, enabled: bool, channel: &str) -> Result<Value, String> {
        if !CHANNELS.contains(&channel) {
            return Err(format!("canale sconosciuto: {channel} (stable o beta)"));
        }
        let mut s = self.state();
        s["enabled"] = enabled.into();
        if s["channel"] != channel {
            s["channel"] = channel.into();
            for key in ["latest", "error", "checked_at", "update_available"] {
                s.as_object_mut()
                    .expect("stato degli aggiornamenti")
                    .remove(key);
            }
        }
        self.save(&s)?;
        Ok(self.state())
    }

    pub fn reset_check(&self) -> Result<(), String> {
        let mut state = self.state();
        if let Some(object) = state.as_object_mut() {
            for key in ["checked_at", "error", "latest", "update_available"] {
                object.remove(key);
            }
        }
        self.save(&state)
    }

    /// Vero se il controllo automatico è attivo e l'ultimo è più vecchio di un'ora.
    pub fn due(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        let s = self.state();
        s["enabled"] == true
            && s["checked_at"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .is_none_or(|t| now - t.with_timezone(&chrono::Utc) >= EVERY)
    }

    /// Scarica l'elenco delle release (da chiamare fuori dal blocco del Core).
    pub fn fetch(&self) -> Result<Vec<Value>, String> {
        self.fetch_with_token(None)
    }

    /// Scarica le release; un token opzionale consente l'accesso a repository privati.
    pub fn fetch_with_token(&self, token: Option<&str>) -> Result<Vec<Value>, String> {
        if token.is_some() {
            validate_private_endpoint(&self.url)?;
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .redirects(0)
            .user_agent(concat!("JARVIS/", env!("CARGO_PKG_VERSION")))
            .build();
        let mut request = agent
            .get(&self.url)
            .set("Accept", "application/vnd.github+json");
        if let Some(token) = token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        let body = request
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => {
                    format!("il servizio delle release ha risposto HTTP {code}")
                }
                ureq::Error::Transport(t) => {
                    format!("servizio delle release non raggiungibile ({})", t.kind())
                }
            })?
            .into_string()
            .map_err(|e| e.to_string())?;
        serde_json::from_str::<Vec<Value>>(&body)
            .map_err(|_| "elenco delle release non leggibile".to_string())
    }

    /// Registra l'esito di un controllo; restituisce la release più recente se è un aggiornamento.
    pub fn record(
        &self,
        result: Result<Vec<Value>, String>,
    ) -> Result<(Value, Option<Value>), String> {
        let mut s = self.state();
        s["checked_at"] = crate::now().into();
        let newer = match result {
            Ok(releases) => {
                let latest = latest(&releases, s["channel"].as_str().unwrap_or("stable"));
                s["latest"] = latest.clone().unwrap_or(Value::Null);
                s["error"] = Value::Null;
                latest.filter(|l| {
                    is_newer(
                        l["tag"].as_str().unwrap_or_default(),
                        env!("CARGO_PKG_VERSION"),
                    )
                })
            }
            Err(e) => {
                s["error"] = e.into();
                None
            }
        };
        s["update_available"] = newer.is_some().into();
        self.save(&s)?;
        Ok((self.state(), newer))
    }
}

fn validate_private_endpoint(value: &str) -> Result<(), String> {
    let url =
        url::Url::parse(value).map_err(|_| "URL delle release GitHub non valido".to_string())?;
    let host = url.host_str().unwrap_or_default();
    let github_api = url.scheme() == "https" && host == "api.github.com";
    #[cfg(test)]
    let local_test_api = url.scheme() == "http"
        && url.host().is_some_and(|host| {
            matches!(host, url::Host::Ipv4(ip) if ip.is_loopback())
                || matches!(host, url::Host::Ipv6(ip) if ip.is_loopback())
        });
    #[cfg(not(test))]
    let local_test_api = false;
    let is_release_endpoint = url.path_segments().is_some_and(|mut segments| {
        matches!(segments.next(), Some("repos"))
            && segments.next().is_some_and(|owner| !owner.is_empty())
            && segments.next().is_some_and(|repo| !repo.is_empty())
            && segments.next() == Some("releases")
            && segments.next().is_none()
    });
    if (!github_api && !local_test_api)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !is_release_endpoint
    {
        return Err(
            "il token GitHub viene inviato solo all'API HTTPS di GitHub per endpoint releases"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_channels() {
        assert!(is_newer("v1.3.0", "1.2.0"));
        assert!(!is_newer("v1.2.0", "1.2.0"));
        assert!(
            is_newer("v1.2.0", "1.2.0-rc.1"),
            "la stabile segue la prerelease"
        );
        assert!(!is_newer("v1.3.0-rc.1", "1.3.0"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(!is_newer("nonsense", "1.0.0"));
        let releases = vec![
            json!({"tag_name": "v1.2.0", "prerelease": false, "html_url": "https://x/1.2.0"}),
            json!({"tag_name": "v1.3.0-rc.1", "prerelease": true, "html_url": "https://x/rc"}),
            json!({"tag_name": "v1.4.0", "draft": true}),
        ];
        assert_eq!(latest(&releases, "stable").unwrap()["tag"], "v1.2.0");
        assert_eq!(
            latest(&releases, "beta").unwrap()["tag"],
            "v1.3.0-rc.1",
            "beta include le prerelease"
        );
    }

    #[test]
    fn settings_and_schedule() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-upd-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let u = Updates::new(&dir);
        assert_eq!(
            (
                u.state()["enabled"].as_bool(),
                u.state()["channel"].as_str()
            ),
            (Some(true), Some("stable"))
        );
        assert!(u.due(chrono::Utc::now()), "controllo automatico attivo");
        assert!(u.configure(true, "nightly").is_err());
        u.configure(true, "beta").unwrap();
        assert!(u.due(chrono::Utc::now()));
        let (state, newer) = u
            .record(Ok(vec![
                json!({"tag_name": "v99.0.0", "prerelease": false, "html_url": "https://x/99"}),
            ]))
            .unwrap();
        assert_eq!(newer.unwrap()["tag"], "v99.0.0");
        assert_eq!(state["update_available"], true);
        assert!(!u.due(chrono::Utc::now()), "un controllo all'ora");
        assert!(u.due(chrono::Utc::now() + chrono::Duration::hours(2)));
        let (state, newer) = u.record(Err("servizio non raggiungibile".into())).unwrap();
        assert!(newer.is_none() && state["error"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_repository_fetch_uses_a_bearer_token() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]")
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        let dir = std::env::temp_dir().join(format!(
            "jarvis-upd-private-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut updates = Updates::new(&dir);
        updates.url = format!("http://{address}/repos/owner/repo/releases");
        validate_private_endpoint(&updates.url).unwrap();
        assert!(updates
            .fetch_with_token(Some("synthetic-private-token"))
            .unwrap()
            .is_empty());
        let request = server.join().unwrap();
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-private-token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_repository_tokens_are_limited_to_github_release_endpoints() {
        for endpoint in [
            "http://api.github.com/repos/owner/repo/releases",
            "https://example.test/repos/owner/repo/releases",
            "https://api.github.com/user",
            "https://user:password@api.github.com/repos/owner/repo/releases",
        ] {
            assert!(
                validate_private_endpoint(endpoint).is_err(),
                "rejected endpoint {endpoint}"
            );
        }
        validate_private_endpoint(
            "https://api.github.com/repos/xX-VyrnCore-Xx/JARVIS/releases?per_page=20",
        )
        .unwrap();
    }
}
