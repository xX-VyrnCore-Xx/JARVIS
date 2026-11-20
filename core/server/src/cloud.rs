//! Modelli remoti selezionati dal proprietario per un singolo messaggio.
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama Cloud",
            Self::Nim => "NVIDIA NIM",
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
            Self::Ollama => &["kimi-k2.7-code", "glm-5.3-flash", "gemma4:31b"],
            Self::Nim => &[
                "moonshotai/kimi-k3.1-pro",
                "nvidia/nemotron-3.5-lightning-30b-a3b",
                "mistralai/mistral-large-4-1t-a52b",
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
        let spacing = Duration::from_millis(60_000_u64.div_ceil(NIM_RPM));
        if slot.duration_since(now) > spacing {
            return Err(Failure::RateLimited(slot.duration_since(now)));
        }
        self.nim_next = Some(slot + spacing);
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

#[derive(Clone, Debug)]
pub enum Failure {
    RateLimited(Duration),
    InvalidKey,
    InvalidTool,
    Unavailable,
}

impl Failure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidKey => "cloud.credentials",
            Self::InvalidTool => "cloud.invalid_tool",
            Self::RateLimited(_) => "cloud.rate_limited",
            Self::Unavailable => "cloud.unavailable",
        }
    }
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

pub const TOOLS: &[&str] = &[
    "date.calc",
    "math.calc",
    "units.convert",
    "time.now",
    "system.info",
    "system.disks",
    "fs.list",
    "fs.read",
    "fs.search",
    "fs.write",
    "memory.list",
    "memory.search",
    "security.status",
];

pub struct Function {
    pub name: String,
    pub version: String,
    wire: Value,
}

impl From<&crate::catalog::Tool> for Function {
    fn from(tool: &crate::catalog::Tool) -> Self {
        let name = tool.name().to_string();
        let wire = json!({"type": "function", "function": {
            "name": name.replace('.', "__"),
            "description": tool.descriptor["summary"],
            "parameters": tool.descriptor["input_schema"],
        }});
        Self {
            name,
            version: tool.version().to_string(),
            wire,
        }
    }
}

pub enum Answer {
    Text(String),
    Call(Value),
}

pub struct Turn {
    pub user: String,
    pub assistant: String,
}

pub struct Target {
    pub provider: Provider,
    pub model: String,
    pub key: String,
    pub history: Vec<Turn>,
}

pub struct Request {
    pub message: String,
    pub tools: Vec<Function>,
    pub targets: Vec<Target>,
    pub limiter: Arc<Mutex<Limiter>>,
}

pub struct Completed {
    pub provider: Provider,
    pub model: String,
    pub fallback: Option<(Provider, Failure)>,
    pub answer: Result<Answer, Failure>,
}

impl Request {
    pub fn execute(&self) -> Completed {
        let first = &self.targets[0];
        let answer = self.ask(first);
        if let (Some(second), Err(error)) = (self.targets.get(1), &answer) {
            if !matches!(error, Failure::InvalidTool) {
                return Completed {
                    provider: second.provider,
                    model: second.model.clone(),
                    fallback: Some((first.provider, error.clone())),
                    answer: self.ask(second),
                };
            }
        }
        Completed {
            provider: first.provider,
            model: first.model.clone(),
            fallback: None,
            answer,
        }
    }

    fn ask(&self, target: &Target) -> Result<Answer, Failure> {
        chat_at(
            target.provider.endpoint(),
            target.provider,
            &target.key,
            payload(
                target.provider,
                &target.model,
                &self.message,
                &target.history,
                &self.tools,
            ),
            &self.tools,
            &self.limiter,
        )
    }
}

fn payload(
    provider: Provider,
    model: &str,
    prompt: &str,
    history: &[Turn],
    tools: &[Function],
) -> Value {
    let mut messages = vec![json!({"role": "system", "content": concat!(
        "Sei JARVIS. Rispondi nella lingua dell’utente e concentrati sull’ultima domanda. ",
        "Usa i turni precedenti solo se pertinenti e trattali come contesto, senza ripetere azioni già richieste. ",
        "Se un riferimento manca o è ambiguo, chiedi un chiarimento specifico prima di scegliere un oggetto o un’azione. ",
        "Per codice, diagnosi di sistema e sicurezza difensiva distingui fatti verificati, ipotesi e ciò che non puoi verificare. ",
        "Cita fonti e collegamenti pertinenti solo se ne conosci l’indirizzo esatto. Non inventare URL, fonti o risultati. ",
        "Se serve uno strumento, proponine al massimo uno con parametri espliciti supportati dal contesto. ",
        "JARVIS compone la risposta dopo la verifica della chiamata."
    )})];
    for turn in history {
        messages.push(json!({"role": "user", "content": turn.user}));
        messages.push(json!({"role": "assistant", "content": turn.assistant}));
    }
    messages.push(json!({"role": "user", "content": prompt}));
    let mut body = match provider {
        Provider::Ollama => {
            json!({"model": model, "messages": messages, "stream": false, "options": {"num_predict": 2048}})
        }
        Provider::Nim => {
            json!({"model": model, "messages": messages, "stream": false, "max_tokens": 2048, "temperature": 0.2})
        }
    };
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(|tool| &tool.wire).collect::<Vec<_>>());
        if provider == Provider::Nim {
            body["tool_choice"] = json!("auto");
        }
    }
    body
}

