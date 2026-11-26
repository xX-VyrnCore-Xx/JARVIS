//! Collegamento dei dispositivi tramite Tailscale (ADR-0141).
//!
//! Il Core resta su `127.0.0.1`. Tailscale Serve lo pubblica con HTTPS privato nella rete del proprietario
//! (mai Funnel, che sarebbe pubblico). Questo modulo legge lo stato del client Tailscale installato sul computer,
//! ricava il nome della macchina e, su richiesta esplicita del proprietario, esegue `tailscale serve --bg <porta>`.
//! Così non serve scrivere file né variabili: l'app mostra il collegamento pronto da toccare o inviare.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_secs(15);

/// Percorsi in cui il programma di Tailscale si trova di solito (oltre al PATH).
fn candidates() -> Vec<String> {
    let mut list = vec!["tailscale".to_string()];
    if cfg!(target_os = "macos") {
        list.push("/Applications/Tailscale.app/Contents/MacOS/Tailscale".into());
        list.push("/usr/local/bin/tailscale".into());
        list.push("/opt/homebrew/bin/tailscale".into());
    }
    if cfg!(windows) {
        list.push(r"C:\Program Files\Tailscale\tailscale.exe".into());
    }
    list
}

/// Stato del client e nome della macchina dal JSON di `tailscale status --json`.
pub fn parse_status(raw: &str) -> (String, Option<String>) {
    let doc: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let state = doc["BackendState"].as_str().unwrap_or("").to_string();
    let host = doc["Self"]["DNSName"]
        .as_str()
        .map(|name| name.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|name| name.contains('.'));
    (state, host)
}

/// Dettagli che spiegano perché un nome non si trova (ADR-0192): MagicDNS e HTTPS della tailnet, indirizzi privati
/// e avvisi di salute del client. Se un campo manca nel JSON resta `None` o vuoto: nessuna ipotesi.
#[derive(Debug, Default, PartialEq)]
pub struct Details {
    pub magic_dns: Option<bool>,
    pub https_enabled: Option<bool>,
    pub ips: Vec<String>,
    pub health: Vec<String>,
}

pub fn parse_details(raw: &str) -> Details {
    let doc: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let texts = |v: &Value, limit: usize| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str())
            .map(|x| x.chars().take(200).collect())
            .take(limit)
            .collect()
    };
    Details {
        magic_dns: doc["CurrentTailnet"]["MagicDNSEnabled"].as_bool(),
        https_enabled: doc["CertDomains"]
            .as_array()
            .map(|domains| !domains.is_empty()),
        ips: texts(&doc["Self"]["TailscaleIPs"], 4),
        health: texts(&doc["Health"], 5),
    }
}

/// Indicazione per l'utente, dalla causa più probabile alla meno: il primo ostacolo reale vince.
pub fn advice(
    connected: bool,
    host: Option<&str>,
    serving: bool,
    details: &Details,
    resolves: Option<bool>,
) -> String {
    if !connected {
        return "Tailscale è installato ma non è connesso: apri Tailscale e accedi.".into();
    }
    if host.is_none() || details.magic_dns == Some(false) {
        return "MagicDNS è disattivato nella tua tailnet, per questo i dispositivi non trovano il nome della macchina («hostname could not be found»): attivalo da login.tailscale.com/admin/dns, poi riprova.".into();
    }
    if details.https_enabled == Some(false) {
        return "HTTPS non è abilitato nella tua tailnet: attiva «HTTPS Certificates» da login.tailscale.com/admin/dns, poi tocca di nuovo «Collega con Tailscale».".into();
    }
    if resolves == Some(false) {
        return "Il nome della macchina non si risolve ancora, nemmeno su questo computer: dopo aver attivato MagicDNS o HTTPS può servire fino a un minuto. Controlla in Tailscale che «Use Tailscale DNS settings» sia attivo, poi riprova. Su telefono e altri computer apri l'app Tailscale e verifica che sia connessa con lo stesso account.".into();
    }
    if !serving {
        return "Tailscale è pronto: tocca «Collega con Tailscale» per rendere raggiungibile JARVIS dai tuoi dispositivi.".into();
    }
    "JARVIS è raggiungibile dai tuoi dispositivi Tailscale. Se un dispositivo non trova il nome, apri Tailscale su quel dispositivo: deve essere connesso con lo stesso account e con «Use Tailscale DNS settings» attivo.".into()
}

