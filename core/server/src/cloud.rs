//! Selezione locale del modello remoto per ciascun messaggio autorizzato.
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const NIM_RPM: u64 = 35;
const MODEL_RECHECK: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Elenco dei modelli accessibili con la chiave: serve a verificare le credenziali senza consumare una richiesta di chat.
    fn models_endpoint(self) -> &'static str {
        match self {
            Self::Ollama => "https://ollama.com/api/tags",
            Self::Nim => "https://integrate.api.nvidia.com/v1/models",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::Ollama => "https://ollama.com/api/chat",
            Self::Nim => "https://integrate.api.nvidia.com/v1/chat/completions",
        }
    }

    fn model(self, route: Route) -> &'static str {
        match (self, route) {
            (Self::Ollama, Route::Fast) => "glm-5.3-flash",
            (Self::Ollama, Route::Reasoning) => "deepseek-v4-pro:0813",
            (Self::Ollama, Route::Long) => "kimi-k3.1-pro",
            (Self::Ollama, Route::Coding) => "kimi-k2.7-code",
            (Self::Nim, Route::Fast) => "nvidia/nemotron-3.5-lightning-30b-a3b",
            (Self::Nim, Route::Reasoning) => "nvidia/nemotron-3-ultra-550b-a55b",
            (Self::Nim, Route::Long) => "moonshotai/kimi-k3.1-pro",
            (Self::Nim, Route::Coding) => "mistralai/mistral-large-4-1t-a52b",
        }
    }

    pub fn secret(self) -> String {
        format!("cloud:{}", self.id())
    }

    pub fn view(self, configured: bool, limiter: &Limiter) -> Value {
        json!({
            "id": self.id(),
            "routing": {
                "fast": self.model(Route::Fast),
                "reasoning": self.model(Route::Reasoning),
                "coding": self.model(Route::Coding),
                "long": self.model(Route::Long)
            },
            "configured": configured,
            "effective_rpm": (self == Self::Nim).then_some(NIM_RPM),
            "cooldown_seconds": limiter.remaining(self).as_secs_f64().ceil() as u64,
        })
    }

    pub fn target(self, key: String, prompt: &str, history: Vec<Turn>, settings: &Value) -> Target {
        let route = Route::select(prompt, &history);
        let default = self.model(route);
        let configured = settings[self.id()]
            .as_str()
            .filter(|value| valid_model(value));
        let model = configured
            .map(|value| {
                if self == Self::Ollama {
                    value.strip_suffix(":cloud").unwrap_or(value)
                } else {
                    value
                }
            })
            .unwrap_or(default);
        let alternative = if model != default {
            default
        } else if route == Route::Fast {
            match self {
                Self::Ollama => "minimax-m3.1-flash",
                Self::Nim => "minimaxai/minimax-m3.1-flash",
            }
        } else {
            self.model(Route::Fast)
        };
        Target {
            provider: self,
            model: model.to_string(),
            alternative: alternative.to_string(),
            key,
            history,
            route,
        }
    }
}

/// Scelte esplicite del proprietario. Al provider arrivano solo istruzioni predefinite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QualityPreference {
    length: LengthPreference,
    precision: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LengthPreference {
    #[default]
    Standard,
    Concise,
    Detailed,
}

impl QualityPreference {
    pub(crate) fn from_counts(positive: u32, concise: u32, detailed: u32, precise: u32) -> Self {
        let length = if concise > detailed.max(positive) {
            LengthPreference::Concise
        } else if detailed > concise.max(positive) {
            LengthPreference::Detailed
        } else {
            LengthPreference::Standard
        };
        Self {
            length,
            precision: precise > positive,
        }
    }

    pub(crate) fn length_id(self) -> &'static str {
        match self.length {
            LengthPreference::Standard => "standard",
            LengthPreference::Concise => "concise",
            LengthPreference::Detailed => "detailed",
        }
    }

    pub(crate) fn emphasizes_precision(self) -> bool {
        self.precision
    }

    fn length_guidance(self) -> &'static str {
        match self.length {
            LengthPreference::Standard => "",
            LengthPreference::Concise => "Se la richiesta non indica un formato, rispondi prima con la conclusione e limita le ripetizioni. ",
            LengthPreference::Detailed => "Se la richiesta non indica un formato, aggiungi passaggi verificabili e un esempio quando pertinente. ",
        }
    }

    fn precision_guidance(self) -> &'static str {
        if self.precision {
            "Dichiara le ipotesi decisive e separa ciò che è verificato da ciò che richiede informazioni aggiuntive."
        } else {
            ""
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Route {
    Fast,
    Reasoning,
    Coding,
    Long,
}

impl Route {
    fn select(prompt: &str, history: &[Turn]) -> Self {
        // Le richieste ordinarie vanno al modello veloce (ADR-0130): quelli di ragionamento sono molto più lenti.
        // La complessità si misura sulla richiesta, non sulla cronologia: «continua» dopo un lungo scambio
        // non richiede un modello da 550 miliardi di parametri. La cronologia conta solo per il contesto lungo.
        let prompt_tokens = estimated_tokens(prompt);
        let tokens = prompt_tokens
            + history
                .iter()
                .map(|turn| estimated_tokens(&turn.user) + estimated_tokens(&turn.assistant))
                .sum::<usize>();
        let text = prompt.to_lowercase();
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        let needs_steps = [
            "confronta",
            "progetta",
            "analizza",
            "diagnosi",
            "architettura",
            "trade-off",
            "passo per passo",
            "compare",
            "root cause",
            "migration",
        ]
        .iter()
        .any(|term| text.contains(term));
        let business_topic =
            crate::agents::route(prompt).is_some_and(|agent| agent.name == "business");
        let complex = prompt_tokens >= 420
            || (business_topic && prompt_tokens >= 80)
            || needs_steps
            || prompt
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
                >= 8;
        let coding_topic = crate::agents::route(prompt).is_some_and(|agent| agent.name == "coding");
        let coding_task = text.contains("```")
            || [
                "implementa",
                "debug",
                "stack trace",
                "review del codice",
                "refactor",
                "write code",
                "unit tests",
            ]
            .iter()
            .any(|term| text.contains(term))
            || (coding_topic
                && (complex
                    || ["scrivi", "correggi", "modifica", "genera", "verifica"]
                        .iter()
                        .any(|term| text.contains(term))));
        // «prima» e «poi» compaiono in frasi qualsiasi («prima di», «poi dimmi»): contano solo parole di sequenza
        // vere e proprie, almeno due.
        let multiple_steps = [
            "poi",
            "infine",
            "dopodiché",
            "successivamente",
            "then",
            "finally",
        ]
        .iter()
        .filter(|term| words.contains(term))
        .count()
            >= 2;
        if coding_task {
            Self::Coding
        } else if tokens >= 3_200 {
            Self::Long
        } else if complex || multiple_steps {
            Self::Reasoning
        } else {
            Self::Fast
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Reasoning => "reasoning",
            Self::Coding => "coding",
            Self::Long => "long",
        }
    }

    fn guidance(self) -> &'static str {
        match self {
            Self::Fast => "Rispondi in modo diretto e completo: prima la risposta concreta, poi solo i dettagli che servono (valori, nomi, comandi, passaggi, un esempio). Niente panoramiche generiche. Per un dato attuale usa uno strumento pertinente, se disponibile.",
            Self::Reasoning => "Organizza i requisiti prima di rispondere. Confronta le opzioni sulla base dei vincoli espliciti e indica le verifiche mancanti.",
            Self::Long => "Individua le informazioni pertinenti nel contesto lungo e mantieni i riferimenti ai passaggi originali.",
            Self::Coding => "Verifica linguaggio, interfacce e vincoli di compatibilità. Se serve il repository, chiedi il file o usa gli strumenti concessi prima di suggerire modifiche.",
        }
    }

    fn output_tokens(self) -> usize {
        match self {
            Self::Fast => 1536,
            Self::Reasoning => 3072,
            Self::Coding => 6144,
            Self::Long => 6144,
        }
    }
}

