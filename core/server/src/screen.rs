//! Lettura privata dello schermo (ADR-0122).
//!
//! La schermata si cattura con gli strumenti del sistema, si converte in testo **sul computer** con OCR locale
//! (Tesseract) e il file viene cancellato subito, anche in caso di errore. L'immagine non viene salvata né inviata:
//! solo il testo, già mascherato (`privacy::redact`), arriva al proprietario. Il testo dello schermo non viene
//! mai passato automaticamente ai modelli cloud. Ogni lettura richiede la conferma del proprietario.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::ToolError;

const MAX_TEXT_CHARS: usize = 6_000;
const STALE_AFTER: Duration = Duration::from_secs(600);

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("screen")
}

/// Cartella privata per le catture temporanee; elimina quelle rimaste da oltre 10 minuti.
fn prepare(data_dir: &Path) -> Result<PathBuf, ToolError> {
    let d = dir(data_dir);
    std::fs::create_dir_all(&d)
        .map_err(|_| err("screen.unavailable", "Cartella temporanea non creabile"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    }
    for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_AFTER);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(d)
}

/// Elimina tutte le catture rimaste (all'avvio e alla chiusura).
pub fn purge(data_dir: &Path) {
    for entry in std::fs::read_dir(dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
    {
        let _ = std::fs::remove_file(entry.path());
    }
}

#[derive(Debug)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Comando di cattura per un sistema operativo (`macos` o `windows`).
pub fn capture_plan(os: &str, out: &Path) -> Result<Plan, ToolError> {
    let out = out
        .to_str()
        .ok_or_else(|| err("screen.unavailable", "Percorso non valido"))?
        .to_string();
    match os {
        "macos" => Ok(Plan {
            program: "/usr/sbin/screencapture".into(),
            args: vec!["-x".into(), "-t".into(), "png".into(), out],
            env: vec![],
        }),
        "windows" => Ok(Plan {
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $b=[System.Windows.Forms.SystemInformation]::VirtualScreen; $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height; $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left,$b.Top,0,0,$bmp.Size); $bmp.Save($env:JARVIS_OUT,[System.Drawing.Imaging.ImageFormat]::Png)".into(),
            ],
            env: vec![("JARVIS_OUT".into(), out)],
        }),
        _ => Err(err("screen.unavailable", "La lettura dello schermo è disponibile su macOS e Windows")),
    }
}

fn tesseract_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        "/opt/homebrew/bin/tesseract".into(),
        "/usr/local/bin/tesseract".into(),
        "/usr/bin/tesseract".into(),
    ];
    if let Some(root) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(root).join("Tesseract-OCR/tesseract.exe"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Programs/Tesseract-OCR/tesseract.exe"));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    // Nel PATH.
    let name = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

fn run(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    limit: Duration,
) -> Result<(bool, String, String), ToolError> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramFiles",
        "TESSDATA_PREFIX",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    crate::hide_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|_| {
        err(
            "screen.unavailable",
            "Avvio del comando di sistema non riuscito",
        )
    })?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err(
                    "screen.timeout",
                    "Operazione interrotta: tempo massimo superato",
                ));
            }
        }
    };
    let read = |s: Option<&mut dyn Read>| {
        let mut t = String::new();
        if let Some(s) = s {
            let _ = s.take(2 * 1024 * 1024).read_to_string(&mut t);
        }
        t
    };
    let out = read(child.stdout.as_mut().map(|s| s as &mut dyn Read));
    let errors = read(child.stderr.as_mut().map(|s| s as &mut dyn Read));
    Ok((status.success(), out, errors))
}

/// Pulisce il testo dell'OCR: righe vuote ripetute e spazi finali.
pub fn tidy(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut blank = false;
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            if !blank && !lines.is_empty() {
                lines.push("");
            }
            blank = true;
        } else {
            lines.push(line);
            blank = false;
        }
    }
    lines.join("\n").trim().to_string()
}

pub fn status() -> Value {
    let os = std::env::consts::OS;
    let supported = matches!(os, "macos" | "windows");
    let ocr = tesseract_path().is_some();
    json!({
        "supported": supported, "ocr_installed": ocr, "ready": supported && ocr,
        "hint": if !supported { "Disponibile su macOS e Windows" }
                else if !ocr { "Installa Tesseract OCR (macOS: brew install tesseract tesseract-lang; Windows: winget install UB-Mannheim.TesseractOCR)" }
                else if os == "macos" { "Se il testo risulta vuoto, abilita JARVIS in Impostazioni di sistema → Privacy e sicurezza → Registrazione schermo" }
                else { "Pronto" },
        "privacy": "L'immagine resta sul computer, viene eliminata subito e solo il testo mascherato ti viene mostrato",
    })
}

