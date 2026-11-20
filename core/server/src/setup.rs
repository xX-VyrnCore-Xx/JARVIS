//! Configurazione guidata al primo avvio (ADR-0019): download della voce locale con avanzamento.
//!
//! Gli installer non eseguono codice durante l'installazione (il `.dmg` di macOS non può farlo), quindi la
//! scelta dei modelli avviene al primo avvio, con opzioni già selezionate. I pacchetti Python vanno in
//! `<dati>/python-packages` e i modelli in `<dati>/models`: l'app installata resta in sola lettura.
//! I passi girano in un thread separato, senza il blocco del Core; al termine il worker AI viene riavviato
//! con i nuovi percorsi. Con `JARVIS_SETUP_FAKE=1` i passi sono simulati (test ed E2E).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

pub const STT_MODELS: [&str; 4] = ["tiny", "base", "small", "medium"];
pub const TTS_VOICES: [&str; 3] = [
    "it_IT-paola-medium",
    "it_IT-riccardo-x_low",
    "en_US-lessac-medium",
];
#[derive(Clone, Debug)]
pub struct Plan {
    pub stt_model: String,
    pub tts_voice: String,
}

impl Plan {
    /// Valida la scelta dell'interfaccia: solo valori dagli elenchi noti (niente argomenti arbitrari a pip).
    pub fn from_json(body: &Value) -> Result<Self, String> {
        let pick = |key: &str, allowed: &[&str], default: &str| -> Result<String, String> {
            let v = body[key].as_str().unwrap_or(default);
            if allowed.contains(&v) {
                Ok(v.to_string())
            } else {
                Err(format!("valore non ammesso per {key}: {v}"))
            }
        };
        if body["voice"] == false {
            return Err("scegli la configurazione della voce".into());
        }
        if ["ollama_model", "embed_model"]
            .iter()
            .any(|key| !body[key].is_null() && body[key].as_str() != Some(""))
        {
            return Err("i modelli AI si configurano in Sistema → Modelli cloud".into());
        }
        Ok(Self {
            stt_model: pick("stt_model", &STT_MODELS, "small")?,
            tts_voice: pick("tts_voice", &TTS_VOICES, "it_IT-paola-medium")?,
        })
    }

    fn to_json(&self) -> Value {
        json!({"voice": true, "stt_model": self.stt_model, "tts_voice": self.tts_voice})
    }
}

#[derive(Default)]
struct State {
    running: bool,
    steps: Vec<Value>,
    plan: Option<Value>,
    finished_at: Option<String>,
}

pub struct Setup {
    data_dir: PathBuf,
    python: String,
    ai_dir: PathBuf,
    state: Mutex<State>,
    restart_worker: AtomicBool,
}

impl Setup {
    pub fn new(data_dir: &Path, python: &str, ai_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            python: python.to_string(),
            ai_dir: ai_dir.to_path_buf(),
            state: Mutex::default(),
            restart_worker: AtomicBool::new(false),
        }
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.data_dir.join("python-packages")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    fn record_path(&self) -> PathBuf {
        self.data_dir.join("setup.json")
    }

    /// Ultima configurazione completata (scelte salvate), se esiste.
    pub fn saved(&self) -> Option<Value> {
        std::fs::read(self.record_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }

    /// Variabili per il worker AI, derivate da ciò che la configurazione ha installato. Non sovrascrivono
    /// quelle impostate dal proprietario.
    pub fn worker_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        let set = |k: &str| std::env::var_os(k).is_some();
        let pkgs = self.packages_dir();
        if pkgs.is_dir() {
            let mut paths = vec![pkgs.clone()];
            if let Some(existing) = std::env::var_os("PYTHONPATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                env.push(("PYTHONPATH".into(), joined.to_string_lossy().into_owned()));
            }
        }
        let models = self.models_dir();
        if models.is_dir() && !set("JARVIS_MODELS_DIR") {
            env.push((
                "JARVIS_MODELS_DIR".into(),
                models.to_string_lossy().into_owned(),
            ));
        }
        if let Some(saved) = self.saved() {
            let plan = &saved["plan"];
            if let Some(voice) = plan["tts_voice"].as_str() {
                let onnx = models.join("piper").join(format!("{voice}.onnx"));
                if onnx.is_file() && !set("JARVIS_TTS_VOICE") {
                    env.push((
                        "JARVIS_TTS_VOICE".into(),
                        onnx.to_string_lossy().into_owned(),
                    ));
                }
            }
            if let Some(stt) = plan["stt_model"].as_str() {
                if !set("JARVIS_STT_MODEL") {
                    env.push(("JARVIS_STT_MODEL".into(), stt.into()));
                }
            }
        }
        env
    }