pub fn settings(dir: &Path) -> Value {
    std::fs::read(dir.join("cloud.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// Riconosce formati di segreti comuni e intestazioni di autenticazione.
pub fn contains_credentials(text: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "nvapi-",
        "sk-",
        "ghp_",
        "github_pat_",
        "xoxb-",
        "AIza",
        "GOCSPX-",
        "ya29.",
        "1//",
    ];
    text.contains("PRIVATE KEY-----")
        || text.lines().any(|line| {
            let lower = line.trim_start().to_ascii_lowercase();
            lower.starts_with("authorization:")
                || lower.contains("password=")
                || lower.contains("api_key=")
                || lower.contains("api-key:")
                || lower.contains(r#""password":"#)
                || lower.contains(r#""api_key":"#)
                || ["client_secret", "access_token", "refresh_token"]
                    .iter()
                    .any(|field| {
                        lower.contains(&format!("{field}="))
                            || lower.contains(&format!("\"{field}\":"))
                    })
                || line
                    .split(|c: char| {
                        !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
                    })
                    .any(|part| {
                        PREFIXES.iter().any(|prefix| {
                            part.starts_with(prefix) && part.len() >= prefix.len() + 16
                        })
                    })
        })
}

/// Stima prudenziale indipendente dal tokenizer del provider, con margine per ruoli e messaggi.
pub fn estimated_tokens(text: &str) -> usize {
    let mut cost = 8;
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || (ch.is_alphabetic() && ch.len_utf8() == 2) {
            run += ch.len_utf8();
        } else {
            cost += run.div_ceil(3);
            run = 0;
            cost += if matches!(ch, ' ' | '\t') {
                0
            } else {
                ch.len_utf8().div_ceil(2)
            };
        }
    }
    (cost + run.div_ceil(3)).max(8 + text.len().div_ceil(4))
}

/// Limita un testo su confini UTF-8 conservando inizio e fine, senza ricomporre dati fra provider.
pub fn clip_tokens(text: &str, budget: usize) -> String {
    if estimated_tokens(text) <= budget {
        return text.to_owned();
    }
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut low = 0;
    let mut high = boundaries.len() / 2;
    while low < high {
        let count = (low + high).div_ceil(2);
        let candidate = format!(
            "{}\n[…]\n{}",
            &text[..boundaries[count]],
            &text[boundaries[boundaries.len() - 1 - count]..]
        );
        if estimated_tokens(&candidate) <= budget {
            low = count;
        } else {
            high = count - 1;
        }
    }
    if low == 0 {
        String::new()
    } else {
        format!(
            "{}\n[…]\n{}",
            &text[..boundaries[low]],
            &text[boundaries[boundaries.len() - 1 - low]..]
        )
    }
}

/// Ripulisce una chiave incollata (ADR-0119): spazi e a capo invisibili, virgolette, `Bearer `, `Authorization:`,
/// `export NOME=`/`NOME=`. Una chiave con spazi interni o caratteri non ASCII viene rifiutata con il motivo.
pub fn normalize_key(raw: &str) -> Result<String, &'static str> {
    let mut text: String = raw
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}'
            )
        })
        .collect();
    text = text.trim().to_string();
    let lower = text.to_ascii_lowercase();
    for prefix in ["authorization:", "export "] {
        if lower.starts_with(prefix) {
            text = text[prefix.len()..].trim().to_string();
        }
    }
    // NOME_VARIABILE=valore
    if let Some((name, value)) = text.split_once('=') {
        if !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            text = value.trim().to_string();
        }
    }
    text = text
        .trim_matches(|c| matches!(c, '"' | '\'' | '`' | '\u{201c}' | '\u{201d}'))
        .trim()
        .to_string();
    if text.to_ascii_lowercase().starts_with("bearer ") {
        text = text[7..].trim().to_string();
    }
    if text.is_empty() {
        return Err("La chiave è vuota.");
    }
    if text.chars().any(char::is_whitespace) {
        return Err(
            "La chiave contiene spazi o a capo: incolla solo la chiave, senza altro testo.",
        );
    }
    if !text.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(
            "La chiave contiene caratteri non validi (accenti o simboli copiati per errore).",
        );
    }
    if text.len() < 8 {
        return Err("La chiave è troppo corta: controlla di averla copiata per intero.");
    }
    if text.len() > 1024 {
        return Err("La chiave è troppo lunga.");
    }
    Ok(text)
}

/// Avviso, non errore, se la chiave non ha il formato tipico del provider.
pub fn key_format_hint(provider: Provider, key: &str) -> Option<&'static str> {
    match provider {
        Provider::Nim if !key.starts_with("nvapi-") => {
            Some("Le chiavi NVIDIA iniziano di solito con «nvapi-»: se il test fallisce, rigenerala da build.nvidia.com.")
        }
        Provider::Ollama if key.starts_with("nvapi-") || key.starts_with("sk-") => {
            Some("Questa sembra la chiave di un altro servizio: per Ollama Cloud usa una chiave creata su ollama.com/settings/keys.")
        }
        _ => None,
    }
}

/// Modelli accessibili con la chiave. Un errore di credenziali o di rete è già classificato come per la chat.
pub fn list_models(provider: Provider, key: &str) -> Result<Vec<String>, Failure> {
    list_models_at(provider.models_endpoint(), provider, key)
}

