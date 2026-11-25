use std::time::Duration;

use serde_json::{json, Value};
use url::Url;

#[derive(Clone)]
pub struct ApiClient {
    agent: ureq::Agent,
    base: Url,
    auth: Option<(&'static str, String)>,
}

impl ApiClient {
    pub fn owner(base: &str, token: String) -> Result<Self, String> {
        Self::new(base, Some(("X-Jarvis-Token", token)))
    }

    /// Sessione di un utente (ADR-0169): intestazione dedicata, mai quella del proprietario.
    pub fn user(base: &str, token: String) -> Result<Self, String> {
        Self::new(base, Some(("X-Jarvis-User", token)))
    }

    pub fn device(base: &str, token: String) -> Result<Self, String> {
        Self::new(base, Some(("X-Jarvis-Device", token)))
    }

    pub fn device_with_timeout(
        base: &str,
        token: String,
        timeout: Duration,
    ) -> Result<Self, String> {
        Self::new_with_timeout(base, Some(("X-Jarvis-Device", token)), timeout)
    }

    pub fn unauthenticated(base: &str) -> Result<Self, String> {
        Self::new(base, None)
    }

    fn new(base: &str, auth: Option<(&'static str, String)>) -> Result<Self, String> {
        Self::new_with_timeout(base, auth, Duration::from_secs(30))
    }

    fn new_with_timeout(
        base: &str,
        auth: Option<(&'static str, String)>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let mut base = Url::parse(base).map_err(|_| "Indirizzo del Core non valido")?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || !matches!(base.path(), "" | "/")
        {
            return Err("Indirizzo del Core non valido".into());
        }
        base.set_path("/");
        Ok(Self {
            agent: ureq::AgentBuilder::new()
                .timeout(timeout)
                .redirects(0)
                .build(),
            base,
            auth,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base.as_str().trim_end_matches('/'), path)
    }

    fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let mut request = self
            .agent
            .request(method, &self.url(path))
            .set("Accept", "application/json");
        if let Some((name, value)) = &self.auth {
            request = request.set(name, value);
        }
        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        }
        .map_err(read_error)?;
        response.into_json().map_err(|error| error.to_string())
    }

    /// Apre il flusso in tempo reale delle notifiche (Server-Sent Events, ADR-0098). Il Core manda un
    /// segnale ogni 20 secondi: se ne passano 45 senza nulla la connessione è considerata persa.
    pub fn notification_stream(&self) -> Result<Box<dyn std::io::BufRead + Send>, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(45))
            .redirects(0)
            .build();
        let mut request = agent
            .get(&self.url("/api/notifications/stream"))
            .set("Accept", "text/event-stream");
        if let Some((name, value)) = &self.auth {
            request = request.set(name, value);
        }
        let response = request.call().map_err(read_error)?;
        Ok(Box::new(std::io::BufReader::new(response.into_reader())))
    }

    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.request("GET", path, None)
    }

    pub fn post(&self, path: &str, body: Option<&Value>) -> Result<Value, String> {
        self.request("POST", path, body)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    /// Invia un file audio (WAV) e restituisce la risposta JSON del Core.
    pub fn post_audio(&self, path: &str, wav: &[u8]) -> Result<Value, String> {
        let mut request = self
            .agent
            .post(&self.url(path))
            .set("Accept", "application/json")
            .set("Content-Type", "audio/wav");
        if let Some((name, value)) = &self.auth {
            request = request.set(name, value);
        }
        request
            .send_bytes(wav)
            .map_err(read_error)?
            .into_json()
            .map_err(|error| error.to_string())
    }

    pub fn put(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.request("PUT", path, Some(body))
    }

    pub fn patch(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.request("PATCH", path, Some(body))
    }

    pub fn delete(&self, path: &str) -> Result<Value, String> {
        self.request("DELETE", path, None)
    }

    pub fn pair(&self, code: &str) -> Result<Value, String> {
        self.request(
            "POST",
            "/api/devices/pair",
            Some(&json!({
                "code": code,
                "name": "JARVIS Desktop",
                "platform": "desktop",
                "capabilities": []
            })),
        )
    }

    /// Chat cloud in streaming (ADR-0131): chiama `on_delta` per ogni frammento di testo e restituisce l'esito
    /// finale, lo stesso oggetto di `cloud_chat`. Se il flusso si interrompe prima dell'esito è un errore.
    pub fn cloud_chat_stream(
        &self,
        message: &str,
        use_context: bool,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<Value, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(125))
            .redirects(0)
            .build();
        let mut request = agent
            .post(&self.url("/api/cloud/chat/stream"))
            .set("Accept", "application/x-ndjson");
        if let Some((name, value)) = &self.auth {
            request = request.set(name, value);
        }
        let response = request
            .send_json(json!({
                "provider": "auto", "message": message, "use_context": use_context,
            }))
            .map_err(read_error)?;
        read_chat_stream(std::io::BufReader::new(response.into_reader()), on_delta)
    }

    pub fn test_cloud_key(&self, provider: &str) -> Result<Value, String> {
        self.cloud_post("/api/cloud/test", &json!({"provider": provider}))
    }

    fn cloud_post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let client = Self::new_with_timeout(
            self.base.as_str(),
            self.auth.clone(),
            Duration::from_secs(125),
        )?;
        client.post(path, Some(body))
    }
}

