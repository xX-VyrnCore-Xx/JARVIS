//! Navigazione Web interattiva (RF-06, ADR-0043) con il browser già installato sul computer.
//!
//! JARVIS avvia Chrome, Chromium o Edge in modalità headless con un profilo temporaneo (cancellato alla
//! fine) e lo pilota con il DevTools Protocol su `ws://127.0.0.1`. Ogni richiesta di rete del browser,
//! sotto-risorse comprese, passa dalla stessa policy di `web.fetch` (domini consentiti, indirizzi privati
//! vietati anche dopo la risoluzione DNS). Il contenuto restituito è un dato non attendibile.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::{Message, WebSocket};

use crate::tools::ToolError;
use crate::web::WebConfig;

/// Testo massimo restituito per pagina e numero massimo di collegamenti e campi.
const MAX_TEXT: usize = 20_000;
const MAX_LINKS: usize = 60;

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

/// Browser compatibile: `JARVIS_BROWSER`, poi i percorsi abituali di Chrome, Chromium ed Edge.
pub fn find_browser() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("JARVIS_BROWSER").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let candidates: Vec<PathBuf> = if cfg!(target_os = "macos") {
        [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else if cfg!(windows) {
        let mut v = Vec::new();
        for base in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Some(b) = std::env::var_os(base) {
                let b = PathBuf::from(b);
                v.push(b.join("Microsoft/Edge/Application/msedge.exe"));
                v.push(b.join("Google/Chrome/Application/chrome.exe"));
            }
        }
        v
    } else {
        let mut v: Vec<PathBuf> = [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
        ]
        .iter()
        .filter_map(|n| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|d| d.join(n))
                    .find(|p| p.is_file())
            })
        })
        .collect();
        v.push("/snap/bin/chromium".into());
        v
    };
    candidates.into_iter().find(|p| p.is_file())
}