/// Il nome si risolve su questo computer? Interrogazione con limite di tempo, senza aspettare oltre 4 secondi.
fn resolves_here(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    let target = format!("{host}:443");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(
            target
                .to_socket_addrs()
                .map(|mut a| a.next().is_some())
                .unwrap_or(false),
        );
    });
    rx.recv_timeout(Duration::from_secs(4)).unwrap_or(false)
}

/// Vero se `tailscale serve status` mostra già il Core locale pubblicato.
pub fn serving_port(raw: &str, port: u16) -> bool {
    raw.contains(&format!("127.0.0.1:{port}")) || raw.contains(&format!("localhost:{port}"))
}

/// Esegue Tailscale con argomenti fissi. `Ok(None)` se il programma non è installato.
fn run(args: &[&str]) -> Result<Option<(bool, String)>, String> {
    for program in candidates() {
        let spawned = Command::new(&program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        let started = Instant::now();
        let success = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if started.elapsed() < TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        let mut text = String::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut text);
        }
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut text);
        }
        return Ok(Some((success, text)));
    }
    Ok(None)
}

/// Stato del collegamento: installato, connesso, nome della macchina, indirizzo HTTPS e se il Core è pubblicato.
pub fn status(port: u16) -> Value {
    let status = match run(&["status", "--json"]) {
        Ok(Some((_, text))) => text,
        Ok(None) => {
            return json!({
                "installed": false, "state": "", "host": null, "url": null, "serving": false,
                "message": "Tailscale non è installato su questo computer. Installalo da tailscale.com e accedi con lo stesso account su tutti i dispositivi."
            })
        }
        Err(error) => {
            return json!({
                "installed": true, "state": "", "host": null, "url": null, "serving": false,
                "message": format!("Tailscale non risponde: {error}")
            })
        }
    };
    let (state, host) = parse_status(&status);
    let details = parse_details(&status);
    let connected = state == "Running";
    let serving = connected
        && matches!(run(&["serve", "status"]), Ok(Some((_, text))) if serving_port(&text, port));
    let resolves = host.as_deref().filter(|_| connected).map(resolves_here);
    let message = advice(connected, host.as_deref(), serving, &details, resolves);
    json!({
        "installed": true,
        "state": state,
        "host": host,
        "url": host.as_ref().map(|name| format!("https://{name}")),
        "serving": serving,
        "message": message,
        "magic_dns": details.magic_dns,
        "https_enabled": details.https_enabled,
        "dns_resolves_here": resolves,
        "tailscale_ips": details.ips,
        "health": details.health,
    })
}

/// Un passo della diagnosi: nome, esito e dettaglio breve.
fn step(name: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({"step": name, "ok": ok, "detail": detail.into().chars().take(240).collect::<String>()})
}

/// Esito della diagnosi dal primo passo che fallisce (`dns`, `connect`, `tls`, `http`) o `ok`.
pub fn verdict(failed: Option<&str>, serve_text: &str) -> String {
    match failed {
        None => "Il collegamento funziona: il nome si risolve, il certificato è valido e JARVIS risponde. Se un altro dispositivo non lo trova, il problema è su quel dispositivo: apri Tailscale lì, controlla che sia connesso con lo stesso account e che «Use Tailscale DNS settings» sia attivo.".into(),
        Some("dns") => "Questo computer non risolve il nome della macchina, anche se Tailscale è connesso. Cause tipiche: Tailscale avviato in modalità «userspace» (senza interfaccia di rete di sistema: succede con il demone installato da Homebrew), oppure «Use Tailscale DNS settings» disattivato, oppure un DNS/VPN aziendale che lo intercetta. Dagli altri dispositivi il nome può funzionare comunque: prova da lì. Per usare il nome anche su questo computer installa l'app Tailscale per macOS (non il solo demone) e attiva l'uso del DNS di Tailscale.".into(),
        Some("connect") => format!(
            "Il nome si risolve ma la porta 443 non risponde: JARVIS non è pubblicato. Tocca «Collega con Tailscale»{}",
            if serve_text.trim().is_empty() { "." } else { " (Tailscale Serve non mostra nessuna pubblicazione attiva)." }
        ),
        Some("tls") => "La connessione HTTPS non si completa: il certificato non è ancora pronto o non è valido. Abilita «HTTPS Certificates» su login.tailscale.com/admin/dns, tocca di nuovo «Collega con Tailscale» e riprova dopo un minuto.".into(),
        Some(_) => "Il nome risponde in HTTPS ma non è JARVIS: Tailscale Serve punta a un'altra porta. Tocca «Collega con Tailscale» per ripubblicare la porta corretta.".into(),
    }
}

