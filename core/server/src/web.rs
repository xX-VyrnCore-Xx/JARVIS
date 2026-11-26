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
    Wikipedia {
        base: String,
    },
    Searxng {
        base: String,
    },
    /// Ricerca generale senza chiave (DuckDuckGo, solo risultati in HTML) con ripiego su Wikipedia (ADR-0196).
    Web {
        duckduckgo: String,
        wikipedia: String,
    },
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
            (Some("wikipedia"), _) => Provider::Wikipedia {
                base: var("JARVIS_WIKIPEDIA_URL")
                    .unwrap_or_else(|| format!("https://{lang}.wikipedia.org"))
                    .trim_end_matches('/')
                    .into(),
            },
            _ => Provider::Web {
                duckduckgo: var("JARVIS_DUCKDUCKGO_URL")
                    .unwrap_or_else(|| "https://html.duckduckgo.com".into())
                    .trim_end_matches('/')
                    .into(),
                wikipedia: var("JARVIS_WIKIPEDIA_URL")
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
            Provider::Web { duckduckgo, .. } => ("duckduckgo+wikipedia", duckduckgo),
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

pub(crate) fn resolve_checked(
    url: &Url,
    allow_private: bool,
) -> Result<Vec<SocketAddr>, ToolError> {
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
            .tls_config(crate::net::tls_config())
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
        .tls_config(crate::net::tls_config())
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
        // JSON salvo che il chiamante dichiari un altro tipo (es. i moduli dell'endpoint OAuth).
        Some(b)
            if headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type")) =>
        {
            req.send_bytes(b)
        }
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

/// Scarica un indirizzo fisso e fidato (servizi di mappe) rispettando i controlli di rete del Web: restituisce il corpo.
pub fn get_trusted(cfg: &WebConfig, url: Url) -> Result<Vec<u8>, ToolError> {
    if !cfg.enabled {
        return Err(err("web.disabled", "accesso al Web disattivato"));
    }
    get(cfg, url, true).map(|page| page.body)
}

/// Ricerca tramite il fornitore configurato. Con il fornitore predefinito (`Web`) si usa DuckDuckGo e, se non
/// risponde, è bloccato o non trova nulla, Wikipedia: il risultato dice quale fonte ha risposto e perché.
pub fn search(cfg: &WebConfig, query: &str, limit: usize) -> Result<Value, ToolError> {
    let limit = limit.clamp(1, 10);
    match &cfg.provider {
        Provider::Wikipedia { base } => Ok(finish(
            "wikipedia",
            query,
            wikipedia_results(cfg, base, query, limit)?,
            limit,
        )),
        Provider::Searxng { base } => Ok(finish(
            "searxng",
            query,
            searxng_results(cfg, base, query)?,
            limit,
        )),
        Provider::Web {
            duckduckgo,
            wikipedia,
        } => {
            let reason = match duckduckgo_results(cfg, duckduckgo, query, limit) {
                Ok(results) if !results.is_empty() => {
                    return Ok(finish("duckduckgo", query, results, limit))
                }
                Ok(_) => "nessun risultato".to_string(),
                Err(e) => e.message,
            };
            let mut out = finish(
                "wikipedia",
                query,
                wikipedia_results(cfg, wikipedia, query, limit)?,
                limit,
            );
            out["fallback_from"] = json!("duckduckgo");
            out["fallback_reason"] = json!(reason.chars().take(160).collect::<String>());
            Ok(out)
        }
    }
}

/// Valutazione euristica dell'attendibilità di una fonte dal solo dominio (ADR-0201): non è un giudizio sul
/// contenuto e va letta come un indizio. Livelli: `alta` (istituzioni, enti scientifici, documentazione ufficiale,
/// agenzie di stampa), `media` (predefinito) e `bassa` (social, forum e contenuti generati dagli utenti).
pub fn assess_source(url: &str) -> Value {
    let Some(parsed) = Url::parse(url).ok() else {
        return json!({"level": "media", "score": 50, "reason": "indirizzo non analizzabile"});
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let on = |domains: &[&str]| domains.iter().any(|d| host_matches(&host, d));
    let (level, mut score, reason): (&str, i32, &str) = if on(&[
        "reddit.com",
        "quora.com",
        "facebook.com",
        "x.com",
        "twitter.com",
        "tiktok.com",
        "instagram.com",
        "pinterest.com",
        "answers.yahoo.com",
        "brainly.com",
        "blogspot.com",
        "wordpress.com",
        "medium.com",
        "tumblr.com",
        "telegram.me",
        "t.me",
        "youtube.com",
        "linkedin.com",
    ]) {
        (
            "bassa",
            25,
            "contenuto generato dagli utenti o social: verifica con una fonte primaria",
        )
    } else if host.ends_with(".gov")
        || host.contains(".gov.")
        || host.ends_with(".edu")
        || host.contains(".edu.")
        || host.contains(".ac.")
        || host.ends_with(".mil")
        || on(&[
            "europa.eu",
            "who.int",
            "un.org",
            "nih.gov",
            "nasa.gov",
            "arxiv.org",
            "nature.com",
            "science.org",
            "sciencedirect.com",
            "pubmed.ncbi.nlm.nih.gov",
            "ieee.org",
            "acm.org",
            "iso.org",
            "w3.org",
            "ietf.org",
            "istat.it",
            "gazzettaufficiale.it",
            "inps.it",
        ])
    {
        ("alta", 90, "istituzione, ente scientifico o normativo")
    } else if on(&[
        "docs.python.org",
        "developer.mozilla.org",
        "docs.rs",
        "rust-lang.org",
        "kernel.org",
        "postgresql.org",
        "developer.apple.com",
        "learn.microsoft.com",
        "docs.docker.com",
        "kubernetes.io",
        "github.com",
        "huggingface.co",
        "tailscale.com",
        "nvidia.com",
    ]) {
        ("alta", 80, "documentazione o progetto ufficiale")
    } else if on(&[
        "reuters.com",
        "apnews.com",
        "bbc.com",
        "bbc.co.uk",
        "ansa.it",
        "afp.com",
        "ilsole24ore.com",
    ]) {
        ("alta", 75, "agenzia o testata giornalistica consolidata")
    } else if on(&["wikipedia.org", "britannica.com", "treccani.it"]) {
        (
            "media",
            65,
            "enciclopedia: buona sintesi, da verificare sulle fonti citate",
        )
    } else {
        ("media", 50, "fonte non classificata")
    };
    if parsed.scheme() != "https" {
        score -= 15;
    }
    json!({"level": level, "score": score.clamp(0, 100), "reason": reason,
        "https": parsed.scheme() == "https", "domain": host})
}

/// Finestra di freschezza chiesta dalla domanda («oggi», «ultime notizie», «questa settimana»…): `d`, `w`, `m`.
pub fn recency_window(query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| q.contains(w));
    if has(&[
        "oggi",
        "adesso",
        "in questo momento",
        "ultime ore",
        "breaking",
        "in diretta",
    ]) {
        Some("d")
    } else if has(&[
        "ultime notizie",
        "ultimi giorni",
        "questa settimana",
        "ultima settimana",
        "recenti",
    ]) {
        Some("w")
    } else if has(&["questo mese", "ultimo mese", "ultimi mesi"]) {
        Some("m")
    } else {
        None
    }
}

fn finish(provider: &str, query: &str, results: Vec<Value>, limit: usize) -> Value {
    // Ogni risultato porta la sua valutazione; le fonti a bassa attendibilità vanno in coda, senza essere nascoste
    // e senza cambiare l'ordine degli altri.
    let mut results: Vec<Value> = results
        .into_iter()
        .map(|mut r| {
            let assessment = assess_source(r["url"].as_str().unwrap_or_default());
            r["reliability"] = assessment;
            r
        })
        .collect();
    results.sort_by_key(|r| r["reliability"]["level"] == "bassa");
    let count = |level: &str| {
        results
            .iter()
            .filter(|r| r["reliability"]["level"] == level)
            .count()
    };
    let summary = json!({"alta": count("alta"), "media": count("media"), "bassa": count("bassa")});
    let results: Vec<Value> = results
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, mut r)| {
            r["rank"] = (i + 1).into();
            r
        })
        .collect();
    let mut out = json!({"provider": provider, "query": query, "retrieved_at": now(), "results": results,
        "untrusted": true, "reliability_summary": summary,
        "reliability_note": "Valutazione euristica dal solo dominio: è un indizio, non un giudizio sul contenuto."});
    if let Some(window) = recency_window(query) {
        out["freshness"] = json!({"requested": window,
            "note": "La domanda chiede notizie recenti: i risultati sono limitati al periodo richiesto se il motore lo consente; controlla la data nella pagina."});
    }
    out
}