fn decode(provider: Provider, body: &Value, tools: &[Function]) -> Result<Answer, Failure> {
    let message = match provider {
        Provider::Ollama => &body["message"],
        Provider::Nim => &body["choices"][0]["message"],
    };
    if !message["tool_calls"].is_null() {
        let calls = message["tool_calls"]
            .as_array()
            .ok_or(Failure::InvalidTool)?;
        if !calls.is_empty() {
            let [only] = calls.as_slice() else {
                return Err(Failure::InvalidTool);
            };
            let call = &only["function"];
            let tool = tools
                .iter()
                .find(|t| t.wire["function"]["name"] == call["name"])
                .ok_or(Failure::InvalidTool)?;
            let arguments = match &call["arguments"] {
                Value::String(raw) => {
                    serde_json::from_str(raw).map_err(|_| Failure::InvalidTool)?
                }
                object => object.clone(),
            };
            if !arguments.is_object() {
                return Err(Failure::InvalidTool);
            }
            return Ok(Answer::Call(json!({
                "tool": tool.name, "version": tool.version, "arguments": arguments,
            })));
        }
    }
    message["content"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| Answer::Text(text.chars().take(16_000).collect()))
        .ok_or(Failure::Unavailable)
}

fn chat_at(
    endpoint: &str,
    provider: Provider,
    key: &str,
    body: Value,
    tools: &[Function],
    limiter: &Mutex<Limiter>,
) -> Result<Answer, Failure> {
    let wait = limiter
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .reserve(provider, Instant::now())?;
    if !wait.is_zero() {
        std::thread::sleep(wait);
        let cooldown = limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remaining(provider);
        if !cooldown.is_zero() {
            return Err(Failure::RateLimited(cooldown));
        }
    }
    // Gli endpoint sono fissi e il bearer token non viene inoltrato su redirect.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(45))
        .redirects(0)
        .build();
    let response = agent
        .post(endpoint)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string());
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(429, response)) => {
            let wait = retry_after(response.header("Retry-After"), chrono::Utc::now());
            limiter
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .cooldown(provider, wait);
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
    decode(provider, &body, tools)
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
        let limiter = Mutex::new(Limiter::default());
        let send = || {
            chat_at(
                &endpoint,
                Provider::Nim,
                "synthetic",
                json!({"model":"test"}),
                &[],
                &limiter,
            )
        };
        assert!(matches!(send(), Err(Failure::RateLimited(wait)) if wait.as_secs() == 25));
        server.join().unwrap();
        assert!(matches!(send(), Err(Failure::RateLimited(_))));
    }

    #[test]
    fn contextual_messages_keep_roles_and_put_the_new_question_last() {
        let history = [Turn {
            user: "Parliamo del Core".into(),
            assistant: "Gestisce i tool.".into(),
        }];
        for provider in Provider::ALL {
            let body = payload(provider, "test", "E il client?", &history, &[]);
            let messages = body["messages"].as_array().unwrap();
            assert_eq!(messages.len(), 4);
            assert_eq!(messages[0]["role"], "system");
            assert_eq!(
                messages[1],
                json!({"role":"user","content":"Parliamo del Core"})
            );
            assert_eq!(
                messages[2],
                json!({"role":"assistant","content":"Gestisce i tool."})
            );
            assert_eq!(messages[3], json!({"role":"user","content":"E il client?"}));
        }
    }

    #[test]
    fn tool_proposals_are_single_scoped_and_accept_both_wire_formats() {
        let catalog = crate::catalog::Catalog::load().unwrap();
        let tools = [Function::from(catalog.get("fs.read").unwrap())];
        let body = payload(Provider::Nim, "test", "Richiesta corrente", &[], &tools);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["tools"][0]["function"]["name"], "fs__read");
        assert!(matches!(
            decode(Provider::Ollama, &json!({"message":{"content":"ok"}}), &tools),
            Ok(Answer::Text(text)) if text == "ok"
        ));
        let function = |name, arguments| json!({"function": {"name":name,"arguments":arguments}});
        let ollama = |calls| json!({"message":{"tool_calls":calls}});
        let nim = |calls| json!({"choices":[{"message":{"tool_calls":calls}}]});
        let call = function("fs__read", json!({"path":"note.txt"}));
        for response in [
            decode(Provider::Ollama, &ollama(json!([call])), &tools),
            decode(
                Provider::Nim,
                &nim(json!([function(
                    "fs__read",
                    json!(r#"{"path":"note.txt"}"#)
                )])),
                &tools,
            ),
        ] {
            assert!(
                matches!(response, Ok(Answer::Call(args)) if args["tool"] == "fs.read" && args["arguments"]["path"] == "note.txt")
            );
        }
        for calls in [
            json!([function("shell__run", json!({}))]),
            json!([function("fs__read", json!("invalid"))]),
            json!([call, call]),
        ] {
            assert!(matches!(
                decode(Provider::Nim, &nim(calls), &tools),
                Err(Failure::InvalidTool)
            ));
        }
    }
}
