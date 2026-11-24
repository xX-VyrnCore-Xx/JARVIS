//! Ascolto «Hey Jarvis» sempre attivo (ADR-0140).
//!
//! In attesa il microfono alimenta solo un buffer in memoria: i blocchi con voce vanno al Core per un unico
//! verdetto (sì o no, nessun testo) e poi sono scartati. Solo dopo un sì il comando viene registrato,
//! trascritto e inviato; finito quello, si torna ad ascoltare soltanto la parola di attivazione. Nulla finisce
//! su disco. La logica è indipendente dalla piattaforma; la cattura del microfono è in `mic` (solo Windows).
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

pub const RATE: u32 = 16_000;
const CAPACITY: usize = RATE as usize * 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Off,
    Unavailable,
    Waiting,
    Capturing,
    Working,
}

impl Phase {
    fn code(self) -> u8 {
        match self {
            Phase::Off => 0,
            Phase::Unavailable => 1,
            Phase::Waiting => 2,
            Phase::Capturing => 3,
            Phase::Working => 4,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Phase::Unavailable,
            2 => Phase::Waiting,
            3 => Phase::Capturing,
            4 => Phase::Working,
            _ => Phase::Off,
        }
    }
}

/// Stato condiviso fra il thread di ascolto e l'interfaccia.
#[derive(Default)]
pub struct Shared {
    stop: AtomicBool,
    suspended: AtomicBool,
    phase: AtomicU8,
    level: AtomicU32,
    detail: Mutex<String>,
}

impl Shared {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Mentre JARVIS parla o elabora non ascolta: la sua voce non deve riattivarlo.
    pub fn set_suspended(&self, value: bool) {
        self.suspended.store(value, Ordering::SeqCst);
    }

    pub fn phase(&self) -> Phase {
        Phase::from_code(self.phase.load(Ordering::SeqCst))
    }

    pub fn set_phase(&self, phase: Phase) {
        self.phase.store(phase.code(), Ordering::SeqCst);
    }

    /// Livello della voce da 0 a 1, per l'indicatore.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    fn set_level(&self, value: f32) {
        self.level.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn detail(&self) -> String {
        self.detail.lock().map(|d| d.clone()).unwrap_or_default()
    }

    pub fn set_detail(&self, text: impl Into<String>) {
        if let Ok(mut detail) = self.detail.lock() {
            *detail = text.into();
        }
    }
}

/// Campioni a 16 kHz mono (16 bit) in un buffer circolare in memoria.
#[derive(Default)]
pub struct SampleBuffer {
    inner: Mutex<(Vec<i16>, usize)>,
}

impl SampleBuffer {
    /// Numero totale di campioni ricevuti (indice assoluto del prossimo).
    pub fn total(&self) -> usize {
        self.inner.lock().map(|g| g.1 + g.0.len()).unwrap_or(0)
    }

    pub fn append(&self, new: &[i16]) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.0.extend_from_slice(new);
            if guard.0.len() > CAPACITY {
                let extra = guard.0.len() - CAPACITY;
                guard.0.drain(..extra);
                guard.1 += extra;
            }
        }
    }

    /// Campioni dall'indice assoluto `start` (o dal più vecchio disponibile) fino alla fine.
    pub fn slice_from(&self, start: usize) -> Vec<i16> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        let offset = start.saturating_sub(guard.1);
        guard
            .0
            .get(offset..)
            .map(<[i16]>::to_vec)
            .unwrap_or_default()
    }

    pub fn last(&self, seconds: f64) -> Vec<i16> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        let count = ((seconds * f64::from(RATE)) as usize).min(guard.0.len());
        guard.0[guard.0.len() - count..].to_vec()
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.1 += guard.0.len();
            guard.0.clear();
        }
    }
}

/// Energia efficace di un blocco di campioni.
pub fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / samples.len() as f64).sqrt()
}