/// Legge le righe NDJSON della chat in streaming: `delta` (testo), `done` (esito) o `error`.
fn read_chat_stream(
    reader: impl std::io::BufRead,
    on_delta: &mut dyn FnMut(&str),
) -> Result<Value, String> {
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match event["type"].as_str() {
            Some("delta") => on_delta(event["text"].as_str().unwrap_or_default()),
            Some("done") => return Ok(event["result"].clone()),
            Some("error") => {
                return Err(event["error"]["message"]
                    .as_str()
                    .unwrap_or("Errore del Core")
                    .to_string())
            }
            _ => {}
        }
    }
    Err("Il flusso si è interrotto prima della risposta finale.".into())
}

fn read_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, response) => response
            .into_json::<Value>()
            .ok()
            .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("HTTP {status}")),
        ureq::Error::Transport(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_stream_delivers_fragments_then_the_final_result() {
        let raw = "{\"type\":\"delta\",\"text\":\"Ciao \"}\n\nnon json\n{\"type\":\"delta\",\"text\":\"mondo\"}\n{\"type\":\"done\",\"result\":{\"reply\":\"Ciao mondo\"}}\n";
        let mut pieces = Vec::new();
        let result = read_chat_stream(std::io::Cursor::new(raw), &mut |piece| {
            pieces.push(piece.to_string())
        })
        .unwrap();
        assert_eq!(pieces, ["Ciao ", "mondo"]);
        assert_eq!(result["reply"], "Ciao mondo");
        let failed = "{\"type\":\"error\",\"error\":{\"message\":\"Chiave non valida\"}}\n";
        assert_eq!(
            read_chat_stream(std::io::Cursor::new(failed), &mut |_| {}).unwrap_err(),
            "Chiave non valida"
        );
        let cut = "{\"type\":\"delta\",\"text\":\"x\"}\n";
        assert!(read_chat_stream(std::io::Cursor::new(cut), &mut |_| {}).is_err());
    }

    #[test]
    fn rejects_unsafe_core_origins() {
        for value in [
            "file:///tmp/jarvis",
            "https://user:password@example.test",
            "https://example.test/path",
            "https://example.test/?token=secret",
        ] {
            assert!(ApiClient::unauthenticated(value).is_err(), "{value}");
        }
    }

    #[test]
    fn keeps_an_origin_without_a_trailing_path() {
        let client = ApiClient::owner("https://core.example.test/", "token".into()).unwrap();
        assert_eq!(
            client.url("/api/health"),
            "https://core.example.test/api/health"
        );
    }
}

/// Legge un flusso Server-Sent Events e chiama `on_event(nome, dati)` per ogni evento completo. Si ferma
/// quando il flusso termina, in caso di errore di lettura, se `on_event` restituisce `false` o se `keep_going`
/// (controllato a ogni riga, segnali di vita compresi) restituisce `false`.
pub fn read_events<R: std::io::BufRead>(
    mut reader: R,
    keep_going: impl Fn() -> bool,
    mut on_event: impl FnMut(&str, &str) -> bool,
) {
    let (mut name, mut data) = (String::new(), String::new());
    let mut line = String::new();
    while keep_going() {
        line.clear();
        // Una riga senza fine non deve crescere senza limite.
        match std::io::BufRead::read_line(
            &mut std::io::Read::take(&mut reader, 256 * 1024),
            &mut line,
        ) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = line.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            if !name.is_empty() && !on_event(&name, &data) {
                return;
            }
            name.clear();
            data.clear();
        } else if let Some(value) = text.strip_prefix("event:") {
            name = value.trim().to_string();
        } else if let Some(value) = text.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.trim_start());
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use super::read_events;

    #[test]
    fn server_sent_events_are_split_and_comments_ignored() {
        let raw = "retry: 3000\nevent: ready\ndata: {}\n\n: ping\n\nid: 7\nevent: notification\ndata: {\"a\":1}\n\nevent: resync\ndata: {}\n\n";
        let mut seen = Vec::new();
        read_events(
            std::io::Cursor::new(raw),
            || true,
            |name, data| {
                seen.push((name.to_string(), data.to_string()));
                true
            },
        );
        assert_eq!(
            seen,
            [
                ("ready".to_string(), "{}".to_string()),
                ("notification".to_string(), "{\"a\":1}".to_string()),
                ("resync".to_string(), "{}".to_string()),
            ]
        );
        // Il chiamante può fermare la lettura.
        let mut count = 0;
        read_events(
            std::io::Cursor::new(raw),
            || true,
            |_, _| {
                count += 1;
                false
            },
        );
        assert_eq!(count, 1);
        // Un flusso ormai superato (nuova connessione) si ferma alla riga successiva, anche senza eventi.
        let mut events = 0;
        read_events(
            std::io::Cursor::new(raw),
            || false,
            |_, _| {
                events += 1;
                true
            },
        );
        assert_eq!(events, 0);
    }
}