/// Cattura, legge con OCR locale, elimina l'immagine e restituisce il testo con i dati sensibili mascherati.
pub fn read(data_dir: &Path) -> Result<Value, ToolError> {
    let os = std::env::consts::OS;
    let tesseract = tesseract_path().ok_or_else(|| {
        err("screen.ocr_missing", "Manca l'OCR locale: installa Tesseract (macOS: brew install tesseract tesseract-lang; Windows: winget install UB-Mannheim.TesseractOCR)")
    })?;
    let folder = prepare(data_dir)?;
    let file = folder.join(format!("{}.png", uuid::Uuid::new_v4().simple()));
    let plan = capture_plan(os, &file)?;
    let result = (|| {
        let (ok, _, errors) = run(
            &plan.program,
            &plan.args,
            &plan.env,
            Duration::from_secs(15),
        )?;
        if !ok || !file.is_file() {
            let hint = if errors.to_lowercase().contains("could not create image")
                || errors.to_lowercase().contains("not permitted")
            {
                "macOS non consente la registrazione dello schermo: abilita JARVIS in Impostazioni di sistema → Privacy e sicurezza → Registrazione schermo"
            } else {
                "Cattura dello schermo non riuscita"
            };
            return Err(err("screen.capture_failed", hint));
        }
        let image = file.to_string_lossy().to_string();
        let mut attempt = run(
            &tesseract.to_string_lossy(),
            &[
                image.clone(),
                "stdout".into(),
                "-l".into(),
                "ita+eng".into(),
            ],
            &[],
            Duration::from_secs(60),
        )?;
        if !attempt.0 && attempt.2.to_lowercase().contains("failed loading language") {
            attempt = run(
                &tesseract.to_string_lossy(),
                &[image, "stdout".into(), "-l".into(), "eng".into()],
                &[],
                Duration::from_secs(60),
            )?;
        }
        if !attempt.0 {
            return Err(err(
                "screen.ocr_failed",
                "La lettura del testo non è riuscita",
            ));
        }
        Ok(tidy(&attempt.1))
    })();
    // L'immagine non resta mai sul disco, qualunque sia l'esito.
    let _ = std::fs::remove_file(&file);
    let raw = result?;
    let (masked, redactions) = crate::privacy::redact(&raw);
    let truncated = masked.chars().count() > MAX_TEXT_CHARS;
    let text: String = masked.chars().take(MAX_TEXT_CHARS).collect();
    Ok(json!({
        "text": text, "chars": text.chars().count(), "truncated": truncated, "redactions": redactions,
        "empty": text.trim().is_empty(),
        "note": "Testo letto in locale; dati sensibili mascherati; immagine eliminata",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_commands_exist_for_both_systems_and_pass_paths_safely() {
        let path = Path::new("/tmp/jarvis/screen/abc.png");
        let mac = capture_plan("macos", path).unwrap();
        assert_eq!(mac.program, "/usr/sbin/screencapture");
        assert_eq!(mac.args.last().unwrap(), "/tmp/jarvis/screen/abc.png");
        let win = capture_plan("windows", path).unwrap();
        assert_eq!(
            win.env,
            vec![(
                "JARVIS_OUT".to_string(),
                "/tmp/jarvis/screen/abc.png".to_string()
            )]
        );
        assert!(win.args.iter().all(|a| !a.contains("abc.png")));
        assert_eq!(
            capture_plan("linux", path).unwrap_err().code,
            "screen.unavailable"
        );
    }

    #[test]
    fn ocr_text_is_tidied() {
        assert_eq!(
            tidy("\n\nRiga uno   \n\n\n\nRiga due\n   \n"),
            "Riga uno\n\nRiga due"
        );
        assert_eq!(tidy("   \n"), "");
    }

    #[test]
    fn stale_captures_are_purged_and_the_folder_is_private() {
        let d =
            std::env::temp_dir().join(format!("jarvis-screen-{}", uuid::Uuid::new_v4().simple()));
        let folder = prepare(&d).unwrap();
        std::fs::write(folder.join("a.png"), b"x").unwrap();
        purge(&d);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn status_explains_what_is_missing() {
        let s = status();
        assert!(s["privacy"].as_str().unwrap().contains("eliminata"));
        assert!(s["hint"].as_str().is_some());
    }
}
