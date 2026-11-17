//! Web Agent Engine del prototipo (RF-06): ricerca e lettura di pagine con provenienza verificabile.
//!
//! Difese:
//! - controllo lessicale dell'URL nella policy (schema, credenziali, domini consentiti o vietati, IP privati);
//! - risoluzione DNS verificata prima di ogni connessione, con connessione agli indirizzi controllati;
//!   lo stesso controllo si ripete a ogni redirect (al massimo 3), contro SSRF e DNS rebinding;
//! - limiti di tempo, dimensione (1 MiB) e tipo di contenuto;
//! - il contenuto è marcato `untrusted`: l'Orchestrator non esegue strumenti proposti dopo un risultato Web
//!   nello stesso turno.

use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use crate::tools::ToolError;

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_TEXT: usize = 20_000;
const MAX_REDIRECTS: usize = 3;

#[derive(Clone, Debug)]
pub enum Provider {
    Wikipedia { base: String },
    Searxng { base: String },
}

#[derive(Clone, Debug)]
pub struct WebConfig {
    pub enabled: bool,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    /// Consente indirizzi privati e di loopback (solo per test o reti fidate del proprietario).
    pub allow_private: bool,
    pub provider: Provider,
    pub timeout: Duration,
}

impl Default for WebConfig {
    /// Web attivo, fornitore Wikipedia in italiano, solo indirizzi pubblici.
    fn default() -> Self {
        Self {
            enabled: true,
            allow: vec![],
            deny: vec![],
            allow_private: false,
            provider: Provider::Wikipedia {
                base: "https://it.wikipedia.org".into(),
            },
            timeout: Duration::from_secs(15),
        }
    }
}

impl WebConfig {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let list = |k: &str| {
            var(k)
                .map(|v| {
                    v.split(',')
                        .map(|d| d.trim().trim_start_matches("*.").to_ascii_lowercase())
                        .filter(|d| !d.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let lang = var("JARVIS_SEARCH_LANG").unwrap_or_else(|| "it".into());
        let provider = match (
            var("JARVIS_SEARCH_PROVIDER").as_deref(),
            var("JARVIS_SEARXNG_URL"),
        ) {
            (Some("searxng"), Some(url)) | (None, Some(url)) => Provider::Searxng {
                base: url.trim_end_matches('/').into(),
            },
            _ => Provider::Wikipedia {
                base: var("JARVIS_WIKIPEDIA_URL")
                    .unwrap_or_else(|| format!("https://{lang}.wikipedia.org"))
                    .trim_end_matches('/')
                    .into(),
            },
        };
        Self {
            enabled: var("JARVIS_WEB").is_none_or(|v| !matches!(v.as_str(), "off" | "0" | "false")),
            allow: list("JARVIS_WEB_ALLOW"),
            deny: list("JARVIS_WEB_DENY"),
            allow_private: var("JARVIS_WEB_ALLOW_PRIVATE").is_some_and(|v| v == "1" || v == "true"),
            provider,
            timeout: Duration::from_secs(
                var("JARVIS_WEB_TIMEOUT_S")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(15),
            ),
        }
    }

    pub fn describe(&self) -> Value {
        let (name, base) = match &self.provider {
            Provider::Wikipedia { base } => ("wikipedia", base),
            Provider::Searxng { base } => ("searxng", base),
        };
        json!({"enabled": self.enabled, "provider": name, "provider_url": base, "allow": self.allow, "deny": self.deny})
    }
}

fn host_matches(host: &str, pattern: &str) -> bool {
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

/// Indirizzo instradabile su Internet pubblica.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..20).contains(&o[1]))
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || s[0] == 0x2001 && s[1] == 0x0db8)
        }
    }
}

/// Controllo lessicale (usato dalla policy): restituisce il motivo del diniego.
pub fn check_url(cfg: &WebConfig, raw: &str) -> Result<Url, &'static str> {
    let url = Url::parse(raw.trim()).map_err(|_| "policy.invalid_url")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("policy.url_scheme_not_allowed");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("policy.url_with_credentials");
    }
    let host = url
        .host_str()
        .ok_or("policy.invalid_url")?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if cfg.deny.iter().any(|d| host_matches(&host, d))
        || (!cfg.allow.is_empty() && !cfg.allow.iter().any(|d| host_matches(&host, d)))
    {
        return Err("policy.url_not_allowed");
    }
    if !cfg.allow_private {
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
        {
            return Err("policy.private_address");
        }
        if let Ok(ip) = bare.parse::<IpAddr>() {
            if !is_public_ip(ip) {
                return Err("policy.private_address");
            }
        }
    }
    Ok(url)
}

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn resolve_checked(url: &Url, allow_private: bool) -> Result<Vec<SocketAddr>, ToolError> {
    let host = url
        .host_str()
        .ok_or_else(|| err("web.invalid_url", "URL senza host"))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = (host.trim_start_matches('[').trim_end_matches(']'), port)
        .to_socket_addrs()
        .map_err(|e| {
            err(
                "web.unreachable",
                format!("risoluzione di {host} non riuscita: {e}"),
            )
        })?
        .collect();
    if addrs.is_empty() {
        return Err(err(
            "web.unreachable",
            format!("nessun indirizzo per {host}"),
        ));
    }
    if !allow_private && addrs.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(err(
            "web.private_address",
            format!("{host} risolve verso un indirizzo privato o locale"),
        ));
    }
    Ok(addrs)
}