/// Diagnosi del collegamento dal punto di vista di questo computer: DNS, porta 443, certificato HTTPS e risposta di
/// JARVIS (`/api/connectivity`, senza credenziali). Ogni passo ha un tempo massimo; si ferma al primo che fallisce.
pub fn diagnose(port: u16) -> Value {
    let status = match run(&["status", "--json"]) {
        Ok(Some((_, text))) => text,
        Ok(None) => {
            return json!({"ok": false, "steps": [], "verdict": "Tailscale non è installato su questo computer."})
        }
        Err(error) => {
            return json!({"ok": false, "steps": [], "verdict": format!("Tailscale non risponde: {error}")})
        }
    };
    let (state, host) = parse_status(&status);
    let details = parse_details(&status);
    let mut steps = vec![step(
        "tailscale",
        state == "Running",
        format!("stato {state}"),
    )];
    if state != "Running" {
        return json!({"ok": false, "steps": steps, "health": details.health,
            "verdict": "Tailscale non è connesso: apri Tailscale e accedi."});
    }
    let Some(host) = host else {
        return json!({"ok": false, "steps": steps, "health": details.health,
            "verdict": advice(true, None, false, &details, None)});
    };
    let serve_text = run(&["serve", "status"])
        .ok()
        .flatten()
        .map(|(_, text)| text)
        .unwrap_or_default();
    steps.push(step(
        "serve",
        serving_port(&serve_text, port),
        if serve_text.trim().is_empty() {
            "nessuna pubblicazione".to_string()
        } else {
            serve_text
                .trim()
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" | ")
        },
    ));
    let (tx, rx) = std::sync::mpsc::channel();
    let target = host.clone();
    std::thread::spawn(move || {
        let _ = tx.send(probe_remote(&target));
    });
    let probe = rx
        .recv_timeout(Duration::from_secs(14))
        .unwrap_or_else(|_| vec![step("dns", false, "nessuna risposta entro 14 secondi")]);
    let failed = probe
        .iter()
        .find(|s| s["ok"] == false)
        .and_then(|s| s["step"].as_str().map(str::to_string));
    steps.extend(probe);
    let ok = failed.is_none();
    json!({"ok": ok, "host": host, "steps": steps, "health": details.health,
        "tailscale_ips": details.ips, "verdict": verdict(failed.as_deref(), &serve_text)})
}