pub(crate) fn list_models_at(
    endpoint: &str,
    provider: Provider,
    key: &str,
) -> Result<Vec<String>, Failure> {
    let agent = crate::net::builder(Duration::from_secs(20))
        .try_proxy_from_env(endpoint.starts_with("https://"))
        .build();
    let mut attempt = 0;
    let response = loop {
        let result = agent
            .get(endpoint)
            .set("Authorization", &format!("Bearer {key}"))
            .set("Accept", "application/json")
            .call();
        match &result {
            Err(ureq::Error::Transport(t))
                if attempt < CONNECT_RETRIES && crate::net::classify(t).transient() =>
            {
                std::thread::sleep(Duration::from_millis(400 * 3u64.pow(attempt)));
                attempt += 1;
            }
            _ => break result,
        }
    };
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(401, _)) => return Err(Failure::InvalidKey),
        Err(ureq::Error::Status(403, _)) => return Err(Failure::Forbidden),
        Err(ureq::Error::Status(402, _)) => return Err(Failure::Quota),
        Err(ureq::Error::Status(429, response)) => {
            return Err(Failure::RateLimited(retry_after(
                response.header("Retry-After"),
                chrono::Utc::now(),
            )))
        }
        Err(ureq::Error::Status(status, _)) if status < 500 => {
            return Err(Failure::Rejected(status))
        }
        Err(ureq::Error::Status(_, _)) => return Err(Failure::Unavailable),
        Err(ureq::Error::Transport(t)) => {
            return Err(Failure::Connection(crate::net::classify(&t)))
        }
    };
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Unavailable)?;
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| Failure::Unavailable)?;
    Ok(parse_models(provider, &body))
}

fn parse_models(provider: Provider, body: &Value) -> Vec<String> {
    let (list, field) = match provider {
        Provider::Ollama => (&body["models"], "name"),
        Provider::Nim => (&body["data"], "id"),
    };
    let mut names: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m[field].as_str().or_else(|| m["model"].as_str()))
        .map(|name| name.strip_suffix(":cloud").unwrap_or(name).to_string())
        .filter(|name| valid_model(name))
        .collect();
    names.sort();
    names.dedup();
    names
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
    output_pressure: HashMap<(Provider, String, Route), u8>,
    unavailable_models: HashMap<(Provider, String), Instant>,
}

impl Limiter {
    pub(crate) fn reset_model_availability(&mut self, provider: Provider) {
        self.unavailable_models.retain(|(id, _), _| *id != provider);
    }

    fn model_available(&self, provider: Provider, model: &str) -> bool {
        self.unavailable_models
            .get(&(provider, model.to_owned()))
            .is_none_or(|until| *until <= Instant::now())
    }

    fn observe_model(&mut self, provider: Provider, model: &str, answer: &Result<Answer, Failure>) {
        let key = (provider, model.to_owned());
        if matches!(answer, Err(Failure::ModelUnavailable)) {
            let now = Instant::now();
            self.unavailable_models.retain(|_, until| *until > now);
            if self.unavailable_models.len() < 64 {
                self.unavailable_models.insert(key, now + MODEL_RECHECK);
            }
        } else if answer.is_ok() {
            self.unavailable_models.remove(&key);
        }
    }

    fn observe_output(&mut self, key: (Provider, String, Route), answer: &Result<Answer, Failure>) {
        if matches!(answer, Err(Failure::Truncated)) {
            if self.output_pressure.len() < 64 || self.output_pressure.contains_key(&key) {
                let level = self.output_pressure.entry(key).or_default();
                *level = (*level + 1).min(2);
            }
        } else if matches!(answer, Ok(Answer::Text(_))) {
            if let Some(level) = self.output_pressure.get_mut(&key) {
                *level -= 1;
                if *level == 0 {
                    self.output_pressure.remove(&key);
                }
            }
        }
    }

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
    Forbidden,
    Quota,
    Rejected(u16),
    Connection(crate::net::Cause),
    InvalidTool,
    ModelUnavailable,
    Incomplete,
    Truncated,
    Unavailable,
}