struct Fetched {
    final_url: Url,
    content_type: String,
    body: Vec<u8>,
    truncated: bool,
}

/// GET con controlli a ogni salto. `trusted_endpoint` esenta dal controllo lessicale i fornitori configurati dal proprietario.
fn get(cfg: &WebConfig, start: Url, trusted_endpoint: bool) -> Result<Fetched, ToolError> {
    let mut url = start;
    for hop in 0..=MAX_REDIRECTS {
        if !trusted_endpoint || hop > 0 {
            check_url(cfg, url.as_str())
                .map_err(|reason| err("web.blocked", format!("{} ({reason})", url.as_str())))?;
        }
        let addrs = resolve_checked(&url, cfg.allow_private || (trusted_endpoint && hop == 0))?;
        let pinned = addrs.clone();
        let agent = ureq::AgentBuilder::new()
            .timeout(cfg.timeout)
            .redirects(0)
            .user_agent(concat!(
                "JARVIS/",
                env!("CARGO_PKG_VERSION"),
                " (assistente personale; uso locale)"
            ))
            .resolver(move |_: &str| Ok(pinned.clone()))
            .build();
        let response = match agent
            .get(url.as_str())
            .set(
                "Accept",
                "text/html,text/plain,application/json;q=0.9,*/*;q=0.1",
            )
            .call()
        {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => {
                return Err(err(
                    "web.http_error",
                    format!("{} ha risposto {code}", url.as_str()),
                ))
            }
            Err(ureq::Error::Transport(t)) => {
                let timeout = t.to_string().to_ascii_lowercase().contains("timed out");
                return Err(err(
                    if timeout {
                        "web.timeout"
                    } else {
                        "web.unreachable"
                    },
                    format!("{}: {t}", url.as_str()),
                ));
            }
        };
        if (300..400).contains(&response.status()) {
            let location = response
                .header("location")
                .ok_or_else(|| err("web.http_error", "redirect senza Location"))?;
            url = url
                .join(location)
                .map_err(|_| err("web.invalid_url", "redirect non valido"))?;
            continue;
        }
        let content_type = response.content_type().to_ascii_lowercase();
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|e| err("web.unreachable", format!("lettura interrotta: {e}")))?;
        let truncated = body.len() as u64 > MAX_BYTES;
        body.truncate(MAX_BYTES as usize);
        return Ok(Fetched {
            final_url: url,
            content_type,
            body,
            truncated,
        });
    }
    Err(err(
        "web.too_many_redirects",
        format!("più di {MAX_REDIRECTS} redirect"),
    ))
}

/// Richiesta HTTP per i connettori: host vincolato a `allowed_hosts`, controlli SSRF, nessun redirect.
/// Restituisce stato, tipo di contenuto e corpo (al massimo 1 MiB).
pub fn request(
    cfg: &WebConfig,
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    allowed_hosts: &[String],
) -> Result<(u16, String, Vec<u8>), ToolError> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !allowed_hosts.iter().any(|h| h == &host) {
        return Err(err(
            "connector.host_not_allowed",
            format!("host {host} non dichiarato dal connettore"),
        ));
    }
    check_url(cfg, url.as_str()).map_err(|reason| {
        err(
            "connector.blocked",
            format!("indirizzo non ammesso ({reason})"),
        )
    })?;
    let pinned = resolve_checked(url, cfg.allow_private)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(cfg.timeout)
        .redirects(0)
        .user_agent(concat!("JARVIS/", env!("CARGO_PKG_VERSION")))
        .resolver(move |_: &str| Ok(pinned.clone()))
        .build();
    let mut req = agent.request(method, url.as_str());
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let result = match body {
        Some(b) => req.set("Content-Type", "application/json").send_bytes(b),
        None => req.call(),
    };
    let response = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(t)) => {
            let timeout = t.to_string().to_ascii_lowercase().contains("timed out");
            return Err(err(
                if timeout {
                    "web.timeout"
                } else {
                    "web.unreachable"
                },
                format!("{host}: {t}"),
            ));
        }
    };
    let status = response.status();
    if (300..400).contains(&status) {
        return Err(err(
            "connector.redirect_refused",
            "i connettori non seguono redirect",
        ));
    }
    let ctype = response.content_type().to_ascii_lowercase();
    let mut data = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut data)
        .map_err(|e| err("web.unreachable", format!("lettura interrotta: {e}")))?;
    Ok((status, ctype, data))
}