    /// Vero una volta dopo una configurazione completata: il Core riavvia il worker.
    pub fn take_restart(&self) -> bool {
        self.restart_worker.swap(false, Ordering::SeqCst)
    }

    pub fn status(&self) -> Value {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        json!({
            "first_run": self.saved().is_none(),
            "running": s.running,
            "steps": s.steps,
            "plan": s.plan,
            "finished_at": s.finished_at,
            "saved": self.saved(),
            "detected": {
                "voice_packages": self.packages_dir().join("faster_whisper").is_dir(),
            },
            "choices": {"stt_models": STT_MODELS, "tts_voices": TTS_VOICES},
            "sizes_mb": {"voice_packages": 350, "stt": {"tiny": 75, "base": 145, "small": 470, "medium": 1500}, "tts": 65},
        })
    }

    /// Avvia la configurazione in un thread separato. Errore se ne è già in corso una.
    pub fn start(self: &Arc<Self>, plan: Plan) -> Result<(), String> {
        let steps = vec![
            step(
                "voice.packages",
                "Pacchetti della voce (faster-whisper, Piper, openWakeWord)",
            ),
            step(
                "voice.stt",
                &format!("Riconoscimento vocale: modello «{}»", plan.stt_model),
            ),
            step(
                "voice.tts",
                &format!("Voce di sintesi «{}»", plan.tts_voice),
            ),
            step("voice.wake", "Parola di attivazione «Ehi Jarvis»"),
        ];
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if s.running {
                return Err("una configurazione è già in corso".into());
            }
            *s = State {
                running: true,
                steps,
                plan: Some(plan.to_json()),
                finished_at: None,
            };
        }
        let me = self.clone();
        std::thread::spawn(move || me.run(plan));
        Ok(())
    }

    fn update(&self, id: &str, status: &str, detail: &str, progress: Option<f64>) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = s.steps.iter_mut().find(|x| x["id"] == id) {
            st["status"] = status.into();
            st["detail"] = detail.chars().take(300).collect::<String>().into();
            st["progress"] = progress.map_or(Value::Null, |p| ((p * 1000.0).round() / 10.0).into());
        }
    }

    fn run(&self, plan: Plan) {
        let fake = std::env::var("JARVIS_SETUP_FAKE").is_ok_and(|v| v == "1");
        let ids: Vec<String> = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.steps
                .iter()
                .filter_map(|x| x["id"].as_str().map(str::to_string))
                .collect()
        };
        let mut failed = false;
        for id in &ids {
            // Senza i pacchetti della voce i modelli non si possono scaricare: i passi successivi si saltano.
            if failed {
                self.update(
                    id,
                    "skipped",
                    "saltato: i pacchetti della voce non sono stati installati",
                    None,
                );
                continue;
            }
            self.update(id, "running", "", None);
            let result = if fake {
                self.fake_step(id)
            } else {
                self.real_step(id, &plan)
            };
            match result {
                Ok(detail) => self.update(id, "done", &detail, Some(1.0)),
                Err(e) => {
                    if id == "voice.packages" {
                        failed = true;
                    }
                    self.update(id, "failed", &e, None)
                }
            }
        }
        let at = crate::now();
        let ok_any = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.steps.iter().any(|x| x["status"] == "done")
        };
        if ok_any {
            let record = json!({"done_at": at, "plan": plan.to_json(), "steps": self.state.lock().unwrap_or_else(|e| e.into_inner()).steps});
            let _ = std::fs::write(
                self.record_path(),
                serde_json::to_vec_pretty(&record).unwrap_or_default(),
            );
            self.restart_worker.store(true, Ordering::SeqCst);
        }
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.running = false;
        s.finished_at = Some(at);
    }

    fn fake_step(&self, id: &str) -> Result<String, String> {
        for i in 1..=4 {
            std::thread::sleep(Duration::from_millis(40));
            self.update(id, "running", "simulazione", Some(f64::from(i) / 4.0));
        }
        if std::env::var("JARVIS_SETUP_FAKE_FAIL").is_ok_and(|f| f == id) {
            return Err("errore simulato".into());
        }
        Ok("simulato".into())
    }

    fn python(&self, args: &[&str], id: &str) -> Result<String, String> {
        let mut cmd = Command::new(&self.python);
        crate::hide_console(&mut cmd);
        cmd.args(args)
            .current_dir(&self.ai_dir)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in self.worker_env() {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("avvio di Python non riuscito: {e}"))?;
        // L'ultima riga di output fa da avanzamento leggibile; stderr si conserva per l'errore.
        let stderr = child.stderr.take();
        let err_tail = std::thread::spawn(move || {
            let mut tail = String::new();
            if let Some(e) = stderr {
                for line in BufReader::new(e).lines().map_while(Result::ok) {
                    tail = line;
                }
            }
            tail
        });
        let mut last = String::new();
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if !line.is_empty() {
                    self.update(id, "running", &line, None);
                    last = line;
                }
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let tail = err_tail.join().unwrap_or_default();
        if status.success() {
            Ok(last)
        } else {
            Err(if tail.is_empty() {
                format!("uscita con codice {:?}", status.code())
            } else {
                tail
            })
        }
    }

    fn real_step(&self, id: &str, plan: &Plan) -> Result<String, String> {
        let pkgs = self.packages_dir();
        let models = self.models_dir();
        std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
        let models_s = models.to_string_lossy().into_owned();
        match id {
            "voice.packages" => {
                let req = self.ai_dir.join("requirements-voice.txt");
                std::fs::create_dir_all(&pkgs).map_err(|e| e.to_string())?;
                self.python(
                    &["-m", "pip", "install", "--progress-bar", "off", "--upgrade", "--target",
                      &pkgs.to_string_lossy(), "-r", &req.to_string_lossy()],
                    id,
                )
            }
            "voice.stt" => self.python(
                &["-c", "import sys; from faster_whisper import WhisperModel; WhisperModel(sys.argv[1], device='cpu', compute_type='int8', download_root=sys.argv[2]); print('modello pronto')",
                  &plan.stt_model, &models_s],
                id,
            ),
            "voice.tts" => {
                let dir = models.join("piper");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                self.python(&["-m", "piper.download_voices", &plan.tts_voice, "--download-dir", &dir.to_string_lossy()], id)?;
                if dir.join(format!("{}.onnx", plan.tts_voice)).is_file() {
                    Ok("voce pronta".into())
                } else {
                    Err("il file della voce non è stato scaricato".into())
                }
            }
            "voice.wake" => self.python(
                &["-c", "import openwakeword, os\nreg = getattr(openwakeword, 'MODELS', None) or getattr(openwakeword, 'models', {})\np = reg.get('hey_jarvis', {}).get('model_path', '')\nif not (p and os.path.isfile(p)):\n    from openwakeword import utils\n    utils.download_models(['hey_jarvis'])\nprint('parola di attivazione pronta')"],
                id,
            ),
            other => Err(format!("passo sconosciuto: {other}")),
        }
    }
}

fn step(id: &str, label: &str) -> Value {
    json!({"id": id, "label": label, "status": "pending", "detail": "", "progress": null})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_accepts_only_voice_settings() {
        let plan = Plan::from_json(&json!({})).unwrap();
        assert_eq!(plan.stt_model, "small");
        assert_eq!(plan.tts_voice, "it_IT-paola-medium");
        assert!(Plan::from_json(&json!({"stt_model": "--index-url=http://x"})).is_err());
        assert!(Plan::from_json(&json!({"ollama_model": "qwen3:4b"})).is_err());
        assert!(Plan::from_json(&json!({"embed_model": "nomic-embed-text"})).is_err());
        assert!(Plan::from_json(&json!({"voice": false})).is_err());
    }
}