/// Sessione del browser: processo, profilo temporaneo e connessione DevTools.
pub struct Browser {
    child: Child,
    profile: PathBuf,
    ws: WebSocket<TcpStream>,
    next_id: u64,
    session: String,
    web: WebConfig,
    loaded: bool,
    blocked: Vec<String>,
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

impl Browser {
    /// Avvia il browser headless con un profilo nuovo e apre una scheda vuota con la policy di rete attiva.
    pub fn launch(web: &WebConfig) -> Result<Self, ToolError> {
        let exe = find_browser().ok_or_else(|| {
            err(
                "web.browser_unavailable",
                "nessun browser compatibile: installa Chrome, Chromium o Edge, oppure imposta JARVIS_BROWSER",
            )
        })?;
        let profile =
            std::env::temp_dir().join(format!("jarvis-browser-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&profile)
            .map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        let child = Command::new(&exe)
            .args([
                "--headless=new",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-background-networking",
                "--disable-sync",
                "--disable-default-apps",
                "--disable-component-update",
                "--mute-audio",
                "--password-store=basic",
                "--use-mock-keychain",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| err("web.browser_unavailable", format!("{}: {e}", exe.display())))?;
        let ws_url = match wait_devtools(&profile) {
            Ok(u) => u,
            Err(e) => {
                let mut c = child;
                let _ = c.kill();
                let _ = std::fs::remove_dir_all(&profile);
                return Err(e);
            }
        };
        let url =
            url::Url::parse(&ws_url).map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        let addr = format!(
            "{}:{}",
            url.host_str().unwrap_or("127.0.0.1"),
            url.port().unwrap_or(9222)
        );
        let stream =
            TcpStream::connect(&addr).map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        let (ws, _) = tungstenite::client::client(ws_url.as_str(), stream)
            .map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        // Dopo l'handshake letture brevi: il ciclo degli eventi controlla le scadenze.
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(200)))
            .map_err(|e| err("web.browser_unavailable", e.to_string()))?;
        let mut b = Self {
            child,
            profile,
            ws,
            next_id: 0,
            session: String::new(),
            web: web.clone(),
            loaded: false,
            blocked: Vec::new(),
        };
        let target = b.call("Target.createTarget", json!({"url": "about:blank"}), false)?;
        let attached = b.call(
            "Target.attachToTarget",
            json!({"targetId": target["targetId"], "flatten": true}),
            false,
        )?;
        b.session = attached["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        b.call("Page.enable", json!({}), true)?;
        b.call(
            "Fetch.enable",
            json!({"patterns": [{"urlPattern": "*"}]}),
            true,
        )?;
        Ok(b)
    }

    fn send(&mut self, msg: &Value) -> Result<(), ToolError> {
        self.ws
            .send(Message::Text(msg.to_string().into()))
            .map_err(|e| err("web.browser_error", e.to_string()))
    }

    /// Legge un messaggio entro `until`; `None` se il tempo è scaduto.
    fn read(&mut self, until: Instant) -> Result<Option<Value>, ToolError> {
        while Instant::now() < until {
            match self.ws.read() {
                Ok(Message::Text(t)) => return Ok(serde_json::from_str(&t).ok()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(err("web.browser_error", e.to_string())),
            }
        }
        Ok(None)
    }

    /// Gestisce un evento: applica la policy di rete alle richieste e registra il caricamento della pagina.
    fn on_event(&mut self, ev: &Value) -> Result<(), ToolError> {
        match ev["method"].as_str() {
            Some("Fetch.requestPaused") => {
                let id = ev["params"]["requestId"].clone();
                let url = ev["params"]["request"]["url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let allowed = url.starts_with("data:")
                    || url.starts_with("blob:")
                    || crate::web::check_url(&self.web, &url)
                        .ok()
                        .is_some_and(|u| {
                            crate::web::resolve_checked(&u, self.web.allow_private).is_ok()
                        });
                if allowed {
                    self.send_session("Fetch.continueRequest", json!({"requestId": id}))?;
                } else {
                    if self.blocked.len() < 20 {
                        self.blocked.push(url.chars().take(200).collect());
                    }
                    self.send_session(
                        "Fetch.failRequest",
                        json!({"requestId": id, "errorReason": "AccessDenied"}),
                    )?;
                }
            }
            Some("Page.loadEventFired") => self.loaded = true,
            _ => {}
        }
        Ok(())
    }

    fn send_session(&mut self, method: &str, params: Value) -> Result<u64, ToolError> {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({"id": id, "method": method, "params": params, "sessionId": self.session});
        self.send(&msg)?;
        Ok(id)
    }

    /// Comando DevTools con risposta; gli eventi ricevuti nel frattempo vengono gestiti.
    fn call(&mut self, method: &str, params: Value, in_page: bool) -> Result<Value, ToolError> {
        let id = if in_page {
            self.send_session(method, params)?
        } else {
            self.next_id += 1;
            let id = self.next_id;
            self.send(&json!({"id": id, "method": method, "params": params}))?;
            id
        };
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let Some(msg) = self.read(until)? else {
                return Err(err(
                    "web.timeout",
                    format!("il browser non ha risposto a {method}"),
                ));
            };
            if msg["id"].as_u64() == Some(id) {
                if let Some(e) = msg.get("error") {
                    return Err(err(
                        "web.browser_error",
                        e["message"]
                            .as_str()
                            .unwrap_or("errore del browser")
                            .to_string(),
                    ));
                }
                return Ok(msg["result"].clone());
            }
            self.on_event(&msg)?;
        }
    }

    /// Attende il caricamento della pagina (o `max`) gestendo gli eventi di rete.
    fn wait_load(&mut self, max: Duration) -> Result<(), ToolError> {
        let until = Instant::now() + max;
        while !self.loaded {
            match self.read(until)? {
                Some(ev) => self.on_event(&ev)?,
                None => break,
            }
        }
        // Breve assestamento per gli script che completano la pagina dopo il caricamento.
        let settle = Instant::now() + Duration::from_millis(300);
        while let Some(ev) = self.read(settle)? {
            self.on_event(&ev)?;
        }
        Ok(())
    }

    /// Apre un indirizzo controllato dalla policy e attende il caricamento.
    pub fn open(&mut self, url: &str) -> Result<(), ToolError> {
        crate::web::check_url(&self.web, url)
            .map_err(|code| err(code, crate::policy::explain(code)))?;
        self.loaded = false;
        let r = self.call("Page.navigate", json!({"url": url}), true)?;
        if let Some(e) = r["errorText"].as_str() {
            let code = if self.blocked.iter().any(|b| b.starts_with(url)) {
                "web.blocked"
            } else {
                "web.unreachable"
            };
            return Err(err(code, format!("pagina non raggiungibile: {e}")));
        }
        self.wait_load(Duration::from_secs(15))
    }

    /// Valuta uno script nella pagina e restituisce il valore JSON.
    fn eval(&mut self, script: &str) -> Result<Value, ToolError> {
        let r = self.call(
            "Runtime.evaluate",
            json!({"expression": script, "returnByValue": true, "awaitPromise": true}),
            true,
        )?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(err(
                "web.browser_error",
                e["text"]
                    .as_str()
                    .unwrap_or("errore nello script")
                    .to_string(),
            ));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Fa clic sul collegamento con quel testo (o indice) e attende la nuova pagina.
    pub fn click(&mut self, link: &str) -> Result<(), ToolError> {
        let script = format!(
            r#"(() => {{ const q = {q}; const links = [...document.querySelectorAll('a[href]')];
                const a = /^\d+$/.test(q) ? links[Number(q)] : links.find(l => l.innerText.trim().toLowerCase().includes(q.toLowerCase()));
                if (!a) return false; a.removeAttribute('target'); a.click(); return true; }})()"#,
            q = Value::String(link.to_string())
        );
        self.loaded = false;
        if self.eval(&script)? != Value::Bool(true) {
            return Err(err(
                "web.element_not_found",
                format!("nessun collegamento «{link}» nella pagina"),
            ));
        }
        self.wait_load(Duration::from_secs(10))
    }

    /// Compila i campi di un modulo (per nome) e, se richiesto, lo invia.
    pub fn fill(&mut self, form: usize, fields: &Value, submit: bool) -> Result<(), ToolError> {
        let script = format!(
            r#"(() => {{ const f = document.forms[{form}]; if (!f) return "modulo";
                const values = {values};
                for (const [name, value] of Object.entries(values)) {{
                  const el = f.elements.namedItem(name); if (!el) return "campo:" + name;
                  if (el.type === 'checkbox' || el.type === 'radio') el.checked = !!value && value !== 'false';
                  else {{ el.value = String(value); el.dispatchEvent(new Event('input', {{bubbles: true}})); el.dispatchEvent(new Event('change', {{bubbles: true}})); }}
                }}
                if ({submit}) {{ if (f.requestSubmit) f.requestSubmit(); else f.submit(); }}
                return "ok"; }})()"#,
            values = fields
        );
        self.loaded = false;
        match self.eval(&script)?.as_str() {
            Some("ok") => {}
            Some("modulo") => {
                return Err(err(
                    "web.element_not_found",
                    format!("la pagina non ha il modulo {form}"),
                ))
            }
            Some(other) => {
                return Err(err(
                    "web.element_not_found",
                    format!(
                        "campo «{}» non presente nel modulo",
                        other.trim_start_matches("campo:")
                    ),
                ))
            }
            None => return Err(err("web.browser_error", "risultato inatteso")),
        }
        if submit {
            self.wait_load(Duration::from_secs(10))?;
        }
        Ok(())
    }

    /// Istantanea della pagina: indirizzo, titolo, testo visibile, collegamenti e moduli (con i nomi dei campi).
    pub fn snapshot(&mut self, requested: &str) -> Result<Value, ToolError> {
        let script = format!(
            r#"(() => ({{
              url: location.href, title: document.title,
              text: (document.body ? document.body.innerText : '').slice(0, {MAX_TEXT}),
              links: [...document.querySelectorAll('a[href]')].slice(0, {MAX_LINKS}).map((a, i) => ({{index: i, text: a.innerText.trim().slice(0, 120), href: a.href}})),
              forms: [...document.forms].slice(0, 10).map((f, i) => ({{index: i, action: f.action, method: (f.method || 'get').toLowerCase(),
                fields: [...f.elements].filter(e => e.name).slice(0, 40).map(e => ({{name: e.name, type: e.type,
                  label: ((e.labels && e.labels[0] && e.labels[0].innerText) || e.placeholder || '').trim().slice(0, 80)}}))}}))
            }}))()"#
        );
        let mut snap = self.eval(&script)?;
        let text = snap["text"].as_str().unwrap_or_default().to_string();
        snap["requested_url"] = requested.into();
        snap["retrieved_at"] = crate::now().into();
        snap["content_sha256"] =
            hex::encode(<sha2::Sha256 as sha2::Digest>::digest(text.as_bytes())).into();
        snap["blocked_requests"] = json!(self.blocked);
        snap["untrusted"] = true.into();
        Ok(snap)
    }
}