/// DNS → TCP 443 → HTTPS verso `/api/connectivity`, dal Core su questo computer.
fn probe_remote(host: &str) -> Vec<Value> {
    use std::net::{TcpStream, ToSocketAddrs};
    let mut steps = Vec::new();
    let addrs: Vec<std::net::SocketAddr> = match (host, 443).to_socket_addrs() {
        Ok(found) => found.collect(),
        Err(error) => return vec![step("dns", false, format!("{host}: {error}"))],
    };
    if addrs.is_empty() {
        return vec![step("dns", false, format!("{host}: nessun indirizzo"))];
    }
    steps.push(step(
        "dns",
        true,
        addrs
            .iter()
            .take(2)
            .map(|a| a.ip().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    ));
    if let Err(error) = TcpStream::connect_timeout(&addrs[0], Duration::from_secs(4)) {
        steps.push(step("connect", false, error.to_string()));
        return steps;
    }
    steps.push(step("connect", true, "porta 443 raggiungibile"));
    let agent = ureq::AgentBuilder::new()
        .tls_config(crate::net::tls_config())
        .timeout(Duration::from_secs(6))
        .redirects(0)
        .build();
    match agent
        .get(&format!("https://{host}/api/connectivity"))
        .call()
    {
        Ok(response) => {
            let mut raw = String::new();
            let _ = response
                .into_reader()
                .take(8 * 1024)
                .read_to_string(&mut raw);
            let body: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
            let is_jarvis = body["service"] == "jarvis-core";
            steps.push(step("tls", true, "certificato valido"));
            steps.push(step(
                "http",
                is_jarvis,
                if is_jarvis {
                    "risponde JARVIS".to_string()
                } else {
                    "la risposta non è di JARVIS".to_string()
                },
            ));
        }
        Err(ureq::Error::Status(code, _)) => {
            // Una risposta HTTP significa che DNS, porta e certificato funzionano.
            steps.push(step("tls", true, "certificato valido"));
            steps.push(step("http", false, format!("stato HTTP {code}")));
        }
        Err(error) => {
            steps.push(step("tls", false, error.to_string()));
        }
    }
    steps
}

/// Pubblica il Core locale nella rete privata (`tailscale serve --bg <porta>`). Non usa mai Funnel.
pub fn publish(port: u16) -> Result<String, String> {
    let port = port.to_string();
    match run(&["serve", "--bg", &port])? {
        None => Err("Tailscale non è installato su questo computer.".into()),
        Some((true, text)) => Ok(text.trim().to_string()),
        Some((false, text)) => {
            let detail = text.trim();
            Err(if detail.is_empty() {
                "Tailscale non ha completato la pubblicazione: controlla di essere connesso e di aver abilitato HTTPS nella console.".into()
            } else {
                detail.chars().take(400).collect()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_machine_name_comes_from_the_status() {
        let raw = r#"{"BackendState":"Running","Self":{"DNSName":"Casa-Mac.tail1234.ts.net.","HostName":"x"}}"#;
        let (state, host) = parse_status(raw);
        assert_eq!(state, "Running");
        assert_eq!(host.as_deref(), Some("casa-mac.tail1234.ts.net"));
        let (state, host) = parse_status(r#"{"BackendState":"NeedsLogin","Self":{"DNSName":""}}"#);
        assert_eq!((state.as_str(), host), ("NeedsLogin", None));
        assert_eq!(parse_status("non json"), (String::new(), None));
    }

    #[test]
    fn details_explain_why_a_name_cannot_be_found() {
        let raw = r#"{"BackendState":"Running","Health":["DNS non raggiungibile"],
            "CurrentTailnet":{"MagicDNSEnabled":false},"CertDomains":[],
            "Self":{"DNSName":"casa.tail1.ts.net.","TailscaleIPs":["100.64.0.5","fd7a::5"]}}"#;
        let details = parse_details(raw);
        assert_eq!(details.magic_dns, Some(false));
        assert_eq!(details.https_enabled, Some(false));
        assert_eq!(details.ips, ["100.64.0.5", "fd7a::5"]);
        assert_eq!(details.health, ["DNS non raggiungibile"]);
        assert_eq!(parse_details("non json"), Details::default());
        // Il primo ostacolo reale vince: prima MagicDNS, poi HTTPS, poi la risoluzione sul computer.
        let host = Some("casa.tail1.ts.net");
        let off = advice(true, host, false, &details, Some(false));
        assert!(off.contains("MagicDNS è disattivato"), "{off}");
        let no_https = Details {
            magic_dns: Some(true),
            https_enabled: Some(false),
            ..Default::default()
        };
        assert!(advice(true, host, false, &no_https, Some(true)).contains("HTTPS Certificates"));
        let fine = Details {
            magic_dns: Some(true),
            https_enabled: Some(true),
            ..Default::default()
        };
        assert!(advice(true, host, true, &fine, Some(false)).contains("non si risolve ancora"));
        assert!(advice(true, host, false, &fine, Some(true)).contains("Collega con Tailscale"));
        assert!(advice(true, host, true, &fine, Some(true)).contains("raggiungibile"));
        assert!(advice(false, None, false, &fine, None).contains("non è connesso"));
        // Dati assenti nel JSON: nessuna ipotesi, nessun falso allarme.
        assert!(!advice(true, host, true, &Details::default(), None).contains("disattivato"));
    }

    #[test]
    fn publishing_is_recognised_by_the_local_port() {
        let shown = "https://casa.tail1.ts.net (tailnet only)\n|-- / proxy http://127.0.0.1:8765";
        assert!(serving_port(shown, 8765));
        assert!(!serving_port(shown, 9000));
        assert!(!serving_port("No serve config", 8765));
    }

    #[test]
    fn the_verdict_names_the_first_failing_step_and_what_to_do() {
        assert!(verdict(None, "").contains("funziona"));
        let dns = verdict(Some("dns"), "");
        assert!(
            dns.contains("userspace") && dns.contains("Use Tailscale DNS settings"),
            "{dns}"
        );
        assert!(verdict(Some("connect"), "").contains("Collega con Tailscale"));
        assert!(
            verdict(Some("connect"), "No serve config").contains("nessuna pubblicazione attiva")
        );
        assert!(verdict(Some("tls"), "").contains("HTTPS Certificates"));
        assert!(verdict(Some("http"), "").contains("non è JARVIS"));
        let item = step("dns", false, "x".repeat(500));
        assert_eq!(item["detail"].as_str().unwrap().chars().count(), 240);
    }
}
