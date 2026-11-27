//! Voce di sistema, solo maschile (ADR-0123): legge le risposte ad alta voce con la voce del computer, senza
//! modelli da scaricare. macOS usa `say` con una voce italiana maschile («Luca»); Windows usa `System.Speech`
//! chiedendo una voce maschile italiana («Cosimo»). Una voce femminile non viene mai scelta: se non c'è una voce
//! maschile installata, JARVIS tace e spiega come installarla.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::tools::ToolError;

const MAX_SPOKEN_CHARS: usize = 700;

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

// ------------------------------------------------------------------ testo da leggere

/// Prepara il testo per la voce: niente markdown, codice, indirizzi o emoji; simboli e unità a parole.
pub fn prepare(text: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            if !in_code {
                out.push_str(" Ecco il codice nella finestra. ");
            }
            in_code = !in_code;
            continue;
        }
        if in_code || t.is_empty() {
            if t.is_empty() && !out.ends_with(". ") {
                out.push_str(". ");
            }
            continue;
        }
        let t = t.trim_start_matches(['#', '>']).trim_start();
        let t = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
            .unwrap_or(t);
        out.push_str(t);
        out.push(' ');
    }
    let mut words: Vec<String> = Vec::new();
    for word in out.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            words.push("link".into());
            continue;
        }
        let cleaned: String = word
            .chars()
            .filter(|c| !matches!(c, '*' | '`' | '_' | '~' | '#' | '|') && !is_emoji(*c))
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        words.push(
            cleaned
                .replace('%', " per cento")
                .replace("€", " euro")
                .replace("°C", " gradi")
                .replace('°', " gradi")
                .replace('&', " e "),
        );
    }
    let text = words.join(" ");
    let clipped: String = text.chars().take(MAX_SPOKEN_CHARS).collect();
    // Taglia a fine frase se possibile.
    match clipped.rfind(['.', '!', '?']) {
        Some(end) if text.chars().count() > MAX_SPOKEN_CHARS && end > 120 => {
            clipped[..=end].to_string()
        }
        _ => clipped,
    }
    .trim()
    .to_string()
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F)
}

// ------------------------------------------------------------------ scelta della voce

/// Voci maschili italiane note su macOS, in ordine di preferenza: «Luca» (da scaricare) e le voci già incluse in
/// macOS 13 e successivi («Eddy», «Reed», «Rocko»). I nomi possono avere un suffisso di qualità o di lingua,
/// per esempio «Luca (Premium)» o «Eddy (Italiano (Italia))».
const MAC_MALE: [&str; 4] = ["Luca", "Eddy", "Reed", "Rocko"];

/// Voce maschile dall'elenco di `say -v '?'` (righe «Nome  it_IT  # esempio»). Mai una voce non nota come maschile.
pub fn pick_mac_voice(listing: &str) -> Option<String> {
    let rank = |name: &str| -> Option<(usize, u8)> {
        let base = name.split(" (").next().unwrap_or(name).trim();
        let order = MAC_MALE.iter().position(|m| *m == base)?;
        let quality = if name.contains("Premium") {
            0
        } else if name.contains("Enhanced") {
            1
        } else {
            2
        };
        Some((order, quality))
    };
    listing
        .lines()
        .filter(|l| l.contains("it_IT") || l.contains("it-IT"))
        .filter_map(|l| {
            let name = l.split("  ").next()?.trim();
            rank(name).map(|r| (r, name.to_string()))
        })
        .min_by_key(|(r, _)| *r)
        .map(|(_, name)| name)
}

/// Piano per pronunciare un testo: programma, argomenti e variabili d'ambiente.
#[derive(Debug, PartialEq, Eq)]
pub struct SpeakPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn windows_plan(text: &str) -> SpeakPlan {
    SpeakPlan {
        program: "powershell.exe".into(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            // Solo una voce con genere maschile: se non esiste, esce con errore invece di usare la predefinita.
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             $s.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::Male, [System.Speech.Synthesis.VoiceAge]::NotSet, 0, [System.Globalization.CultureInfo]::GetCultureInfo('it-IT')); \
             if ($s.Voice.Gender -ne 'Male') { exit 3 }; $s.Rate = 0; $s.Speak($env:JARVIS_SPEECH)"
                .into(),
        ],
        env: vec![("JARVIS_SPEECH".into(), text.into())],
    }
}

pub fn mac_plan(voice: &str, text: &str) -> SpeakPlan {
    SpeakPlan {
        program: "/usr/bin/say".into(),
        // «--» impedisce che un testo che inizia con «-» sia letto come opzione; la velocità è un po' più calma.
        args: vec![
            "-v".into(),
            voice.into(),
            "-r".into(),
            "175".into(),
            "--".into(),
            text.into(),
        ],
        env: vec![],
    }
}

