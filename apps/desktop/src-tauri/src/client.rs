use std::io::{BufRead, BufReader};
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
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
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

    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.request("GET", path, None)
    }

    pub fn post(&self, path: &str, body: Option<&Value>) -> Result<Value, String> {
        self.request("POST", path, body)
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

    pub fn chat_stream(&self, message: &str, mut delta: impl FnMut(&str)) -> Result<Value, String> {
        let mut request = self
            .agent
            .post(&self.url("/api/chat/stream"))
            .set("Accept", "application/x-ndjson")
            .set("Content-Type", "application/json");
        if let Some((name, value)) = &self.auth {
            request = request.set(name, value);
        }
        let response = request
            .send_json(json!({"message": message}))
            .map_err(read_error)?;
        let reader = BufReader::new(response.into_reader());
        for line in reader.lines() {
            let line = line.map_err(|error| error.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let event: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
            match event["type"].as_str() {
                Some("delta") => {
                    if let Some(text) = event["text"].as_str() {
                        delta(text);
                    }
                }
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
        Err("Risposta interrotta prima della fine".into())
    }
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
