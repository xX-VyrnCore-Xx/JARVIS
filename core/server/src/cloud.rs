//! Modelli remoti selezionati dal proprietario per un singolo messaggio.
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const NIM_RPM: u64 = 35;
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Ollama,
    Nim,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::Ollama, Self::Nim];

    pub fn parse(value: &Value) -> Option<Self> {
        match value.as_str()? {
            "ollama_cloud" => Some(Self::Ollama),
            "nvidia_nim" => Some(Self::Nim),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Ollama => "ollama_cloud",
            Self::Nim => "nvidia_nim",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::Ollama => "https://ollama.com/api/chat",
            Self::Nim => "https://integrate.api.nvidia.com/v1/chat/completions",
        }
    }

    fn models(self) -> &'static [&'static str] {
        match self {
            Self::Ollama => &["kimi-k2.7-code", "deepseek-v4-pro:0813", "gemma4:31b"],
            Self::Nim => &[
                "qwen/qwen3-coder-480b-a35b-instruct",
                "nvidia/nemotron-3-super-120b-a12b",
                "meta/llama-3.3-70b-instruct",
            ],
        }
    }

    pub fn secret(self) -> String {
        format!("cloud:{}", self.id())
    }

    pub fn model(self, settings: &Value) -> &str {
        settings[self.id()].as_str().unwrap_or(self.models()[0])
    }

    pub fn view(self, settings: &Value, configured: bool, limiter: &Limiter) -> Value {
        json!({
            "id": self.id(), "model": self.model(settings), "models": self.models(),
            "configured": configured,
            "effective_rpm": (self == Self::Nim).then_some(NIM_RPM),
            "cooldown_seconds": limiter.remaining(self).as_secs_f64().ceil() as u64,
        })
    }
}