fn now() -> String {
    crate::now()
}

/// Legge una pagina e ne restituisce testo e provenienza.
pub fn fetch(cfg: &WebConfig, raw: &str) -> Result<Value, ToolError> {
    let start = check_url(cfg, raw)
        .map_err(|reason| err("web.blocked", format!("URL non ammesso ({reason})")))?;
    let page = get(cfg, start, false)?;
    let ct = page.content_type.clone();
    if !(ct.starts_with("text/") || ct.contains("json") || ct.contains("xml") || ct.is_empty()) {
        return Err(err(
            "web.unsupported_content",
            format!("tipo di contenuto non testuale: {ct}"),
        ));
    }
    let raw_text = String::from_utf8_lossy(&page.body);
    let (title, text) = if ct.contains("html") || raw_text.trim_start().starts_with('<') {
        html_to_text(&raw_text)
    } else {
        (None, raw_text.into_owned())
    };
    let mut content: String = text.chars().take(MAX_TEXT).collect();
    let cut = text.chars().count() > MAX_TEXT;
    if cut {
        content.push('…');
    }
    Ok(json!({
        "url": raw.trim(),
        "final_url": page.final_url.as_str(),
        "title": title.unwrap_or_else(|| page.final_url.host_str().unwrap_or("").to_string()),
        "content": content,
        "content_type": ct,
        "bytes": page.body.len(),
        "truncated": page.truncated || cut,
        "retrieved_at": now(),
        "content_sha256": hex::encode(Sha256::digest(&page.body)),
        "untrusted": true,
    }))
}

