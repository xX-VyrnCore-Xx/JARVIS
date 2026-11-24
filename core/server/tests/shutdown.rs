//! Arresto del processo (ADR-0103): quando il padre chiude lo stdin il Core deve uscire da solo, con codice 0,
//! anche dopo aver servito richieste, senza lasciare il worker Python orfano. Una prova di installazione su
//! macOS aveva mostrato un Core ancora vivo 15 secondi dopo «JARVIS arrestato».

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PORT: u16 = 18798;
const TOKEN: &str = "tokendiarresto0123456789abcdefghijklmnop";

fn http(method: &str, path: &str, body: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", PORT)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .ok()?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nX-Jarvis-Token: {TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .ok()?;
    let mut text = String::new();
    stream.read_to_string(&mut text).ok()?;
    Some(text)
}

#[test]
fn the_core_exits_promptly_when_its_parent_closes_stdin() {
    let data =
        std::env::temp_dir().join(format!("jarvis-shutdown-{}", uuid::Uuid::new_v4().simple()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_jarvis"))
        .env("JARVIS_DATA_DIR", &data)
        .env("JARVIS_PORT", PORT.to_string())
        .env("JARVIS_TOKEN", TOKEN)
        .env("JARVIS_EXIT_WITH_PARENT", "stdin")
        .env("JARVIS_VOICE_ENGINE", "fake")
        .env("JARVIS_SECRETS_KEYCHAIN", "off")
        .env("JARVIS_NO_BROWSER", "1")
        .env(
            "JARVIS_PYTHON",
            std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        )
        .env(
            "JARVIS_AI_DIR",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../ai"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("avvio del Core");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(format!("{TOKEN}\n").as_bytes())
        .unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();

    let deadline = Instant::now() + Duration::from_secs(40);
    while http("GET", "/api/health", "").is_none_or(|r| !r.starts_with("HTTP/1.1 200")) {
        assert!(Instant::now() < deadline, "il Core non è partito");
        assert!(
            child.try_wait().unwrap().is_none(),
            "il Core si è chiuso all'avvio"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    // Una richiesta che avvia il worker Python: è il caso in cui restavano processi attivi.
    let reply =
        http("POST", "/api/chat", r#"{"message":"che ore sono"}"#).expect("risposta della chat");
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");

    let closed = Instant::now();
    drop(child.stdin.take());
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if closed.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("il Core è ancora vivo 10 secondi dopo la chiusura dello stdin");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
    let _ = std::fs::remove_dir_all(data);
}