/// Attesa massima per l'avvio a freddo del browser: su una macchina carica (CI, avvio dopo l'accensione) 20 secondi
/// erano al limite, e un avvio più lento di così è comunque un guasto da segnalare.
const START_SECONDS: u64 = 60;

/// Legge `DevToolsActivePort` dal profilo: porta e percorso del WebSocket del browser.
fn wait_devtools(profile: &Path) -> Result<String, ToolError> {
    let until = Instant::now() + Duration::from_secs(START_SECONDS);
    while Instant::now() < until {
        if let Ok(t) = std::fs::read_to_string(profile.join("DevToolsActivePort")) {
            let mut lines = t.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                return Ok(format!("ws://127.0.0.1:{}{}", port.trim(), path.trim()));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(err(
        "web.browser_unavailable",
        format!("il browser non si è avviato entro {START_SECONDS} secondi"),
    ))
}

/// Web attivo e indirizzo ammesso, prima di avviare il browser.
fn precheck(web: &WebConfig, url: &str) -> Result<(), ToolError> {
    if !web.enabled {
        return Err(err(
            "policy.web_disabled",
            crate::policy::explain("policy.web_disabled"),
        ));
    }
    crate::web::check_url(web, url).map_err(|code| err(code, crate::policy::explain(code)))?;
    Ok(())
}

/// `web.browse`: apre la pagina, segue al massimo 3 collegamenti, restituisce l'istantanea finale.
pub fn browse(web: &WebConfig, args: &Value) -> Result<Value, ToolError> {
    let url = args["url"].as_str().unwrap_or_default();
    precheck(web, url)?;
    let mut b = Browser::launch(web)?;
    b.open(url)?;
    let mut followed = Vec::new();
    for link in args["follow"].as_array().into_iter().flatten().take(3) {
        let text = link.as_str().unwrap_or_default();
        b.click(text)?;
        followed.push(text.to_string());
    }
    let mut snap = b.snapshot(url)?;
    snap["followed"] = json!(followed);
    Ok(snap)
}

/// `web.form`: compila e invia un modulo (azione con effetti esterni, confermata dal proprietario).
pub fn submit_form(web: &WebConfig, args: &Value) -> Result<Value, ToolError> {
    let url = args["url"].as_str().unwrap_or_default();
    precheck(web, url)?;
    let mut b = Browser::launch(web)?;
    b.open(url)?;
    let form = args["form"].as_u64().unwrap_or(0) as usize;
    b.fill(
        form,
        &args["fields"],
        args["submit"].as_bool().unwrap_or(true),
    )?;
    let mut snap = b.snapshot(url)?;
    snap["submitted_form"] = form.into();
    Ok(snap)
}