/// Ricerca tramite il fornitore configurato.
pub fn search(cfg: &WebConfig, query: &str, limit: usize) -> Result<Value, ToolError> {
    let limit = limit.clamp(1, 10);
    let (provider, url) = match &cfg.provider {
        Provider::Wikipedia { base } => {
            let mut u = Url::parse(&format!("{base}/w/api.php"))
                .map_err(|_| err("web.invalid_url", "fornitore non valido"))?;
            u.query_pairs_mut()
                .append_pair("action", "query")
                .append_pair("list", "search")
                .append_pair("srsearch", query)
                .append_pair("srlimit", &limit.to_string())
                .append_pair("format", "json")
                .append_pair("utf8", "1");
            ("wikipedia", u)
        }
        Provider::Searxng { base } => {
            let mut u = Url::parse(&format!("{base}/search"))
                .map_err(|_| err("web.invalid_url", "fornitore non valido"))?;
            u.query_pairs_mut()
                .append_pair("q", query)
                .append_pair("format", "json");
            ("searxng", u)
        }
    };
    let page = get(cfg, url, true)?;
    let data: Value = serde_json::from_slice(&page.body)
        .map_err(|_| err("web.invalid_response", "risposta del fornitore non JSON"))?;
    let results: Vec<Value> = match &cfg.provider {
        Provider::Wikipedia { base } => data["query"]["search"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|r| {
                        let title = r["title"].as_str()?;
                        let mut page = Url::parse(&format!("{base}/wiki/")).ok()?;
                        page.path_segments_mut().ok()?.pop_if_empty().push(&title.replace(' ', "_"));
                        Some(json!({"title": title, "url": page.as_str(), "snippet": html_to_text(r["snippet"].as_str().unwrap_or("")).1}))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Provider::Searxng { .. } => data["results"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|r| {
                        let url = r["url"].as_str()?;
                        Url::parse(url).ok().filter(|u| matches!(u.scheme(), "http" | "https"))?;
                        Some(json!({"title": r["title"].as_str().unwrap_or(url), "url": url, "snippet": r["content"].as_str().unwrap_or("").chars().take(400).collect::<String>()}))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let results: Vec<Value> = results
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, mut r)| {
            r["rank"] = (i + 1).into();
            r
        })
        .collect();
    Ok(
        json!({"provider": provider, "query": query, "retrieved_at": now(), "results": results, "untrusted": true}),
    )
}

/// Estrazione semplice del testo da HTML: titolo e testo leggibile, senza script, stili e commenti.
pub fn html_to_text(html: &str) -> (Option<String>, String) {
    let lower = html.to_ascii_lowercase();
    let title = lower.find("<title").and_then(|start| {
        let open_end = lower[start..].find('>')? + start + 1;
        let close = lower[open_end..].find("</title>")? + open_end;
        Some(decode_entities(html[open_end..close].trim()))
    });
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < html.len() {
        if bytes[i] == b'<' {
            let rest = &lower[i..];
            if rest.starts_with("<!--") {
                i += rest.find("-->").map_or(rest.len(), |e| e + 3);
                continue;
            }
            let skip = ["script", "style", "noscript", "template", "svg", "head"]
                .iter()
                .find_map(|t| {
                    (rest.starts_with(&format!("<{t}"))
                        && rest[1 + t.len()..].starts_with(|c: char| c == '>' || c.is_whitespace()))
                    .then(|| {
                        rest.find(&format!("</{t}"))
                            .and_then(|e| rest[e..].find('>').map(|g| e + g + 1))
                            .unwrap_or(rest.len())
                    })
                });
            if let Some(len) = skip {
                i += len;
                continue;
            }
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            let tag = &rest[..end];
            if [
                "<br",
                "</p",
                "<p",
                "</div",
                "</li",
                "<li",
                "</h",
                "<h1",
                "<h2",
                "<h3",
                "<tr",
                "</tr",
                "</section",
                "</article",
            ]
            .iter()
            .any(|t| tag.starts_with(t))
            {
                out.push('\n');
            }
            i += end;
        } else {
            let next = lower[i..].find('<').map_or(html.len(), |n| i + n);
            out.push_str(&html[i..next]);
            i = next;
        }
    }
    let text = decode_entities(&out);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    (title.filter(|t| !t.is_empty()), lines.join("\n"))
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = tail[..tail.len().min(12)].find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> WebConfig {
        WebConfig {
            enabled: true,
            allow: vec![],
            deny: vec!["vietato.example".into()],
            allow_private: false,
            provider: Provider::Wikipedia {
                base: "https://it.wikipedia.org".into(),
            },
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn url_checks() {
        let c = cfg();
        assert!(check_url(&c, "https://it.wikipedia.org/wiki/Rust").is_ok());
        for (url, reason) in [
            ("ftp://example.org/x", "policy.url_scheme_not_allowed"),
            ("file:///etc/passwd", "policy.url_scheme_not_allowed"),
            (
                "https://utente:pw@example.org/",
                "policy.url_with_credentials",
            ),
            ("http://localhost:8765/api/memory", "policy.private_address"),
            ("http://127.0.0.1/", "policy.private_address"),
            ("http://[::1]/", "policy.private_address"),
            (
                "http://169.254.169.254/latest/meta-data",
                "policy.private_address",
            ),
            ("http://192.168.1.10/", "policy.private_address"),
            ("http://10.0.0.1/", "policy.private_address"),
            ("http://[::ffff:127.0.0.1]/", "policy.private_address"),
            ("https://sub.vietato.example/", "policy.url_not_allowed"),
            ("non un url", "policy.invalid_url"),
        ] {
            assert_eq!(check_url(&c, url).unwrap_err(), reason, "{url}");
        }
        let mut only = cfg();
        only.allow = vec!["wikipedia.org".into()];
        assert!(check_url(&only, "https://en.wikipedia.org/").is_ok());
        assert_eq!(
            check_url(&only, "https://example.org/").unwrap_err(),
            "policy.url_not_allowed"
        );
    }

    #[test]
    fn html_extraction() {
        let html = "<html><head><title>Prova &amp; titolo</title><style>p{}</style></head><body><script>alert('x')</script><!-- c --><h1>Titolo</h1><p>Uno&nbsp;due <b>tre</b></p><p>&#232; &#xE0;</p></body></html>";
        let (title, text) = html_to_text(html);
        assert_eq!(title.as_deref(), Some("Prova & titolo"));
        assert_eq!(text, "Titolo\nUno due tre\nè à");
    }

    #[test]
    fn public_ip_classification() {
        assert!(is_public_ip("93.184.215.14".parse().unwrap()));
        assert!(is_public_ip("2a00:1450::1".parse().unwrap()));
        for ip in ["100.64.0.1", "0.0.0.0", "fd00::1", "fe80::1", "224.0.0.1"] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
