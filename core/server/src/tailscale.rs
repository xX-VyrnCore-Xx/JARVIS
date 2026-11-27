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
    let mut list: Vec<String> = Vec::new();
    // Su macOS l'app è la copia canonica: la riga di comando di Homebrew (nel PATH) può parlare con un altro
    // demone, non collegato allo stesso account, e dare uno stato diverso da quello che vede l'utente.
    if cfg!(target_os = "macos") {
        list.push("/Applications/Tailscale.app/Contents/MacOS/Tailscale".into());
    }
    list.push("tailscale".to_string());
    if cfg!(target_os = "macos") {
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

/// Indicazione per l'utente. **La prova reale vince sul dato dichiarato** (ADR-0230): `MagicDNSEnabled` e
/// `CertDomains` del JSON di Tailscale possono essere vuoti o in ritardo anche a funzionalità attive; se il nome si
/// risolve o Tailscale Serve è già pubblicato, quei campi si ignorano. Se la prova fallisce, diventano un indizio
/// con la cautela «se l'hai già attivato, ignora».
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
    let Some(_) = host else {
        return "Tailscale non mostra ancora il nome della macchina. Se MagicDNS è già attivo (login.tailscale.com/admin/dns), attendi qualche secondo che l'app Tailscale si sincronizzi e riprova; se il nome non compare, esci e rientra dall'app Tailscale.".into();
    };
    let flagged = if details.magic_dns == Some(false) {
        Some("MagicDNS risulta disattivato")
    } else if details.https_enabled == Some(false) {
        Some("«HTTPS Certificates» risulta disattivato")
    } else {
        None
    };
    match resolves {
        // Il nome funziona: i campi dichiarati, se dicono altro, sono sbagliati o in ritardo.
        Some(true) if serving => "JARVIS è raggiungibile dai tuoi dispositivi Tailscale. Se un dispositivo non trova il nome, apri Tailscale su quel dispositivo: deve essere connesso con lo stesso account e con «Use Tailscale DNS settings» attivo.".into(),
        Some(true) => "Tailscale è pronto: tocca «Collega con Tailscale» per rendere raggiungibile JARVIS dai tuoi dispositivi.".into(),
        Some(false) => {
            let hint = flagged.map_or(String::new(), |text| {
                format!(" Tailscale segnala che {text}: se l'hai già attivato nella console, ignora questo avviso (il dato può essere in ritardo)." )
            });
            if serving {
                format!("JARVIS è pubblicato, ma questo computer non risolve il nome della macchina (succede con il demone senza interfaccia di rete o con «Use Tailscale DNS settings» disattivato). Gli altri dispositivi possono funzionare comunque: tocca «Diagnostica» per provare il collegamento direttamente.{hint}")
            } else {
                format!("Il nome della macchina non si risolve su questo computer: dopo aver attivato MagicDNS o HTTPS può servire fino a un minuto. Tocca «Collega con Tailscale», poi «Diagnostica».{hint}")
            }
        }
        None if serving => "JARVIS è pubblicato nella tua rete Tailscale.".into(),
        None => "Tailscale è pronto: tocca «Collega con Tailscale» per rendere raggiungibile JARVIS dai tuoi dispositivi.".into(),
    }
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

/// Esito di un comando Tailscale, con i due flussi separati: gli avvisi (per esempio «client version != server
/// version») arrivano su stderr e non devono rovinare il JSON di stdout.
#[derive(Debug)]
pub struct Out {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    /// Testo complessivo per i messaggi e per `serve status`.
    pub fn text(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
            .trim()
            .to_string()
    }
}

/// Il programma c'è ma non riesce a parlare con il demone: è un'altra copia (per esempio la riga di comando di
/// Homebrew accanto all'app di macOS), quindi conviene provare il candidato successivo.
pub fn daemon_unreachable(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    [
        "failed to connect to local",
        "doesn't appear to be running",
        "does not appear to be running",
        "is tailscale running",
        "tailscaled is not running",
        "connect: no such file or directory",
        "the tailscale ipn service",
    ]
    .iter()
    .any(|marker| t.contains(marker))
}

/// Legge un flusso fino alla fine **in parallelo** con l'attesa del processo: `status --json` di una tailnet con
/// molti dispositivi supera il buffer della pipe e, se nessuno legge, il processo si blocca fino al limite di tempo.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = pipe {
            let mut limited = pipe.take(8 * 1024 * 1024);
            let mut raw = Vec::new();
            let _ = limited.read_to_end(&mut raw);
            text = String::from_utf8_lossy(&raw).into_owned();
        }
        text
    })
}

