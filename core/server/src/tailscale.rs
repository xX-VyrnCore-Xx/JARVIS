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
}