pub fn settings(dir: &Path) -> Value {
    std::fs::read(dir.join("cloud.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 160
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:+".contains(&b))
}

/// La chiave è condivisa tra i modelli NIM. Distanziamo i tentativi e rispettiamo Retry-After.
#[derive(Default)]
pub struct Limiter {
    nim_next: Option<Instant>,
    nim_cooldown: Option<Instant>,
    ollama_cooldown: Option<Instant>,
}

impl Limiter {
    fn deadline(&self, provider: Provider) -> Option<Instant> {
        match provider {
            Provider::Ollama => self.ollama_cooldown,
            Provider::Nim => self.nim_cooldown,
        }
    }

    fn remaining(&self, provider: Provider) -> Duration {
        self.deadline(provider).map_or(Duration::ZERO, |until| {
            until.saturating_duration_since(Instant::now())
        })
    }

    fn reserve(&mut self, provider: Provider, now: Instant) -> Result<Duration, Failure> {
        if let Some(until) = self.deadline(provider).filter(|until| *until > now) {
            return Err(Failure::RateLimited(until.duration_since(now)));
        }
        if provider != Provider::Nim {
            return Ok(Duration::ZERO);
        }
        let slot = self.nim_next.unwrap_or(now).max(now);
        self.nim_next = Some(slot + Duration::from_millis(60_000_u64.div_ceil(NIM_RPM)));
        Ok(slot.duration_since(now))
    }

    fn cooldown(&mut self, provider: Provider, wait: Duration) {
        let target = Some(Instant::now() + wait);
        match provider {
            Provider::Ollama => self.ollama_cooldown = target,
            Provider::Nim => self.nim_cooldown = target,
        }
    }
}

#[derive(Debug)]
pub enum Failure {
    RateLimited(Duration),
    InvalidKey,
    Unavailable,
}

fn retry_after(raw: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> Duration {
    let seconds = raw.and_then(|s| {
        s.parse().ok().or_else(|| {
            chrono::DateTime::parse_from_rfc2822(s).ok().map(|date| {
                (date.with_timezone(&chrono::Utc) - now)
                    .num_seconds()
                    .max(0) as u64
            })
        })
    });
    Duration::from_secs(seconds.unwrap_or(60).clamp(1, 86_400))
}

fn payload(provider: Provider, model: &str, prompt: &str) -> Value {
    let messages = json!([
        {"role": "system", "content": "Sei JARVIS. Rispondi nella lingua dell’utente. Aiuta a programmare, verificare codice, diagnosticare sistemi e migliorare la sicurezza difensiva. Non puoi eseguire strumenti. Distingui le ipotesi dai risultati osservati."},
        {"role": "user", "content": prompt},
    ]);
    match provider {
        Provider::Ollama => {
            json!({"model": model, "messages": messages, "stream": false, "options": {"num_predict": 2048}})
        }
        Provider::Nim => {
            json!({"model": model, "messages": messages, "stream": false, "max_tokens": 2048, "temperature": 0.2})
        }
    }
}

fn decode(provider: Provider, body: &Value) -> Option<String> {
    let text = match provider {
        Provider::Ollama => body["message"]["content"].as_str(),
        Provider::Nim => body["choices"][0]["message"]["content"].as_str(),
    }?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.chars().take(16_000).collect())
}

pub fn chat(
    provider: Provider,
    model: &str,
    key: &str,
    prompt: &str,
    limiter: &mut Limiter,
) -> Result<String, Failure> {
    chat_at(provider.endpoint(), provider, model, key, prompt, limiter)
}

fn chat_at(
    endpoint: &str,
    provider: Provider,
    model: &str,
    key: &str,
    prompt: &str,
    limiter: &mut Limiter,
) -> Result<String, Failure> {
    let wait = limiter.reserve(provider, Instant::now())?;
    std::thread::sleep(wait);
    // Gli endpoint sono fissi e il bearer token non viene inoltrato su redirect.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(45))
        .redirects(0)
        .build();
    let response = agent
        .post(endpoint)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .send_string(&payload(provider, model, prompt).to_string());
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(429, response)) => {
            let wait = retry_after(response.header("Retry-After"), chrono::Utc::now());
            limiter.cooldown(provider, wait);
            return Err(Failure::RateLimited(wait));
        }
        Err(ureq::Error::Status(401 | 403, _)) => return Err(Failure::InvalidKey),
        Err(_) => return Err(Failure::Unavailable),
    };
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Unavailable)?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(Failure::Unavailable);
    }
    let body = serde_json::from_slice(&bytes).map_err(|_| Failure::Unavailable)?;
    decode(provider, &body).ok_or(Failure::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacing_respects_the_limit_and_keeps_provider_cooldowns_separate() {
        let mut limiter = Limiter::default();
        let now = Instant::now();
        assert_eq!(limiter.reserve(Provider::Nim, now).unwrap(), Duration::ZERO);
        let next = limiter.reserve(Provider::Nim, now).unwrap();
        assert!(next > Duration::from_millis(1700));
        limiter.cooldown(Provider::Nim, Duration::from_secs(20));
        assert!(matches!(
            limiter.reserve(Provider::Nim, now),
            Err(Failure::RateLimited(_))
        ));
        assert_eq!(
            limiter.reserve(Provider::Ollama, now).unwrap(),
            Duration::ZERO
        );
        assert_eq!(retry_after(Some("42"), chrono::Utc::now()).as_secs(), 42);
    }

    #[test]
    fn rate_limited_response_prevents_a_second_http_request_until_cooldown() {
        use std::io::Write;
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", socket.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            use std::io::BufRead;
            let mut conn = socket.accept().unwrap().0;
            let reader = std::io::BufReader::new(&mut conn);
            let mut authorized = false;
            for line in reader.lines() {
                let line = line.unwrap();
                if line.is_empty() {
                    break;
                }
                authorized |= line.eq_ignore_ascii_case("authorization: bearer synthetic");
            }
            assert!(authorized);
            write!(conn, "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 25\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let mut limiter = Limiter::default();
        assert!(
            matches!(chat_at(&endpoint, Provider::Nim, "m", "synthetic", "test", &mut limiter),
            Err(Failure::RateLimited(wait)) if wait.as_secs() == 25)
        );
        server.join().unwrap();
        assert!(matches!(
            chat_at(
                &endpoint,
                Provider::Nim,
                "m",
                "synthetic",
                "test",
                &mut limiter
            ),
            Err(Failure::RateLimited(_))
        ));
    }

    #[test]
    fn wire_formats_only_contain_the_current_message_and_decode_responses() {
        let body = payload(Provider::Ollama, "test", "Richiesta corrente");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert!(body.get("tools").is_none());
        assert_eq!(
            decode(Provider::Ollama, &json!({"message":{"content":"ok"}})).as_deref(),
            Some("ok")
        );
        assert_eq!(
            decode(
                Provider::Nim,
                &json!({"choices":[{"message":{"content":"ok"}}]})
            )
            .as_deref(),
            Some("ok")
        );
    }
}