/// Esegue Tailscale con argomenti fissi provando i candidati in ordine. `Ok(None)` se nessuno è installato.
fn run_with(programs: &[String], args: &[&str]) -> Result<Option<Out>, String> {
    let mut last: Option<Out> = None;
    for program in programs {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::hide_console(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let started = Instant::now();
        let ok = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if started.elapsed() < TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        let out = Out {
            ok,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        };
        if out.ok || !daemon_unreachable(&out.text()) {
            return Ok(Some(out));
        }
        last = Some(out);
    }
    Ok(last)
}

fn run(args: &[&str]) -> Result<Option<Out>, String> {
    run_with(&candidates(), args)
}

/// Il JSON di `status`, ignorando avvisi prima o dopo: da stdout, altrimenti dal primo `{` all'ultimo `}`.
pub fn json_part(out: &Out) -> String {
    let direct = out.stdout.trim();
    if direct.starts_with('{') && serde_json::from_str::<Value>(direct).is_ok() {
        return direct.to_string();
    }
    let all = out.text();
    match (all.find('{'), all.rfind('}')) {
        (Some(a), Some(b)) if b > a => all[a..=b].to_string(),
        _ => all,
    }
}

/// `status --json` senza l'elenco degli altri dispositivi (non serve e può essere enorme); se il client è
/// così vecchio da non conoscere l'opzione, ripiega sulla forma completa.
fn status_out() -> Result<Option<Out>, String> {
    match run(&["status", "--json", "--peers=false"])? {
        Some(out)
            if !out.ok && {
                let t = out.text().to_ascii_lowercase();
                t.contains("flag provided but not defined") || t.contains("unknown flag")
            } =>
        {
            run(&["status", "--json"])
        }
        other => Ok(other),
    }
}

/// Stato del collegamento: installato, connesso, nome della macchina, indirizzo HTTPS e se il Core è pubblicato.
pub fn status(port: u16) -> Value {
    let status = match status_out() {
        Ok(Some(out)) => out,
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
    let raw = json_part(&status);
    let (state, host) = parse_status(&raw);
    let details = parse_details(&raw);
    let connected = state == "Running";
    let serving = connected
        && matches!(run(&["serve", "status"]), Ok(Some(out)) if serving_port(&out.text(), port));
    let resolves = host.as_deref().filter(|_| connected).map(resolves_here);
    let mut message = advice(connected, host.as_deref(), serving, &details, resolves);
    if state.is_empty() {
        // Nessuno stato leggibile: il motivo vero è nell'output del programma (demone fermo, accesso, permessi).
        let first = status.text();
        let first = first
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("nessuna risposta");
        message = format!(
            "Tailscale è installato ma non risponde: «{}». Apri l'app Tailscale, accedi e attendi che risulti connessa.",
            first.chars().take(200).collect::<String>()
        );
    }
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

/// Esito quando solo il DNS di questo computer non funziona ma il collegamento reale sì.
const LOCAL_DNS_ONLY: &str = "Il collegamento funziona: JARVIS è pubblicato, il certificato è valido e risponde. Solo questo computer non risolve il nome (demone Tailscale senza interfaccia di rete o «Use Tailscale DNS settings» disattivato qui): non influisce sugli altri dispositivi. Se un dispositivo non si collega, apri Tailscale su quel dispositivo: deve essere connesso con lo stesso account.";

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
    let status = match status_out() {
        Ok(Some(out)) => json_part(&out),
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
        .map(|out| out.text())
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
    let ips = details.ips.clone();
    std::thread::spawn(move || {
        let _ = tx.send(probe_remote(&target, &ips));
    });
    let probe = rx
        .recv_timeout(Duration::from_secs(14))
        .unwrap_or_else(|_| vec![step("dns", false, "nessuna risposta entro 14 secondi")]);
    let failed = failed_step(&probe);
    let local_dns_only =
        failed.is_none() && probe.iter().any(|s| s["step"] == "dns" && s["ok"] == false);
    steps.extend(probe);
    let ok = failed.is_none();
    json!({"ok": ok, "host": host, "steps": steps, "health": details.health,
        "tailscale_ips": details.ips, "local_dns_only": local_dns_only,
        "verdict": if local_dns_only { LOCAL_DNS_ONLY.to_string() } else { verdict(failed.as_deref(), &serve_text) }})
}

/// Risolutore che indirizza il nome della macchina all'IP di Tailscale: il certificato si verifica comunque sul
/// nome, ma la connessione non dipende dal DNS di questo computer.
struct PinnedAddress(std::net::SocketAddr);

impl ureq::Resolver for PinnedAddress {
    fn resolve(&self, _netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
        Ok(vec![self.0])
    }
}

/// Primo indirizzo IPv4 di Tailscale (100.64.0.0/10) tra quelli del client.
pub fn tailscale_ipv4(ips: &[String]) -> Option<std::net::Ipv4Addr> {
    ips.iter()
        .filter_map(|ip| ip.parse::<std::net::Ipv4Addr>().ok())
        .find(|ip| ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
}

/// DNS → TCP 443 → HTTPS verso `/api/connectivity`, dal Core su questo computer. Se **questo** computer non
/// risolve il nome (demone senza interfaccia di rete, DNS di Tailscale non in uso), la prova prosegue
/// direttamente sull'indirizzo Tailscale: dimostra che pubblicazione e certificato funzionano, che è ciò che
/// serve agli altri dispositivi (ADR-0230).
fn probe_remote(host: &str, ips: &[String]) -> Vec<Value> {
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
    let mut steps = Vec::new();
    let mut pinned = false;
    let resolved: Result<Vec<SocketAddr>, String> = match (host, 443).to_socket_addrs() {
        Ok(found) => {
            let list: Vec<SocketAddr> = found.collect();
            if list.is_empty() {
                Err(format!("{host}: nessun indirizzo"))
            } else {
                Ok(list)
            }
        }
        Err(error) => Err(format!("{host}: {error}")),
    };
    let addrs = match resolved {
        Ok(list) => {
            steps.push(step(
                "dns",
                true,
                list.iter()
                    .take(2)
                    .map(|a| a.ip().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
            list
        }
        Err(reason) => {
            match tailscale_ipv4(ips) {
                Some(ip) => {
                    pinned = true;
                    steps.push(step(
                    "dns",
                    false,
                    format!("{reason}. Questo computer non risolve il nome: provo direttamente {ip}"),
                ));
                    vec![SocketAddr::new(ip.into(), 443)]
                }
                None => return vec![step("dns", false, reason)],
            }
        }
    };
    if let Err(error) = TcpStream::connect_timeout(&addrs[0], Duration::from_secs(4)) {
        steps.push(step("connect", false, error.to_string()));
        return steps;
    }
    steps.push(step("connect", true, "porta 443 raggiungibile"));
    let mut builder = ureq::AgentBuilder::new()
        .tls_config(crate::net::tls_config())
        .timeout(Duration::from_secs(6))
        .redirects(0);
    if pinned {
        builder = builder.resolver(PinnedAddress(addrs[0]));
    }
    let agent = builder.build();
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

/// Esito della prova: i passi dopo il DNS decidono. Un DNS locale che fallisce ma con connessione, certificato e
/// risposta di JARVIS riusciti sull'IP di Tailscale non è un guasto del collegamento.
pub fn failed_step(steps: &[Value]) -> Option<String> {
    let reached = steps.iter().any(|s| s["step"] == "http" && s["ok"] == true);
    steps
        .iter()
        .find(|s| s["ok"] == false && !(reached && s["step"] == "dns"))
        .and_then(|s| s["step"].as_str().map(str::to_string))
}

/// Pubblica il Core locale nella rete privata (`tailscale serve --bg <porta>`). Non usa mai Funnel.
pub fn publish(port: u16) -> Result<String, String> {
    let port = port.to_string();
    match run(&["serve", "--bg", &port])? {
        None => Err("Tailscale non è installato su questo computer.".into()),
        Some(out) if out.ok => Ok(out.text()),
        Some(out) => {
            let text = out.text();
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
    fn a_failing_local_dns_is_not_a_failure_when_the_direct_probe_works() {
        let ok = |name: &str, ok: bool| step(name, ok, "x");
        // DNS locale fallito, ma connessione, certificato e risposta di JARVIS riusciti sull'IP di Tailscale.
        let reached = [
            ok("dns", false),
            ok("connect", true),
            ok("tls", true),
            ok("http", true),
        ];
        assert_eq!(failed_step(&reached), None);
        // Il DNS fallisce e anche il resto: il primo passo fallito resta il DNS.
        assert_eq!(failed_step(&[ok("dns", false)]).as_deref(), Some("dns"));
        // Un guasto reale dopo il DNS riuscito viene segnalato.
        let tls = [ok("dns", true), ok("connect", true), ok("tls", false)];
        assert_eq!(failed_step(&tls).as_deref(), Some("tls"));
        // Il DNS fallisce, la connessione sull'IP riesce, ma JARVIS non risponde: il guasto è reale.
        let bad = [
            ok("dns", false),
            ok("connect", true),
            ok("tls", true),
            ok("http", false),
        ];
        assert_eq!(failed_step(&bad).as_deref(), Some("dns"));
    }

    #[test]
    fn the_tailscale_address_is_the_ipv4_in_the_cgnat_range() {
        let ips: Vec<String> = ["fd7a:115c::5", "192.168.1.9", "100.64.0.5"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            tailscale_ipv4(&ips),
            Some(std::net::Ipv4Addr::new(100, 64, 0, 5))
        );
        assert_eq!(
            tailscale_ipv4(&["100.128.0.1".to_string(), "10.0.0.1".to_string()]),
            None
        );
        assert_eq!(tailscale_ipv4(&[]), None);
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
        // La prova reale vince sul dato dichiarato (ADR-0230).
        let host = Some("casa.tail1.ts.net");
        // Il nome si risolve: se il JSON dice «MagicDNS disattivato» è sbagliato o in ritardo, e non si segnala.
        let works = advice(true, host, true, &details, Some(true));
        assert!(
            works.contains("raggiungibile") && !works.contains("risulta disattivato"),
            "{works}"
        );
        let ready = advice(true, host, false, &details, Some(true));
        assert!(
            ready.contains("Collega con Tailscale") && !ready.contains("disattivato"),
            "{ready}"
        );
        // Il nome non si risolve: il dato dichiarato diventa un indizio, con la cautela «se l'hai già attivato».
        let off = advice(true, host, false, &details, Some(false));
        assert!(
            off.contains("MagicDNS risulta disattivato") && off.contains("già attivato"),
            "{off}"
        );
        let no_https = Details {
            magic_dns: Some(true),
            https_enabled: Some(false),
            ..Default::default()
        };
        assert!(advice(true, host, false, &no_https, Some(false)).contains("HTTPS Certificates"));
        let fine = Details {
            magic_dns: Some(true),
            https_enabled: Some(true),
            ..Default::default()
        };
        let local = advice(true, host, true, &fine, Some(false));
        assert!(
            local.contains("pubblicato") && local.contains("Diagnostica"),
            "{local}"
        );
        assert!(advice(true, host, false, &fine, Some(true)).contains("Collega con Tailscale"));
        assert!(advice(true, host, true, &fine, Some(true)).contains("raggiungibile"));
        assert!(advice(false, None, false, &fine, None).contains("non è connesso"));
        // Nome assente: messaggio prudente, non un'accusa a MagicDNS.
        let nameless = advice(true, None, false, &details, None);
        assert!(nameless.contains("Se MagicDNS è già attivo"), "{nameless}");
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

    #[cfg(unix)]
    fn fake(dir: &std::path::Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    fn scratch() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jarvis-ts-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(unix)]
    #[test]
    fn a_large_status_does_not_block_the_client() {
        let d = scratch();
        // 400 KB su stdout: oltre il buffer della pipe, il vecchio codice restava bloccato fino al limite di tempo.
        let big = fake(
            &d,
            "big",
            "head -c 400000 /dev/zero | tr '\\0' 'a'; echo; exit 0",
        );
        let started = Instant::now();
        let out = run_with(&[big], &["status", "--json"]).unwrap().unwrap();
        assert!(out.ok && out.stdout.len() > 399_000);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn a_cli_that_cannot_reach_the_daemon_is_skipped_for_the_next_candidate() {
        let d = scratch();
        let wrong = fake(
            &d,
            "brew",
            r#"echo "failed to connect to local tailscaled; it doesn't appear to be running" >&2; exit 1"#,
        );
        let right = fake(
            &d,
            "app",
            r#"echo '{"BackendState":"Running","Self":{"DNSName":"casa.tail1.ts.net."}}'; echo 'Warning: client version 1.2 != server 1.3' >&2"#,
        );
        let missing = d.join("assente").to_string_lossy().into_owned();
        let out = run_with(&[missing.clone(), wrong.clone(), right], &["status"])
            .unwrap()
            .unwrap();
        assert!(out.ok, "{out:?}");
        // L'avviso su stderr non rovina il JSON.
        let (state, host) = parse_status(&json_part(&out));
        assert_eq!(
            (state.as_str(), host.as_deref()),
            ("Running", Some("casa.tail1.ts.net"))
        );
        // Se nessuno raggiunge il demone, resta l'ultimo esito con il motivo.
        let none = run_with(&[wrong], &["status"]).unwrap().unwrap();
        assert!(!none.ok && daemon_unreachable(&none.text()));
        // Nessun programma installato.
        assert!(run_with(&[missing], &["status"]).unwrap().is_none());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn json_is_extracted_around_warnings_and_unreachable_daemons_are_recognised() {
        let out = Out {
            ok: true,
            stdout: "Warning: x\n{\"BackendState\":\"Running\"}\ncoda".into(),
            stderr: String::new(),
        };
        assert_eq!(parse_status(&json_part(&out)).0, "Running");
        let plain = Out {
            ok: false,
            stdout: String::new(),
            stderr: "Tailscale is stopped.".into(),
        };
        assert_eq!(parse_status(&json_part(&plain)).0, "");
        assert!(daemon_unreachable(
            "failed to connect to local Tailscale service"
        ));
        assert!(!daemon_unreachable("Logged out."));
    }
}