fn wikipedia_results(
    cfg: &WebConfig,
    base: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, ToolError> {
    let mut u = Url::parse(&format!("{base}/w/api.php"))
        .map_err(|_| err("web.invalid_url", "fornitore non valido"))?;
    u.query_pairs_mut()
        .append_pair("action", "query")
        .append_pair("list", "search")
        .append_pair("srsearch", query)
        .append_pair("srlimit", &limit.to_string())
        .append_pair("format", "json")
        .append_pair("utf8", "1");
    let page = get(cfg, u, true)?;
    let data: Value = serde_json::from_slice(&page.body)
        .map_err(|_| err("web.invalid_response", "risposta del fornitore non JSON"))?;
    Ok(data["query"]["search"]
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
        .unwrap_or_default())
}

fn searxng_results(cfg: &WebConfig, base: &str, query: &str) -> Result<Vec<Value>, ToolError> {
    let mut u = Url::parse(&format!("{base}/search"))
        .map_err(|_| err("web.invalid_url", "fornitore non valido"))?;
    u.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json");
    let page = get(cfg, u, true)?;
    let data: Value = serde_json::from_slice(&page.body)
        .map_err(|_| err("web.invalid_response", "risposta del fornitore non JSON"))?;
    Ok(data["results"]
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
        .unwrap_or_default())
}

fn duckduckgo_results(
    cfg: &WebConfig,
    base: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, ToolError> {
    let mut u = Url::parse(&format!("{base}/html/"))
        .map_err(|_| err("web.invalid_url", "fornitore non valido"))?;
    u.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("kl", "it-it");
    if let Some(window) = recency_window(query) {
        u.query_pairs_mut().append_pair("df", window);
    }
    let page = get(cfg, u, true)?;
    let html = String::from_utf8_lossy(&page.body);
    if html.contains("anomaly-modal") || html.contains("g-recaptcha") {
        return Err(err(
            "web.blocked",
            "DuckDuckGo chiede una verifica: la ricerca automatica è bloccata",
        ));
    }
    Ok(parse_duckduckgo(&html, limit))
}

/// Risultati dalla pagina HTML di DuckDuckGo: titolo, indirizzo (decodificato dal reindirizzamento `uddg`) e testo.
/// Gli annunci e i risultati senza indirizzo http(s) pubblico sono scartati.
pub fn parse_duckduckgo(html: &str, limit: usize) -> Vec<Value> {
    let starts: Vec<usize> = html.match_indices("result__a").map(|(i, _)| i).collect();
    let mut found: Vec<Value> = Vec::new();
    let mut previous_end = 0;
    for (n, &at) in starts.iter().enumerate() {
        let Some(open) = html[..at].rfind("<a ") else {
            continue;
        };
        if html[open..at].contains('>') {
            continue;
        }
        let Some(tag_close) = html[at..].find('>').map(|i| at + i) else {
            continue;
        };
        let tag = &html[open..=tag_close];
        let Some(inner_end) = html[tag_close..].find("</a>").map(|i| tag_close + i) else {
            continue;
        };
        let next = starts.get(n + 1).copied().unwrap_or(html.len());
        // Gli annunci hanno un contenitore «result--ad» o un indirizzo di tracciamento.
        let block = &html[previous_end.min(open)..open];
        previous_end = inner_end;
        if block.contains("result--ad") || tag.contains("/y.js") || tag.contains("ad_domain") {
            continue;
        }
        let Some(href) = tag
            .split_once("href=\"")
            .and_then(|(_, r)| r.split('"').next())
        else {
            continue;
        };
        let href = decode_entities(href);
        let absolute = if let Some(rest) = href.strip_prefix("//") {
            format!("https://{rest}")
        } else {
            href
        };
        let Ok(parsed) = Url::parse(&absolute) else {
            continue;
        };
        let target = if parsed
            .host_str()
            .is_some_and(|h| h.ends_with("duckduckgo.com"))
        {
            match parsed.query_pairs().find(|(k, _)| k == "uddg") {
                Some((_, v)) => v.into_owned(),
                None => continue,
            }
        } else {
            absolute
        };
        let Some(target_url) = Url::parse(&target)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
        else {
            continue;
        };
        let title = html_to_text(&html[tag_close + 1..inner_end]).1;
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let snippet = html[inner_end..next]
            .find("result__snippet")
            .map(|i| &html[inner_end + i..next])
            .and_then(|s| {
                let from = s.find('>')? + 1;
                let to = s[from..]
                    .find("</a>")
                    .into_iter()
                    .chain(s[from..].find("</td>"))
                    .min()?;
                Some(html_to_text(&s[from..from + to]).1)
            })
            .map(|t| {
                t.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(400)
                    .collect::<String>()
            })
            .unwrap_or_default();
        if title.is_empty() || found.iter().any(|r| r["url"] == target_url.as_str()) {
            continue;
        }
        found.push(json!({"title": title, "url": target_url.as_str(), "snippet": snippet}));
        if found.len() >= limit {
            break;
        }
    }
    found
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

    const DDG: &str = r#"
<div class="result results_links result--ad"><a rel="nofollow" class="result__a" href="//duckduckgo.com/y.js?ad_domain=ads.example&amp;u=1">Annuncio</a></div>
<div class="result results_links_deep"><h2 class="result__title">
 <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2Fit%2F&amp;rut=abc">Rust <b>Programming</b> Language</a></h2>
 <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2Fit%2F">Un linguaggio <b>veloce</b> &amp; sicuro.</a></div>
<div class="result"><h2><a class="result__a" href="https://doc.rust-lang.org/book/">The Book</a></h2>
 <a class="result__snippet">Il libro ufficiale.</a></div>
<div class="result"><h2><a class="result__a" href="//duckduckgo.com/l/?uddg=javascript%3Aalert(1)">Pericoloso</a></h2></div>
<div class="result"><h2><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2Fit%2F">Doppione</a></h2></div>
"#;

    #[test]
    fn duckduckgo_results_are_decoded_without_ads_unsafe_links_or_duplicates() {
        let results = parse_duckduckgo(DDG, 10);
        let urls: Vec<&str> = results.iter().filter_map(|r| r["url"].as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://www.rust-lang.org/it/",
                "https://doc.rust-lang.org/book/"
            ]
        );
        assert_eq!(results[0]["title"], "Rust Programming Language");
        assert_eq!(results[0]["snippet"], "Un linguaggio veloce & sicuro.");
        assert_eq!(results[1]["snippet"], "Il libro ufficiale.");
        assert_eq!(parse_duckduckgo(DDG, 1).len(), 1);
        assert!(parse_duckduckgo("<html>niente</html>", 5).is_empty());
    }

    #[test]
    fn sources_are_rated_by_domain_and_weak_ones_go_last() {
        let level = |u: &str| assess_source(u)["level"].as_str().unwrap().to_string();
        assert_eq!(level("https://www.istat.it/dati"), "alta");
        assert_eq!(level("https://comune.milano.gov.it/x"), "alta");
        assert_eq!(level("https://docs.python.org/3/"), "alta");
        assert_eq!(level("https://www.reddit.com/r/rust"), "bassa");
        assert_eq!(level("https://it.wikipedia.org/wiki/Rust"), "media");
        assert_eq!(level("https://sito-sconosciuto.example/pagina"), "media");
        assert_eq!(level("non un indirizzo"), "media");
        let plain = assess_source("http://sito-sconosciuto.example/");
        assert!(
            plain["score"].as_i64().unwrap()
                < assess_source("https://sito-sconosciuto.example/")["score"]
                    .as_i64()
                    .unwrap()
        );
        let out = finish(
            "test",
            "query",
            vec![
                json!({"title": "forum", "url": "https://www.reddit.com/x"}),
                json!({"title": "ente", "url": "https://www.nih.gov/y"}),
                json!({"title": "blog", "url": "https://esempio.example/z"}),
            ],
            10,
        );
        let titles: Vec<&str> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["title"].as_str())
            .collect();
        assert_eq!(titles, ["ente", "blog", "forum"]);
        assert_eq!(out["results"][0]["rank"], 1);
        assert_eq!(
            out["reliability_summary"],
            json!({"alta": 1, "media": 1, "bassa": 1})
        );
        assert!(out.get("freshness").is_none());
    }

    #[test]
    fn recent_news_requests_set_a_freshness_window() {
        assert_eq!(recency_window("ultime notizie sul Core"), Some("w"));
        assert_eq!(recency_window("risultati di oggi"), Some("d"));
        assert_eq!(recency_window("novità di questo mese"), Some("m"));
        assert_eq!(recency_window("storia di Roma"), None);
        let out = finish("test", "meteo adesso", vec![], 5);
        assert_eq!(out["freshness"]["requested"], "d");
    }
}