/// WAV PCM 16 bit mono a 16 kHz.
pub fn wav(samples: &[i16]) -> Vec<u8> {
    let bytes = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Porta i campioni del microfono (qualsiasi frequenza e numero di canali) a 16 kHz mono con interpolazione lineare.
pub struct Resampler {
    step: f64,
    position: f64,
    pending: Vec<f32>,
    channels: usize,
}

impl Resampler {
    pub fn new(input_rate: u32, channels: u16) -> Self {
        Self {
            step: f64::from(input_rate.max(1)) / f64::from(RATE),
            position: 0.0,
            pending: Vec::new(),
            channels: usize::from(channels.max(1)),
        }
    }

    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<i16>) {
        for frame in interleaved.chunks(self.channels) {
            self.pending
                .push(frame.iter().sum::<f32>() / frame.len() as f32);
        }
        while self.position + 1.0 < self.pending.len() as f64 {
            let index = self.position as usize;
            let fraction = (self.position - index as f64) as f32;
            let value = self.pending[index] * (1.0 - fraction) + self.pending[index + 1] * fraction;
            out.push((value.clamp(-1.0, 1.0) * 32767.0) as i16);
            self.position += self.step;
        }
        let used = (self.position as usize).min(self.pending.len());
        self.pending.drain(..used);
        self.position -= used as f64;
    }
}

/// Come l'ascolto parla con il Core: un verdetto sulla parola di attivazione e la trascrizione del comando.
pub trait Transport {
    fn post_audio(&self, path: &str, wav: &[u8]) -> Result<Value, String>;
    fn stop_voice(&self);
}

/// Tempi e soglia dell'ascolto; i test li riducono.
#[derive(Clone, Copy)]
pub struct Pacing {
    pub tick: Duration,
    pub window: f64,
    pub gate: f64,
    pub silence: Duration,
    pub minimum: f64,
    pub maximum: f64,
    pub first_voice: f64,
    pub retry: Duration,
}

impl Default for Pacing {
    fn default() -> Self {
        Self {
            tick: Duration::from_millis(400),
            window: 2.0,
            gate: 450.0,
            silence: Duration::from_millis(1100),
            minimum: 1.2,
            maximum: 12.0,
            first_voice: 5.0,
            retry: Duration::from_secs(20),
        }
    }
}

fn level_of(samples: &[i16]) -> f32 {
    (rms(samples) / 6000.0).min(1.0) as f32
}

/// Ciclo di ascolto: gira finché `shared.stopped()` è falso. `on_command` riceve il testo del comando.
pub fn run(
    buffer: &SampleBuffer,
    shared: &Shared,
    transport: &dyn Transport,
    pacing: Pacing,
    on_command: &mut dyn FnMut(String),
) {
    shared.set_phase(Phase::Waiting);
    let mut checked = buffer.total();
    let mut retry_at = Instant::now();
    while !shared.stopped() {
        std::thread::sleep(pacing.tick);
        shared.set_level(level_of(&buffer.last(0.4)));
        if shared.suspended.load(Ordering::SeqCst) {
            buffer.clear();
            checked = buffer.total();
            continue;
        }
        if Instant::now() < retry_at {
            continue;
        }
        // Solo se c'è voce nuova: il silenzio non lascia mai il computer.
        let fresh = buffer.total() - checked;
        if fresh < RATE as usize / 2 || rms(&buffer.last(0.6)) <= pacing.gate {
            continue;
        }
        checked = buffer.total();
        match transport.post_audio("/api/voice/wake", &wav(&buffer.last(pacing.window))) {
            Ok(verdict) => {
                shared.set_detail("");
                if verdict["detected"].as_bool() == Some(true) {
                    handle_wake(buffer, shared, transport, pacing, on_command);
                    checked = buffer.total();
                }
            }
            Err(error) => {
                // Voce non installata o Core occupato: si riprova fra poco.
                shared.set_detail(error);
                retry_at = Instant::now() + pacing.retry;
            }
        }
    }
    buffer.clear();
    shared.set_phase(Phase::Off);
    shared.set_level(0.0);
}