impl Failure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidKey => "cloud.credentials",
            Self::Forbidden => "cloud.permission",
            Self::Quota => "cloud.quota",
            Self::Rejected(_) => "cloud.request_rejected",
            Self::Connection(_) => "cloud.connection",
            Self::InvalidTool => "cloud.invalid_tool",
            Self::RateLimited(_) => "cloud.rate_limited",
            Self::ModelUnavailable => "cloud.model_unavailable",
            Self::Incomplete | Self::Truncated => "cloud.incomplete_response",
            Self::Unavailable => "cloud.unavailable",
        }
    }

    pub(crate) fn user_message(&self, provider: Provider) -> (String, bool) {
        let label = provider.label();
        match self {
            Self::InvalidKey => (format!("{label} ha rifiutato la chiave API (HTTP 401). Verificala in Modelli AI."), false),
            Self::Forbidden => (format!("{label} ha negato l’accesso (HTTP 403). Controlla i permessi o il piano dell’account."), false),
            Self::Quota => (format!("{label} richiede credito o un piano abilitato (HTTP 402)."), false),
            Self::Rejected(status) => (format!("{label} ha rifiutato la richiesta (HTTP {status}). Verifica il modello con Testa chiave in Modelli AI."), false),
            Self::Connection(cause) => (format!("Connessione HTTPS a {label} non riuscita: {}.", cause.hint()), !matches!(cause, crate::net::Cause::Certificate | crate::net::Cause::Proxy)),
            Self::InvalidTool => ("Il modello ha proposto un calcolo non valido. Riformula la richiesta oppure usa un comando esplicito.".into(), false),
            Self::RateLimited(wait) => (format!("Limite del provider raggiunto. Riprova tra {} s o scegli un altro provider.", wait.as_secs_f64().ceil() as u64), true),
            Self::ModelUnavailable => (format!("I modelli selezionati non sono disponibili presso {label}. Verifica il modello in Modelli AI."), true),
            Self::Incomplete | Self::Truncated => (format!("{label} ha restituito una risposta incompleta. Prova a suddividere la richiesta."), true),
            Self::Unavailable => (format!("{label} non ha completato la richiesta. Riprova o scegli un altro provider."), true),
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

// Le proposte remote sono limitate a calcoli senza accesso a dati locali o alla rete.
pub const TOOLS: &[&str] = &[
    "date.calc",
    "math.calc",
    "text.count",
    "units.convert",
    // Calcoli di direzione aziendale (ADR-0132): le cifre devono venire da uno strumento, non dal modello.
    "biz.vat",
    "biz.loan",
    "biz.margin",
    "biz.breakeven",
    "biz.npv",
    "biz.runway",
    "biz.unit_economics",
    "biz.priorities",
    "biz.cagr",
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

/// Passo già eseguito da un agente autonomo (ADR-0096): chiamata e risultato restituito al modello.
pub struct AgentStep {
    pub tool: String,
    pub arguments: Value,
    pub output: String,
}

/// Richiesta di un passo a un agente autonomo: obiettivo, passi già eseguiti e strumenti consentiti. Senza
/// strumenti il modello deve rispondere. Come le chat, viene eseguita fuori dal blocco del Core.
pub struct AgentRequest {
    pub provider: Provider,
    pub model: String,
    key: String,
    pub system: String,
    pub goal: String,
    pub steps: Vec<AgentStep>,
    pub tools: Vec<Function>,
    limiter: Arc<Mutex<Limiter>>,
}

impl AgentRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Provider,
        model: String,
        key: String,
        system: String,
        goal: String,
        steps: Vec<AgentStep>,
        tools: Vec<Function>,
        limiter: Arc<Mutex<Limiter>>,
    ) -> Self {
        Self {
            provider,
            model,
            key,
            system,
            goal,
            steps,
            tools,
            limiter,
        }
    }

    pub fn payload(&self) -> Value {
        let mut messages = vec![
            json!({"role": "system", "content": self.system}),
            json!({"role": "user", "content": self.goal}),
        ];
        for (index, step) in self.steps.iter().enumerate() {
            let name = step.tool.replace('.', "__");
            match self.provider {
                Provider::Ollama => {
                    messages.push(json!({"role": "assistant", "content": "",
                        "tool_calls": [{"function": {"name": name, "arguments": step.arguments}}]}));
                    messages
                        .push(json!({"role": "tool", "tool_name": name, "content": step.output}));
                }
                Provider::Nim => {
                    let id = format!("call_{index}");
                    messages.push(json!({"role": "assistant", "content": null,
                        "tool_calls": [{"id": id, "type": "function",
                            "function": {"name": name, "arguments": step.arguments.to_string()}}]}));
                    messages
                        .push(json!({"role": "tool", "tool_call_id": id, "content": step.output}));
                }
            }
        }
        let mut body = match self.provider {
            Provider::Ollama => json!({"model": self.model, "messages": messages, "stream": false,
                "options": {"num_predict": 2_000}}),
            Provider::Nim => json!({"model": self.model, "messages": messages, "stream": false,
                "max_tokens": 2_000, "temperature": 0.2}),
        };
        if !self.tools.is_empty() {
            body["tools"] = json!(self.tools.iter().map(|tool| &tool.wire).collect::<Vec<_>>());
            if self.provider == Provider::Nim {
                body["tool_choice"] = json!("auto");
            }
        }
        body
    }

    pub fn execute(&self) -> Result<Answer, Failure> {
        self.execute_at(self.provider.endpoint())
    }

    pub(crate) fn execute_at(&self, endpoint: &str) -> Result<Answer, Failure> {
        chat_at(
            endpoint,
            self.provider,
            &self.key,
            self.payload(),
            &self.tools,
            &self.limiter,
        )
    }
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
    pub(crate) alternative: String,
    route: Route,
}

pub struct Request {
    pub message: String,
    pub tools: Vec<Function>,
    pub target: Target,
    pub(crate) preferences: [QualityPreference; 2],
    pub limiter: Arc<Mutex<Limiter>>,
}

pub struct Completed {
    pub provider: Provider,
    pub model: String,
    pub answer: Result<Answer, Failure>,
}

impl Request {
    pub fn execute(&self) -> Completed {
        self.execute_at(self.target.provider.endpoint(), false)
    }

    /// Come `execute`, ma per le richieste senza strumenti inoltra il testo a `sink` mentre il modello lo genera
    /// (ADR-0131). Con strumenti, o se lo streaming fallisce prima del primo frammento, ripiega sul percorso
    /// completo e consegna il testo in un solo blocco.
    pub fn execute_streaming(&self, sink: &mut dyn FnMut(&str)) -> Completed {
        self.execute_streaming_at(self.target.provider.endpoint(), sink)
    }

    fn execute_streaming_at(&self, endpoint: &str, sink: &mut dyn FnMut(&str)) -> Completed {
        let target = &self.target;
        let model = &target.model;
        let usable = self
            .limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .model_available(target.provider, model);
        let finish = |completed: Completed, sink: &mut dyn FnMut(&str)| {
            if let Ok(Answer::Text(text)) = &completed.answer {
                sink(text);
            }
            completed
        };
        if !self.tools.is_empty() || !usable {
            return finish(self.execute_at(endpoint, false), sink);
        }
        let key = (target.provider, model.to_owned(), target.route);
        let level = self
            .limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .output_pressure
            .get(&key)
            .copied()
            .unwrap_or_default();
        let mut body = payload(
            target.provider,
            model,
            &self.message,
            &target.history,
            &[],
            (target.route, self.preferences[0], level),
        );
        body["stream"] = json!(true);
        let mut emitted = false;
        let answer = stream_at(
            endpoint,
            target.provider,
            &target.key,
            &body,
            &self.limiter,
            if target.route == Route::Fast {
                FAST_REQUEST_TIMEOUT
            } else {
                REQUEST_TIMEOUT
            },
            &mut |piece| {
                emitted = true;
                sink(piece);
            },
        );
        {
            let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
            limiter.observe_output(key, &answer);
            limiter.observe_model(target.provider, model, &answer);
        }
        match answer {
            Err(_) if !emitted => finish(self.execute_at(endpoint, false), sink),
            answer => Completed {
                provider: target.provider,
                model: model.clone(),
                answer,
            },
        }
    }

    pub(crate) fn verify(&self) -> Completed {
        self.execute_at(self.target.provider.endpoint(), true)
    }

    fn execute_at(&self, endpoint: &str, fresh: bool) -> Completed {
        let target = &self.target;
        let mut model = &target.model;
        let ask = |model: &str, preference, tools: &[Function]| {
            let key = (target.provider, model.to_owned(), target.route);
            let level = self
                .limiter
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .output_pressure
                .get(&key)
                .copied()
                .unwrap_or_default();
            let answer = chat_timed(
                endpoint,
                target.provider,
                &target.key,
                payload(
                    target.provider,
                    model,
                    &self.message,
                    &target.history,
                    tools,
                    (target.route, preference, level),
                ),
                tools,
                &self.limiter,
                if target.route == Route::Fast {
                    FAST_REQUEST_TIMEOUT
                } else {
                    REQUEST_TIMEOUT
                },
            );
            let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
            limiter.observe_output(key, &answer);
            limiter.observe_model(target.provider, model, &answer);
            answer
        };
        let available = |model: &str| {
            fresh
                || self
                    .limiter
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .model_available(target.provider, model)
        };
        let mut answer = if available(model) {
            ask(model, self.preferences[0], &self.tools)
        } else {
            Err(Failure::ModelUnavailable)
        };
        if matches!(
            answer,
            Err(Failure::ModelUnavailable | Failure::Incomplete | Failure::Truncated)
        ) && available(&target.alternative)
        {
            model = &target.alternative;
            answer = ask(model, self.preferences[1], &self.tools);
        } else if !self.tools.is_empty() && matches!(answer, Err(Failure::Rejected(400 | 422))) {
            answer = ask(model, self.preferences[0], &[]);
        }
        Completed {
            provider: target.provider,
            model: model.clone(),
            answer,
        }
    }
}

fn payload(
    provider: Provider,
    model: &str,
    prompt: &str,
    history: &[Turn],
    tools: &[Function],
    response: (Route, QualityPreference, u8),
) -> Value {
    let (route, preference, level) = response;
    // Plafond JARVIS, conservativo anche per i modelli di ripiego.
    let input_tokens = estimated_tokens(prompt)
        + history
            .iter()
            .map(|turn| estimated_tokens(&turn.user) + estimated_tokens(&turn.assistant))
            .sum::<usize>()
        + tools
            .iter()
            .map(|tool| estimated_tokens(&tool.wire.to_string()))
            .sum::<usize>();
    let base_tokens = route.output_tokens();
    let output_tokens = (base_tokens + (base_tokens / 4) * usize::from(level))
        .min(32_000usize.saturating_sub(input_tokens + 1_500).max(256));
    let mut messages = vec![
        json!({"role": "system", "content": crate::prompts::system(&crate::prompts::Context {
            model,
            route: route.name(),
            route_guidance: route.guidance(),
            prompt,
            length: preference.length_guidance(),
            precision: preference.precision_guidance(),
        })}),
    ];
    for turn in history {
        messages.push(json!({"role": "user", "content": turn.user}));
        messages.push(json!({"role": "assistant", "content": turn.assistant}));
    }
    messages.push(json!({"role": "user", "content": prompt}));
    let mut body = match provider {
        Provider::Ollama => {
            json!({"model": model, "messages": messages, "stream": false, "options": {"num_predict": output_tokens}})
        }
        Provider::Nim => {
            json!({"model": model, "messages": messages, "stream": false, "max_tokens": output_tokens, "temperature": 0.2})
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
    let reason = match provider {
        Provider::Ollama => &body["done_reason"],
        Provider::Nim => &body["choices"][0]["finish_reason"],
    };
    if reason == "length" {
        return Err(Failure::Truncated);
    }
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
        .ok_or(Failure::Incomplete)
}

fn classify_model_rejection(status: u16, reader: impl Read) -> Failure {
    let mut bytes = Vec::new();
    let body = reader
        .take(16_384)
        .read_to_end(&mut bytes)
        .ok()
        .and_then(|_| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    let model_error = [
        body["error"]["code"].as_str(),
        body["error"]["message"].as_str(),
        body["error"].as_str(),
        body["detail"].as_str(),
        body["message"].as_str(),
    ]
    .into_iter()
    .flatten()
    .any(|text| {
        let text = text.to_ascii_lowercase();
        text.contains("model")
            && !["tool", "function calling"]
                .iter()
                .any(|term| text.contains(term))
            && [
                "not found",
                "does not exist",
                "not supported",
                "unavailable",
                "model_not_found",
            ]
            .iter()
            .any(|term| text.contains(term))
    });
    if model_error {
        Failure::ModelUnavailable
    } else {
        Failure::Rejected(status)
    }
}

pub(crate) fn may_need_calculation(message: &str) -> bool {
    let text = message.to_lowercase();
    text.split(|character: char| !character.is_alphabetic())
        .any(|word| {
            matches!(
                word,
                "calcola"
                    | "calcolo"
                    | "converti"
                    | "conversione"
                    | "percentuale"
                    | "calculate"
                    | "convert"
                    | "percentage"
                    | "margine"
                    | "markup"
                    | "ricarico"
                    | "pareggio"
                    | "breakeven"
                    | "npv"
                    | "van"
                    | "tir"
                    | "irr"
                    | "payback"
                    | "runway"
                    | "ltv"
                    | "cac"
                    | "cagr"
                    | "iva"
                    | "mutuo"
                    | "rata"
                    | "roi"
            )
        })
}

/// Tempo massimo di una richiesta ai modelli: i modelli di ragionamento possono impiegare più di 45 s.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
/// Una risposta breve non deve tenere l'utente in attesa quanto un ragionamento lungo: se il modello veloce
/// non risponde entro questo tempo subentra l'alternativa.
const FAST_REQUEST_TIMEOUT: Duration = Duration::from_secs(40);
const CONNECT_RETRIES: u32 = 2;

/// Agente HTTP condiviso per (HTTPS, timeout): riusa le connessioni già aperte, così il messaggio successivo
/// non paga di nuovo DNS, TCP e handshake TLS (centinaia di millisecondi a richiesta).
fn shared_agent(endpoint: &str, timeout: Duration) -> ureq::Agent {
    static AGENTS: std::sync::OnceLock<Mutex<std::collections::HashMap<(bool, u64), ureq::Agent>>> =
        std::sync::OnceLock::new();
    let https = endpoint.starts_with("https://");
    AGENTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry((https, timeout.as_secs()))
        .or_insert_with(|| {
            // Gli endpoint sono fissi e il bearer token non viene inoltrato su redirect.
            crate::net::builder(timeout)
                .try_proxy_from_env(https)
                .build()
        })
        .clone()
}

fn chat_at(
    endpoint: &str,
    provider: Provider,
    key: &str,
    body: Value,
    tools: &[Function],
    limiter: &Mutex<Limiter>,
) -> Result<Answer, Failure> {
    chat_timed(
        endpoint,
        provider,
        key,
        body,
        tools,
        limiter,
        REQUEST_TIMEOUT,
    )
}

/// Invia la richiesta (pacing, nuovi tentativi di rete, mappatura degli errori HTTP) e restituisce la risposta.
fn send(
    endpoint: &str,
    provider: Provider,
    key: &str,
    body: &Value,
    limiter: &Mutex<Limiter>,
    timeout: Duration,
) -> Result<ureq::Response, Failure> {
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
    let agent = shared_agent(endpoint, timeout);
    let payload = body.to_string();
    let mut attempt = 0;
    let response = loop {
        let result = agent
            .post(endpoint)
            .set("Authorization", &format!("Bearer {key}"))
            .set("Content-Type", "application/json")
            .send_string(&payload);
        match &result {
            // Errori di rete transitori (DNS, connessione rifiutata o interrotta): fino a due nuovi tentativi.
            Err(ureq::Error::Transport(t))
                if attempt < CONNECT_RETRIES && crate::net::classify(t).transient() =>
            {
                std::thread::sleep(Duration::from_millis(400 * 3u64.pow(attempt)));
                attempt += 1;
            }
            _ => break result,
        }
    };
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
        Err(ureq::Error::Status(404, _)) => return Err(Failure::ModelUnavailable),
        Err(ureq::Error::Status(401, _)) => return Err(Failure::InvalidKey),
        Err(ureq::Error::Status(402, _)) => return Err(Failure::Quota),
        Err(ureq::Error::Status(403, _)) => return Err(Failure::Forbidden),
        Err(ureq::Error::Status(status @ (400 | 422), response)) => {
            return Err(classify_model_rejection(status, response.into_reader()));
        }
        Err(ureq::Error::Status(status, _)) if status < 500 => {
            return Err(Failure::Rejected(status))
        }
        Err(ureq::Error::Status(_, _)) => return Err(Failure::Unavailable),
        Err(ureq::Error::Transport(t)) => {
            return Err(Failure::Connection(crate::net::classify(&t)))
        }
    };
    if !(200..300).contains(&response.status()) {
        return Err(Failure::Rejected(response.status()));
    }
    Ok(response)
}

fn chat_timed(
    endpoint: &str,
    provider: Provider,
    key: &str,
    body: Value,
    tools: &[Function],
    limiter: &Mutex<Limiter>,
    timeout: Duration,
) -> Result<Answer, Failure> {
    let response = send(endpoint, provider, key, &body, limiter, timeout)?;
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

/// Risposta in streaming (ADR-0131): NDJSON per Ollama, SSE per NVIDIA NIM. Inoltra ogni frammento di testo a `sink`
/// e ricostruisce lo stesso esito di una risposta completa, con gli stessi controlli (vuota, troncata).
fn stream_at(
    endpoint: &str,
    provider: Provider,
    key: &str,
    body: &Value,
    limiter: &Mutex<Limiter>,
    timeout: Duration,
    sink: &mut dyn FnMut(&str),
) -> Result<Answer, Failure> {
    use std::io::BufRead;
    let response = send(endpoint, provider, key, body, limiter, timeout)?;
    let reader = std::io::BufReader::new(response.into_reader().take(MAX_RESPONSE_BYTES + 1));
    let mut text = String::new();
    let mut reason = Value::Null;
    for line in reader.lines() {
        let line = line.map_err(|_| Failure::Unavailable)?;
        let line = line.trim();
        let data = match provider {
            Provider::Ollama => line,
            Provider::Nim => match line.strip_prefix("data:") {
                Some(data) => data.trim(),
                None => continue,
            },
        };
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let (piece, finish) = match provider {
            Provider::Ollama => (chunk["message"]["content"].as_str(), &chunk["done_reason"]),
            Provider::Nim => (
                chunk["choices"][0]["delta"]["content"].as_str(),
                &chunk["choices"][0]["finish_reason"],
            ),
        };
        if let Some(piece) = piece.filter(|piece| !piece.is_empty()) {
            text.push_str(piece);
            sink(piece);
        }
        if !finish.is_null() {
            reason = finish.clone();
        }
    }
    if text.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(Failure::Unavailable);
    }
    let synthetic = match provider {
        Provider::Ollama => json!({"message": {"content": text}, "done_reason": reason}),
        Provider::Nim => {
            json!({"choices": [{"message": {"content": text}, "finish_reason": reason}]})
        }
    };
    decode(provider, &synthetic, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_model_rejection_retries_a_catalog_model_with_the_same_key() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/api/chat", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (expected, status, response) in [
                (
                    "legacy",
                    "400 Bad Request",
                    r#"{"error":"model legacy not found"}"#,
                ),
                (
                    "glm-5.3-flash",
                    "200 OK",
                    r#"{"message":{"content":"Risposta sintetica"}}"#,
                ),
                (
                    "glm-5.3-flash",
                    "200 OK",
                    r#"{"message":{"content":"Seconda risposta sintetica"}}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.trim().split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                        if name.eq_ignore_ascii_case("authorization") {
                            authorized = value.trim() == "Bearer fixture-only";
                        }
                    }
                }
                assert!(authorized);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], expected);
                assert!(body.get("tools").is_none());
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        let prompt = "una domanda sintetica";
        let target = Provider::Ollama.target(
            "fixture-only".into(),
            prompt,
            Vec::new(),
            &json!({"ollama_cloud":"legacy:cloud"}),
        );
        let request = Request {
            message: prompt.into(),
            target,
            tools: vec![],
            preferences: [QualityPreference::default(); 2],
            limiter: Default::default(),
        };
        let result = request.execute_at(&endpoint, false);
        assert_eq!(result.model, "glm-5.3-flash");
        assert!(matches!(result.answer, Ok(Answer::Text(text)) if text == "Risposta sintetica"));
        let result = request.execute_at(&endpoint, false);
        assert_eq!(result.model, "glm-5.3-flash");
        assert!(
            matches!(result.answer, Ok(Answer::Text(text)) if text == "Seconda risposta sintetica")
        );
        server.join().unwrap();
    }

    /// Server di prova: per ogni richiesta restituisce `(stream atteso, tipo di contenuto, corpo)`.
    fn serve(script: Vec<(bool, &'static str, String)>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (stream, kind, response) in script {
                let (mut socket, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.trim().split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["stream"] == true, stream, "stream atteso: {stream}");
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        (endpoint, server)
    }

    fn fixture_request(provider: Provider) -> Request {
        let prompt = "una domanda sintetica";
        Request {
            message: prompt.into(),
            target: provider.target("fixture-only".into(), prompt, Vec::new(), &json!({})),
            tools: vec![],
            preferences: [QualityPreference::default(); 2],
            limiter: Default::default(),
        }
    }

    #[test]
    fn streaming_forwards_fragments_for_both_wire_formats() {
        let ollama = [
            r#"{"message":{"content":"Ciao "},"done":false}"#,
            r#"{"message":{"content":"mondo"},"done":false}"#,
            r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#,
        ]
        .join("\n");
        let nim = [
            r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"Ciao "},"finish_reason":null}]}"#,
            r#"data: {"choices":[{"delta":{"content":"mondo"},"finish_reason":null}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ]
        .join("\n\n");
        for (provider, kind, body) in [
            (Provider::Ollama, "application/x-ndjson", ollama),
            (Provider::Nim, "text/event-stream", nim),
        ] {
            let (endpoint, server) = serve(vec![(true, kind, body)]);
            let mut pieces = Vec::new();
            let result = fixture_request(provider)
                .execute_streaming_at(&endpoint, &mut |piece| pieces.push(piece.to_string()));
            assert_eq!(pieces, ["Ciao ", "mondo"], "{}", provider.id());
            assert!(matches!(result.answer, Ok(Answer::Text(text)) if text == "Ciao mondo"));
            server.join().unwrap();
        }
    }

    #[test]
    fn a_truncated_stream_keeps_its_error_and_an_empty_one_falls_back_to_the_full_request() {
        let truncated = [
            r#"{"message":{"content":"Inizio"},"done":false}"#,
            r#"{"message":{"content":""},"done":true,"done_reason":"length"}"#,
        ]
        .join("\n");
        let (endpoint, server) = serve(vec![(true, "application/x-ndjson", truncated)]);
        let mut pieces = Vec::new();
        let result = fixture_request(Provider::Ollama)
            .execute_streaming_at(&endpoint, &mut |piece| pieces.push(piece.to_string()));
        assert_eq!(pieces, ["Inizio"]);
        assert!(matches!(result.answer, Err(Failure::Truncated)));
        server.join().unwrap();

        // Nessun frammento: si ripete con la richiesta completa e il testo arriva in un blocco.
        let empty = r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#.to_string();
        let full = r#"{"message":{"content":"Risposta piena"}}"#.to_string();
        let (endpoint, server) = serve(vec![
            (true, "application/x-ndjson", empty),
            (false, "application/json", full),
        ]);
        let mut pieces = Vec::new();
        let result = fixture_request(Provider::Ollama)
            .execute_streaming_at(&endpoint, &mut |piece| pieces.push(piece.to_string()));
        assert_eq!(pieces, ["Risposta piena"]);
        assert!(matches!(result.answer, Ok(Answer::Text(text)) if text == "Risposta piena"));
        server.join().unwrap();
    }

    #[test]
    fn provider_request_uses_the_configured_bearer_key() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let read = stream.read(&mut chunk).unwrap();
                assert_ne!(read, 0, "il provider deve ricevere la richiesta completa");
                request.extend_from_slice(&chunk[..read]);

                let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    })
                    .expect("richiesta JSON con lunghezza nota");
                if request.len() >= header_end + 4 + content_length {
                    assert!(
                        headers
                            .lines()
                            .any(|line| line
                                .eq_ignore_ascii_case("authorization: Bearer fixture-key")),
                        "la chiave configurata deve essere inviata solo nell'header Bearer"
                    );
                    break;
                }
            }

            let body = r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let response = chat_at(
            &format!("http://{address}/chat"),
            Provider::Nim,
            "fixture-key",
            json!({"model":"fixture"}),
            &[],
            &Mutex::new(Limiter::default()),
        )
        .unwrap();
        assert!(matches!(response, Answer::Text(text) if text == "ok"));
        server.join().unwrap();
    }

    #[test]
    fn incomplete_responses_are_detected_before_text_or_tools_are_used() {
        assert!(matches!(
            decode(
                Provider::Nim,
                &json!({
                    "choices": [{"finish_reason": "length", "message": {"content": "parziale"}}]
                }),
                &[]
            ),
            Err(Failure::Truncated)
        ));
        assert!(matches!(
            decode(
                Provider::Ollama,
                &json!({
                    "done_reason": "length", "message": {"content": "parziale"}
                }),
                &[]
            ),
            Err(Failure::Truncated)
        ));
        assert!(matches!(
            decode(
                Provider::Ollama,
                &json!({
                    "message": {"content": " "}
                }),
                &[]
            ),
            Err(Failure::Incomplete)
        ));
    }

    #[test]
    fn truncation_adjusts_budget_only_for_the_affected_model_and_route() {
        let mut limiter = Limiter::default();
        let key = (Provider::Ollama, "fixture:cloud".into(), Route::Fast);
        for _ in 0..3 {
            limiter.observe_output(key.clone(), &Err(Failure::Truncated));
        }
        let level = limiter.output_pressure[&key];
        assert_eq!(level, 2);
        assert_eq!(limiter.output_pressure.len(), 1);
        let body = payload(
            Provider::Ollama,
            "fixture:cloud",
            "richiesta",
            &[],
            &[],
            (Route::Fast, QualityPreference::default(), level),
        );
        assert_eq!(body["options"]["num_predict"], 2304);
        for _ in 0..2 {
            limiter.observe_output(key.clone(), &Ok(Answer::Text("completa".into())));
        }
        assert!(!limiter.output_pressure.contains_key(&key));
    }

    #[test]
    fn feedback_maps_only_to_a_predefined_model_instruction() {
        let preference = QualityPreference::from_counts(0, 1, 0, 1);
        let answer = payload(
            Provider::Ollama,
            "fixture",
            "richiesta",
            &[],
            &[],
            (Route::Fast, preference, 0),
        );
        assert!(answer["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("limita le ripetizioni"));
        assert!(answer["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Dichiara le ipotesi decisive"));
        assert_eq!(
            QualityPreference::from_counts(0, 1, 1, 0),
            QualityPreference::default()
        );
        assert_eq!(
            QualityPreference::from_counts(1, 1, 0, 0),
            QualityPreference::default()
        );
    }

    #[test]
    fn routes_both_providers_by_task_and_context() {
        for provider in Provider::ALL {
            let request =
                |prompt: &str, history| provider.target("test".into(), prompt, history, &json!({}));
            let fast = request("Cos’è una variabile?", vec![]);
            assert_eq!(fast.route, Route::Fast);
            assert_eq!(
                request("Cos’è una funzione in Python?", vec![]).model,
                fast.model
            );
            let long_request = "Scrivi un riepilogo di questo documento. ".repeat(50);
            let long = request(&long_request, vec![]);
            assert_eq!(long.route, Route::Reasoning);
            let design = request("Confronta i due approcci", vec![]);
            assert_eq!(design.model, long.model);
            assert_eq!(
                request(
                    "Prima fai un riepilogo, poi elenca i rischi e infine proponi le azioni.",
                    vec![]
                )
                .route,
                Route::Reasoning
            );
            // «prima di» e un solo «poi» sono linguaggio comune: restano sul modello veloce.
            assert_eq!(
                request(
                    "Cosa devo controllare prima di partire? Poi dimmi l'orario.",
                    vec![]
                )
                .route,
                Route::Fast
            );
            let coding = request("Analizza questo script Python", vec![]);
            assert_eq!(coding.route, Route::Coding);
            assert_ne!(fast.model, long.model);
            assert_ne!(coding.model, long.model);
            let followup = request(
                "Continua",
                vec![Turn {
                    user: long_request,
                    assistant: "D'accordo".into(),
                }],
            );
            // Il seguito di un lungo scambio resta veloce; solo un contesto davvero enorme passa al modello lungo.
            assert_eq!(followup.route, Route::Fast);
            let huge = request(
                "Continua",
                vec![Turn {
                    user: "dato ".repeat(4000),
                    assistant: "d'accordo".into(),
                }],
            );
            assert_eq!(huge.route, Route::Long);
        }
    }

    #[test]
    fn token_budget_counts_code_and_clips_unicode_at_valid_boundaries() {
        assert!(estimated_tokens("{{}}[];::??") > estimated_tokens("conversation"));
        let text = format!("inizio {} fine", "日本語🙂 ".repeat(180));
        let clipped = clip_tokens(&text, 80);
        assert!(estimated_tokens(&clipped) <= 80);
        assert!(clipped.starts_with("inizio") && clipped.ends_with("fine"));
        let target = Provider::Ollama.target("test".into(), &text.repeat(10), vec![], &json!({}));
        assert_eq!(target.route, Route::Long);
    }

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
            let body = payload(
                provider,
                "test",
                "E il client?",
                &history,
                &[],
                (Route::Fast, QualityPreference::default(), 0),
            );
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
    fn business_requests_get_executive_guidance_and_calculation_tools() {
        let prompt =
            "Calcola il punto di pareggio: costi fissi 10000, prezzo 50, costo variabile 30";
        assert!(may_need_calculation(prompt));
        assert!(may_need_calculation(
            "Qual è il ROI di questo investimento?"
        ));
        assert!(!may_need_calculation("Cos’è una variabile?"));
        let body = payload(
            Provider::Ollama,
            "test",
            prompt,
            &[],
            &[],
            (Route::Fast, QualityPreference::default(), 0),
        );
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("consulente di direzione"));
        let plain = payload(
            Provider::Ollama,
            "test",
            "Cos’è una variabile?",
            &[],
            &[],
            (Route::Fast, QualityPreference::default(), 0),
        );
        assert!(!plain["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("consulente di direzione"));
        // Ogni strumento aziendale esposto al cloud esiste nel catalogo.
        let catalog = crate::catalog::Catalog::load().unwrap();
        for tool in TOOLS {
            assert!(catalog.get(tool).is_some(), "{tool}");
        }
    }

    #[test]
    fn system_prompt_sets_a_grounded_and_composed_persona() {
        let body = payload(
            Provider::Ollama,
            "test",
            "Richiesta",
            &[],
            &[],
            (Route::Fast, QualityPreference::default(), 0),
        );
        let system = body["messages"][0]["content"].as_str().unwrap();
        for instruction in [
            "tono calmo, competente e cordiale",
            "distingui fatti, inferenze e stime",
            "controlla il suo esito",
            "Non dichiarare di aver completato un’azione",
            "Data e ora locali:",
            "maggiordomo digitale",
            "testo semplice e leggibile",
            "Sii specifico",
            "assumi l’ipotesi più probabile",
        ] {
            assert!(
                system.contains(instruction),
                "manca l'istruzione: {instruction}"
            );
        }
    }

    #[test]
    fn tool_proposals_are_single_scoped_and_accept_both_wire_formats() {
        let catalog = crate::catalog::Catalog::load().unwrap();
        let tools = [Function::from(catalog.get("fs.read").unwrap())];
        let body = payload(
            Provider::Nim,
            "test",
            "Richiesta corrente",
            &[],
            &tools,
            (Route::Fast, QualityPreference::default(), 0),
        );
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

#[cfg(test)]
mod agent_tests {
    use super::*;

    fn request(provider: Provider, with_tools: bool) -> AgentRequest {
        let tools = if with_tools {
            let catalog = crate::catalog::Catalog::load().unwrap();
            vec![Function::from(catalog.get("time.now").unwrap())]
        } else {
            Vec::new()
        };
        AgentRequest::new(
            provider,
            "modello-sintetico".into(),
            "chiave-sintetica".into(),
            "sistema".into(),
            "obiettivo".into(),
            vec![AgentStep {
                tool: "time.now".into(),
                arguments: json!({}),
                output: "{\"status\":\"succeeded\"}".into(),
            }],
            tools,
            Default::default(),
        )
    }

    #[test]
    fn the_agent_transcript_uses_each_providers_tool_message_format() {
        let ollama = request(Provider::Ollama, true).payload();
        let m = ollama["messages"].as_array().unwrap();
        assert_eq!(
            (m[0]["role"].as_str(), m[1]["role"].as_str()),
            (Some("system"), Some("user"))
        );
        assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "time__now");
        assert!(m[2]["tool_calls"][0]["function"]["arguments"].is_object());
        assert_eq!(
            (m[3]["role"].as_str(), m[3]["tool_name"].as_str()),
            (Some("tool"), Some("time__now"))
        );
        assert_eq!(ollama["tools"][0]["function"]["name"], "time__now");
        assert!(ollama.get("tool_choice").is_none());

        let nim = request(Provider::Nim, true).payload();
        let m = nim["messages"].as_array().unwrap();
        assert_eq!(m[2]["tool_calls"][0]["id"], m[3]["tool_call_id"]);
        assert_eq!(m[2]["tool_calls"][0]["type"], "function");
        assert!(m[2]["tool_calls"][0]["function"]["arguments"].is_string());
        assert_eq!(nim["tool_choice"], "auto");

        // Senza strumenti (ultimo passo) il modello deve solo rispondere.
        assert!(request(Provider::Nim, false)
            .payload()
            .get("tools")
            .is_none());
    }

    #[test]
    fn an_agent_step_round_trips_through_the_provider() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/api/chat", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((n, v)) = line.trim().split_once(':') {
                    if n.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["messages"].as_array().unwrap().len(), 4);
            let reply = r#"{"message":{"content":"","tool_calls":[{"function":{"name":"time__now","arguments":{}}}]}}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
        });
        let answer = request(Provider::Ollama, true)
            .execute_at(&endpoint)
            .unwrap();
        assert!(
            matches!(&answer, Answer::Call(c) if c["tool"] == "time.now" && c["version"] == "1.0.0")
        );
        server.join().unwrap();
    }

    #[test]
    fn an_unreachable_endpoint_reports_the_network_cause_after_retrying() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let started = Instant::now();
        let Err(failure) = request(Provider::Ollama, false)
            .execute_at(&format!("http://127.0.0.1:{port}/api/chat"))
        else {
            panic!("la connessione doveva fallire");
        };
        assert!(matches!(
            failure,
            Failure::Connection(crate::net::Cause::Refused)
        ));
        // Due nuovi tentativi con attesa 0,4 s + 1,2 s.
        assert!(started.elapsed() >= Duration::from_millis(1500));
        let (text, retryable) = failure.user_message(Provider::Ollama);
        assert!(text.contains("server non accetta") && retryable);
    }

    #[test]
    fn pasted_keys_are_cleaned_and_bad_ones_explained() {
        let good = "nvapi-AbCdEf0123456789xyz";
        for raw in [
            good.to_string(),
            format!("  {good}\n"),
            format!("\"{good}\""),
            format!("Bearer {good}"),
            format!("Authorization: Bearer {good}"),
            format!("export NVIDIA_API_KEY={good}"),
            format!("NVIDIA_API_KEY=\"{good}\""),
            format!("\u{feff}{good}\u{200b}"),
        ] {
            assert_eq!(normalize_key(&raw).unwrap(), good, "{raw:?}");
        }
        for (raw, needle) in [
            ("", "vuota"),
            ("chiave con spazi dentro", "spazi"),
            ("chiave-con-àccento-12345", "non validi"),
            ("corta", "corta"),
        ] {
            assert!(normalize_key(raw).unwrap_err().contains(needle), "{raw}");
        }
        assert!(key_format_hint(Provider::Nim, "abcdefghijkl").is_some());
        assert!(key_format_hint(Provider::Nim, "nvapi-abcdefghij").is_none());
        assert!(key_format_hint(Provider::Ollama, "sk-abcdefghijkl").is_some());
    }

    #[test]
    fn model_lists_are_read_for_both_providers_and_credentials_errors_are_classified() {
        use std::io::{BufRead, BufReader, Write};
        assert_eq!(
            parse_models(
                Provider::Ollama,
                &json!({"models": [{"name": "glm-5.3-flash:cloud"}, {"model": "kimi-k3.1-pro"}]})
            ),
            vec!["glm-5.3-flash", "kimi-k3.1-pro"]
        );
        assert_eq!(
            parse_models(
                Provider::Nim,
                &json!({"data": [{"id": "b/model"}, {"id": "a/model"}, {"id": "bad id"}]})
            ),
            vec!["a/model", "b/model"]
        );
        let serve = |status: &'static str, body: &'static str| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/models", listener.local_addr().unwrap());
            std::thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    while reader.read_line(&mut line).is_ok() && line.trim() != "" {
                        line.clear();
                    }
                    let mut out = stream;
                    let _ = write!(out, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
                }
            });
            url
        };
        let ok = serve("200 OK", "{\"data\":[{\"id\":\"a/m\"}]}");
        assert_eq!(
            list_models_at(&ok, Provider::Nim, "k").unwrap(),
            vec!["a/m"]
        );
        let denied = serve("401 Unauthorized", "{}");
        assert!(matches!(
            list_models_at(&denied, Provider::Nim, "k"),
            Err(Failure::InvalidKey)
        ));
        let forbidden = serve("403 Forbidden", "{}");
        assert!(matches!(
            list_models_at(&forbidden, Provider::Nim, "k"),
            Err(Failure::Forbidden)
        ));
    }
}