/// Processo di lettura in corso: una nuova lettura interrompe la precedente.
fn current() -> &'static Mutex<Option<Child>> {
    static C: std::sync::OnceLock<Mutex<Option<Child>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

pub fn stop() {
    if let Some(mut child) = current().lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Riproduce un file WAV con il lettore di sistema (voce locale, modello Piper): `afplay` su macOS, `SoundPlayer`
/// su Windows. Interrompe la lettura precedente; il file temporaneo è nella cartella indicata e viene tolto da
/// `stop()` o dalla lettura successiva.
pub fn play_wav(dir: &Path, wav: &[u8]) -> Result<Value, ToolError> {
    stop();
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("risposta.wav");
    std::fs::write(&path, wav).map_err(|_| {
        err(
            "speech.unavailable",
            "Impossibile preparare l'audio da riprodurre",
        )
    })?;
    let mut cmd = match std::env::consts::OS {
        "macos" => {
            let mut c = Command::new("/usr/bin/afplay");
            c.arg(&path);
            c
        }
        "windows" => {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command"]);
            c.arg("(New-Object Media.SoundPlayer $env:JARVIS_WAV).PlaySync()");
            c.env("JARVIS_WAV", &path);
            c
        }
        _ => {
            return Err(err(
                "speech.unavailable",
                "La riproduzione è disponibile su macOS e Windows",
            ))
        }
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::hide_console(&mut cmd);
    let child = cmd
        .spawn()
        .map_err(|_| err("speech.unavailable", "Avvio del lettore audio non riuscito"))?;
    *current().lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    Ok(json!({"speaking": true, "engine": "local-model", "gender": "male"}))
}

fn mac_voice() -> Result<String, ToolError> {
    let out = Command::new("/usr/bin/say")
        .args(["-v", "?"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| err("speech.unavailable", "Voce di sistema non disponibile"))?;
    pick_mac_voice(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
        err(
            "speech.no_male_voice",
            "Nessuna voce maschile italiana installata: scarica «Luca» in Impostazioni di sistema → Accessibilità → Contenuto parlato → Voce di sistema → Gestisci voci",
        )
    })
}

/// Legge il testo ad alta voce (in background). Errore se manca una voce maschile.
pub fn speak(text: &str) -> Result<Value, ToolError> {
    let spoken = prepare(text);
    if spoken.is_empty() {
        return Err(err("speech.empty", "Nulla da leggere"));
    }
    let os = std::env::consts::OS;
    let plan = match os {
        "macos" => mac_plan(&mac_voice()?, &spoken),
        "windows" => windows_plan(&spoken),
        _ => {
            return Err(err(
                "speech.unavailable",
                "La voce di sistema è disponibile su macOS e Windows",
            ))
        }
    };
    stop();
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.env_clear();
    for key in [
        "HOME",
        "USERPROFILE",
        "SystemRoot",
        "WINDIR",
        "TMP",
        "TEMP",
        "LANG",
        "PATH",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    for (k, v) in &plan.env {
        cmd.env(k, v);
    }
    crate::hide_console(&mut cmd);
    let child = cmd.spawn().map_err(|_| {
        err(
            "speech.unavailable",
            "Avvio della voce di sistema non riuscito",
        )
    })?;
    *current().lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    Ok(json!({"speaking": true, "chars": spoken.chars().count(), "gender": "male"}))
}

/// Impostazioni vocali (`<dati>/voice.json`): lettura delle risposte e ascolto «hey Jarvis» (ADR-0134).
/// L'ascolto è attivo per impostazione predefinita: in quello stato i client analizzano il microfono solo per
/// riconoscere la parola di attivazione e scartano tutto il resto.
pub fn settings(dir: &Path) -> Value {
    let saved: Value = std::fs::read(dir.join("voice.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    json!({
        "speak_replies": saved["speak_replies"].as_bool().unwrap_or(false),
        "wake_listening": saved["wake_listening"].as_bool().unwrap_or(true),
    })
}

pub fn update(dir: &Path, patch: &Value) -> Result<Value, String> {
    let mut doc = settings(dir);
    let mut changed = false;
    for key in ["speak_replies", "wake_listening"] {
        match patch.get(key) {
            None => {}
            Some(value) => {
                doc[key] = json!(value
                    .as_bool()
                    .ok_or(format!("campo «{key}» booleano richiesto"))?);
                changed = true;
            }
        }
    }
    if !changed {
        return Err("indica «speak_replies» o «wake_listening»".into());
    }
    std::fs::write(dir.join("voice.json"), doc.to_string()).map_err(|e| e.to_string())?;
    if doc["speak_replies"] == false {
        stop();
    }
    Ok(settings(dir))
}

pub fn status(dir: &Path) -> Value {
    let os = std::env::consts::OS;
    let (ready, voice, hint) = match os {
        "macos" => match mac_voice() {
            Ok(v) => (true, Some(v), "Pronta".to_string()),
            Err(e) => (false, None, e.message),
        },
        "windows" => (
            true,
            Some("voce maschile italiana di Windows".to_string()),
            "Pronta se è installata una voce maschile italiana (Cosimo)".to_string(),
        ),
        _ => (false, None, "Disponibile su macOS e Windows".to_string()),
    };
    let mut doc = settings(dir);
    doc["ready"] = json!(ready);
    doc["voice"] = json!(voice);
    doc["gender"] = json!("male");
    doc["hint"] = json!(hint);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_prepared_for_speech() {
        let spoken = prepare("## Titolo\n**Fatto.** Vedi https://example.org per il 12% e 20 €.\n\n```rust\nfn main() {}\n```\n- punto uno 😀");
        for forbidden in ["##", "**", "https", "```", "fn main", "%", "€", "😀"] {
            assert!(!spoken.contains(forbidden), "{forbidden}: {spoken}");
        }
        for expected in ["link", "per cento", "euro", "codice", "punto uno"] {
            assert!(spoken.contains(expected), "{expected}: {spoken}");
        }
        let long = prepare(&"Frase di prova abbastanza lunga. ".repeat(60));
        assert!(
            long.chars().count() <= MAX_SPOKEN_CHARS && long.ends_with('.'),
            "{long}"
        );
        assert_eq!(
            prepare("```\ncodice\n```"),
            "Ecco il codice nella finestra."
        );
    }

    #[test]
    fn only_a_known_male_italian_voice_is_ever_picked() {
        let listing = "Alice               it_IT    # Ciao, mi chiamo Alice.\nLuca                it_IT    # Ciao, mi chiamo Luca.\nFederica            it_IT    # Ciao!\nDaniel              en_GB    # Hello\n";
        assert_eq!(pick_mac_voice(listing).as_deref(), Some("Luca"));
        let better = format!("{listing}Luca (Enhanced)     it_IT    # Ciao\n");
        assert_eq!(pick_mac_voice(&better).as_deref(), Some("Luca (Enhanced)"));
        // Senza Luca si usano le voci maschili già incluse in macOS, anche con il suffisso di lingua.
        let built_in = "Alice  it_IT  # Ciao\nFlo (Italiano (Italia))  it_IT  # Ciao\nRocko (Italiano (Italia))  it_IT  # Ciao\nEddy (Italiano (Italia))  it_IT  # Ciao\n";
        assert_eq!(
            pick_mac_voice(built_in).as_deref(),
            Some("Eddy (Italiano (Italia))")
        );
        // Una voce inglese con lo stesso nome non vale.
        assert_eq!(
            pick_mac_voice("Eddy (English (US))  en_US  # Hello\n"),
            None
        );
        // Solo voci femminili o non italiane: nessuna scelta, mai un ripiego.
        assert_eq!(
            pick_mac_voice("Alice  it_IT  # Ciao\nPaola  it_IT  # Ciao\nDaniel  en_GB  # Hello\n"),
            None
        );
        assert_eq!(pick_mac_voice(""), None);
    }

    #[test]
    fn the_text_travels_as_an_argument_or_environment_never_inside_a_script() {
        let hostile = "-v Alice; rm -rf ~ \"$(whoami)\"";
        let mac = mac_plan("Luca", hostile);
        assert_eq!(mac.args.last().unwrap(), hostile);
        assert_eq!(mac.args[mac.args.len() - 2], "--");
        let win = windows_plan(hostile);
        assert!(win.args.iter().all(|a| !a.contains("rm -rf")));
        assert_eq!(
            win.env,
            vec![("JARVIS_SPEECH".to_string(), hostile.to_string())]
        );
        let script = win.args.last().unwrap();
        assert!(script.contains("VoiceGender]::Male") && script.contains("exit 3"));
    }

    #[test]
    fn the_setting_is_saved_and_defaults_to_silent() {
        let d =
            std::env::temp_dir().join(format!("jarvis-speech-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(settings(&d)["speak_replies"], false);
        assert_eq!(
            update(&d, &json!({"speak_replies": true})).unwrap()["speak_replies"],
            true
        );
        assert!(update(&d, &json!({"speak_replies": "sì"})).is_err());
        // L'ascolto «hey Jarvis» è attivo di default e si disattiva senza toccare la lettura delle risposte.
        assert_eq!(settings(&d)["wake_listening"], true);
        let off = update(&d, &json!({"wake_listening": false})).unwrap();
        assert_eq!(
            (off["wake_listening"].clone(), off["speak_replies"].clone()),
            (json!(false), json!(true))
        );
        assert!(update(&d, &json!({})).is_err());
        assert!(update(&d, &json!({"wake_listening": 1})).is_err());
        assert_eq!(status(&d)["gender"], "male");
        let _ = std::fs::remove_dir_all(&d);
    }
}