fn handle_wake(
    buffer: &SampleBuffer,
    shared: &Shared,
    transport: &dyn Transport,
    pacing: Pacing,
    on_command: &mut dyn FnMut(String),
) {
    shared.set_phase(Phase::Capturing);
    let start = buffer
        .total()
        .saturating_sub((pacing.window * f64::from(RATE)) as usize);
    record_until_silence(buffer, shared, pacing, pacing.minimum, pacing.maximum, 0.0);
    shared.set_phase(Phase::Working);
    let outcome = command_after_wake(buffer, shared, transport, pacing, start);
    shared.set_phase(Phase::Waiting);
    buffer.clear();
    match outcome {
        Some((_, Some(intent))) if intent == "stop" => transport.stop_voice(),
        Some((_, Some(intent))) if intent == "cancel" => {}
        Some((text, _)) if !text.trim().is_empty() => on_command(text.trim().to_owned()),
        _ => {}
    }
}

/// Testo e intento del comando che segue «Hey Jarvis»; se c'è solo la parola di attivazione ne attende uno nuovo.
fn command_after_wake(
    buffer: &SampleBuffer,
    shared: &Shared,
    transport: &dyn Transport,
    pacing: Pacing,
    start: usize,
) -> Option<(String, Option<String>)> {
    let result = transport
        .post_audio(
            "/api/voice/transcribe?mode=wake_word",
            &wav(&buffer.slice_from(start)),
        )
        .ok()?;
    if result["wake"]["detected"].as_bool() != Some(true) {
        return None;
    }
    let pick = |doc: &Value, keys: &[&str]| -> String {
        keys.iter()
            .find_map(|key| doc[*key].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let mut text = pick(&result, &["normalized"]);
    if text.is_empty() {
        text = result["wake"]["command"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
    }
    let mut intent = result["intent"].as_str().map(str::to_owned);
    if text.trim().is_empty() {
        // Solo «Hey Jarvis»: ora attende il comando vero e proprio.
        shared.set_phase(Phase::Capturing);
        let next = buffer.total();
        record_until_silence(buffer, shared, pacing, 0.8, 10.0, pacing.first_voice);
        shared.set_phase(Phase::Working);
        let spoken = transport
            .post_audio("/api/voice/transcribe", &wav(&buffer.slice_from(next)))
            .ok()?;
        text = pick(&spoken, &["normalized", "text"]);
        intent = spoken["intent"].as_str().map(str::to_owned);
    }
    Some((text, intent))
}

/// Registra finché non c'è silenzio dopo la voce, entro un massimo. `first_voice` è l'attesa dell'inizio della voce.
fn record_until_silence(
    buffer: &SampleBuffer,
    shared: &Shared,
    pacing: Pacing,
    minimum: f64,
    maximum: f64,
    first_voice: f64,
) {
    let began = Instant::now();
    let mut last_voice = Instant::now();
    let mut heard = first_voice == 0.0;
    let step = (pacing.tick / 2).max(Duration::from_millis(1));
    while !shared.stopped() {
        std::thread::sleep(step);
        let elapsed = began.elapsed().as_secs_f64();
        let recent = buffer.last(0.3);
        shared.set_level(level_of(&recent));
        if rms(&recent) > pacing.gate {
            last_voice = Instant::now();
            heard = true;
        }
        if elapsed >= maximum || (!heard && elapsed >= first_voice) {
            return;
        }
        if heard && elapsed >= minimum && last_voice.elapsed() >= pacing.silence {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    struct Fake {
        wake: bool,
        transcript: Value,
        calls: Mutex<Vec<String>>,
        stops: AtomicUsize,
    }

    impl Transport for Fake {
        fn post_audio(&self, path: &str, _wav: &[u8]) -> Result<Value, String> {
            self.calls.lock().unwrap().push(path.to_owned());
            if path == "/api/voice/wake" {
                Ok(json!({"detected": self.wake}))
            } else {
                Ok(self.transcript.clone())
            }
        }

        fn stop_voice(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn fast() -> Pacing {
        Pacing {
            tick: Duration::from_millis(4),
            window: 0.5,
            gate: 450.0,
            silence: Duration::from_millis(40),
            minimum: 0.02,
            maximum: 1.0,
            first_voice: 0.1,
            retry: Duration::from_millis(50),
        }
    }

    /// Alimenta il buffer: prima voce sintetica, poi silenzio.
    fn feed(buffer: Arc<SampleBuffer>, shared: Arc<Shared>, loud: bool) {
        std::thread::spawn(move || {
            if loud {
                for _ in 0..4 {
                    buffer.append(&vec![3000; 4000]);
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            while !shared.stopped() {
                buffer.append(&[0; 800]);
                std::thread::sleep(Duration::from_millis(2));
            }
        });
    }

    fn drive(fake: Fake, loud: bool, wait_ms: u64) -> (Vec<String>, Arc<Fake>) {
        let buffer = Arc::new(SampleBuffer::default());
        let shared = Arc::new(Shared::default());
        let fake = Arc::new(fake);
        feed(buffer.clone(), shared.clone(), loud);
        let stopper = shared.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(wait_ms));
            stopper.request_stop();
        });
        let mut heard = Vec::new();
        run(&buffer, &shared, fake.as_ref(), fast(), &mut |text| {
            heard.push(text)
        });
        assert_eq!(shared.phase(), Phase::Off);
        (heard, fake)
    }

    fn fake(wake: bool, transcript: Value) -> Fake {
        Fake {
            wake,
            transcript,
            calls: Mutex::new(Vec::new()),
            stops: AtomicUsize::new(0),
        }
    }

    #[test]
    fn silence_never_reaches_the_core() {
        let (heard, fake) = drive(fake(true, json!({})), false, 150);
        assert!(heard.is_empty());
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_wake_word_delivers_the_command_that_follows() {
        let transcript = json!({
            "wake": {"detected": true, "command": "che ore sono"},
            "normalized": "che ore sono", "intent": "ask"
        });
        let (heard, fake) = drive(fake(true, transcript), true, 500);
        assert_eq!(heard.first().map(String::as_str), Some("che ore sono"));
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls[0], "/api/voice/wake");
        assert!(calls.iter().any(|c| c.contains("mode=wake_word")));
    }

    #[test]
    fn other_speech_is_ignored_and_the_stop_intent_silences_the_voice() {
        let (heard, _) = drive(fake(false, json!({})), true, 300);
        assert!(heard.is_empty());
        let stop = json!({"wake": {"detected": true}, "normalized": "basta", "intent": "stop"});
        let (heard, fake) = drive(fake(true, stop), true, 500);
        assert!(heard.is_empty());
        assert!(fake.stops.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn the_resampler_converts_any_microphone_to_16_khz_mono() {
        let mut resampler = Resampler::new(48_000, 2);
        let mut out = Vec::new();
        let stereo: Vec<f32> = (0..96_000)
            .map(|i| ((i / 2) as f32 * 0.05).sin() * 0.5)
            .collect();
        for chunk in stereo.chunks(1024) {
            resampler.push(chunk, &mut out);
        }
        assert!((15_900..=16_001).contains(&out.len()), "{}", out.len());
        assert!(rms(&out) > 5000.0);
    }

    #[test]
    fn wav_and_buffer_behave() {
        let data = wav(&[1, -1, 2]);
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(data.len(), 44 + 6);
        let buffer = SampleBuffer::default();
        buffer.append(&vec![1; CAPACITY + 10]);
        assert_eq!(buffer.total(), CAPACITY + 10);
        assert_eq!(buffer.slice_from(0).len(), CAPACITY);
        assert_eq!(rms(&[]), 0.0);
    }
}
