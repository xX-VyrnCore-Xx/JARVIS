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
    let connected = state == "Running";
    let serving = connected
        && matches!(run(&["serve", "status"]), Ok(Some((_, text))) if serving_port(&text, port));
    let message = match (connected, &host, serving) {
        (false, _, _) => "Tailscale è installato ma non è connesso: apri Tailscale e accedi.",
        (true, None, _) => "Tailscale è connesso ma non mostra il nome della macchina: abilita MagicDNS e HTTPS nella console di Tailscale.",
        (true, Some(_), false) => "Tailscale è pronto: tocca «Collega con Tailscale» per rendere raggiungibile JARVIS dai tuoi dispositivi.",
        (true, Some(_), true) => "JARVIS è raggiungibile dai tuoi dispositivi Tailscale.",
    };
    json!({
        "installed": true,
        "state": state,
        "host": host,
        "url": host.as_ref().map(|name| format!("https://{name}")),
        "serving": serving,
        "message": message,
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
    fn publishing_is_recognised_by_the_local_port() {
        let shown = "https://casa.tail1.ts.net (tailnet only)\n|-- / proxy http://127.0.0.1:8765";
        assert!(serving_port(shown, 8765));
        assert!(!serving_port(shown, 9000));
        assert!(!serving_port("No serve config", 8765));
    }
}
