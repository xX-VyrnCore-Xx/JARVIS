//! Verifica delle connessioni verso i servizi esterni (ADR-0222).
//!
//! Prova in parallelo, senza credenziali e senza inviare dati dell'utente, che il Core raggiunga i servizi
//! da cui dipende: modelli (NVIDIA NIM), Hugging Face, accesso Google (Gmail e Calendar), GitHub e Tailscale.
//! Qualsiasi risposta HTTP, anche 401/404/302, prova che DNS, rete, proxy e TLS funzionano: l'esito dice
//! «raggiungibile», non «autorizzato». Le credenziali si verificano a parte, dal singolo connettore.

use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Servizio da provare: identificatore, nome leggibile e indirizzo pubblico.
pub struct Target {
    pub id: &'static str,
    pub label: &'static str,
    pub url: String,
}

/// Servizi esterni da cui dipende JARVIS.
pub fn defaults() -> Vec<Target> {
    [
        (
            "nvidia_nim",
            "NVIDIA NIM (modelli)",
            "https://integrate.api.nvidia.com/v1/models",
        ),
        (
            "huggingface",
            "Hugging Face",
            "https://huggingface.co/api/models?limit=1",
        ),
        (
            "google_oauth",
            "Accesso Google",
            "https://accounts.google.com/.well-known/openid-configuration",
        ),
        ("gmail", "Gmail", "https://gmail.googleapis.com/"),
        (
            "google_calendar",
            "Google Calendar",
            "https://www.googleapis.com/calendar/v3/",
        ),
        ("github", "GitHub", "https://api.github.com/"),
        ("tailscale", "Tailscale", "https://login.tailscale.com/"),
    ]
    .into_iter()
    .map(|(id, label, url)| Target {
        id,
        label,
        url: url.into(),
    })
    .collect()
}

fn probe(target: &Target) -> Value {
    let started = Instant::now();
    let agent = crate::net::builder(Duration::from_secs(8))
        .try_proxy_from_env(target.url.starts_with("https://"))
        .build();
    let outcome = match agent
        .get(&target.url)
        .set("Accept", "application/json")
        .call()
    {
        Ok(response) => Ok(response.status()),
        Err(ureq::Error::Status(status, _)) => Ok(status),
        Err(ureq::Error::Transport(t)) => Err(crate::net::classify(&t)),
    };
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(status) => json!({
            "id": target.id, "label": target.label, "reachable": status < 500,
            "status": status, "ms": ms,
            "note": if status < 500 { Value::Null } else { json!("Il servizio risponde con un errore: riprova più tardi.") },
        }),
        Err(kind) => json!({
            "id": target.id, "label": target.label, "reachable": false,
            "status": Value::Null, "ms": ms, "note": kind.hint(), "cause": kind.code(),
        }),
    }
}

/// Esegue le prove in parallelo (una per servizio, al più 8 s ciascuna).
pub fn check(targets: Vec<Target>) -> Value {
    let results: Vec<Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = targets
            .iter()
            .map(|target| scope.spawn(move || probe(target)))
            .collect();
        handles
            .into_iter()
            .zip(&targets)
            .map(|(handle, target)| {
                handle.join().unwrap_or_else(|_| {
                    json!({"id": target.id, "label": target.label, "reachable": false,
                           "status": Value::Null, "ms": 0, "note": "Verifica interrotta."})
                })
            })
            .collect()
    });
    let reachable = results.iter().filter(|r| r["reachable"] == true).count();
    json!({"services": results, "reachable": reachable, "total": results.len(),
           "all_reachable": reachable == results.len()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    fn serve(status: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok() && line.trim() != "" {
                    line.clear();
                }
                let mut out = stream;
                let _ = write!(
                    out,
                    "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        url
    }

    #[test]
    fn any_http_answer_proves_reachability_but_server_errors_do_not() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("http://{}/", closed.local_addr().unwrap());
        drop(closed);
        let report = check(vec![
            Target {
                id: "ok",
                label: "Ok",
                url: serve("200 OK"),
            },
            Target {
                id: "auth",
                label: "Auth",
                url: serve("401 Unauthorized"),
            },
            Target {
                id: "down",
                label: "Giù",
                url: serve("503 Service Unavailable"),
            },
            Target {
                id: "dead",
                label: "Spento",
                url: dead,
            },
        ]);
        let by = |id: &str| {
            report["services"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap()
                .clone()
        };
        assert_eq!(by("ok")["reachable"], true);
        assert_eq!(by("auth")["reachable"], true);
        assert_eq!(by("auth")["status"], 401);
        assert_eq!(by("down")["reachable"], false);
        assert_eq!(by("dead")["reachable"], false);
        assert!(by("dead")["note"].is_string());
        assert_eq!(report["reachable"], 2);
        assert_eq!(report["all_reachable"], false);
    }

    #[test]
    fn default_targets_cover_the_documented_services() {
        let ids: Vec<_> = defaults().iter().map(|t| t.id).collect();
        for needed in [
            "nvidia_nim",
            "huggingface",
            "gmail",
            "google_calendar",
            "github",
            "tailscale",
        ] {
            assert!(ids.contains(&needed), "{needed}");
        }
        assert!(defaults().iter().all(|t| t.url.starts_with("https://")));
    }
}
