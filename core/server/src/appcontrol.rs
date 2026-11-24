//! Controllo delle applicazioni del computer (ADR-0121): aprire un'app, portarla in primo piano, vedere le finestre
//! aperte, scrivere e premere tasti al posto del proprietario.
//!
//! Sicurezza: ogni azione che agisce sul computer richiede la conferma del proprietario, che solo il proprietario
//! dal computer del Core può dare. I valori forniti dal modello non finiscono mai nel testo di uno script: passano
//! come argomenti o variabili d'ambiente (`osascript` con `argv`, PowerShell con variabili); i soli frammenti di script
//! variabili vengono da tabelle fisse. Nessuna credenziale può essere digitata. macOS richiede il permesso
//! «Accessibilità» e Windows non può scrivere nelle finestre avviate come amministratore.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::tools::ToolError;

const MAX_TEXT: usize = 500;

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

/// Strumenti che agiscono sul computer: richiesta diretta del proprietario e conferma solo da lui.
pub fn is_control_tool(tool: &str) -> bool {
    matches!(
        tool,
        "app.open"
            | "app.focus"
            | "app.type"
            | "app.key"
            | "app.windows"
            | "screen.read"
            | "screen.status"
    )
}

/// Richieste di controllo delle app dal testo: vietate ai dispositivi associati.
pub fn is_app_request(message: &str) -> bool {
    let m = message.trim().to_lowercase();
    [
        "apri l'app",
        "apri l'applicazione",
        "apri il programma",
        "avvia l'app",
        "avvia l'applicazione",
        "avvia il programma",
        "lancia l'app",
        "lancia l'applicazione",
        "lancia il programma",
        "porta in primo piano",
        "metti in primo piano",
        "scrivi nell'app",
        "scrivi nella finestra",
        "scrivi nell'applicazione",
        "digita nell'app",
        "premi ",
        "cosa c'è sullo schermo",
        "cosa vedi sullo schermo",
        "leggi lo schermo",
        "leggi il mio schermo",
        "stato della lettura dello schermo",
        "puoi leggere lo schermo",
        "quali app",
        "quali applicazioni",
        "quali finestre",
        "finestre aperte",
        "app aperte",
    ]
    .iter()
    .any(|p| m.starts_with(p) || (p.len() > 12 && m.contains(p)))
        || message.trim().starts_with("apri ")
            && message
                .trim()
                .chars()
                .nth(5)
                .is_some_and(char::is_uppercase)
}

/// Nome di un'applicazione: lettere, cifre, spazi e `.+_-`; nessun percorso né carattere di shell.
pub fn valid_app_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.chars().count() <= 60
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '+' | '_' | '-'))
        && !name.starts_with('-')
        && !name.starts_with('.')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open(String),
    Focus(String),
    Windows,
    Type(String),
    Key { key: String, modifiers: Vec<String> },
}

const MODIFIERS: [&str; 4] = ["command", "control", "option", "shift"];
const SPECIAL_KEYS: [&str; 9] = [
    "return", "tab", "escape", "space", "delete", "left", "right", "up", "down",
];

/// Normalizza i sinonimi dei modificatori; `None` se sconosciuto.
fn modifier(name: &str) -> Option<&'static str> {
    match name.to_lowercase().as_str() {
        "cmd" | "command" | "comando" => Some("command"),
        "ctrl" | "control" | "controllo" => Some("control"),
        "alt" | "option" | "opzione" => Some("option"),
        "shift" | "maiusc" | "maiuscolo" => Some("shift"),
        _ => None,
    }
}

/// Costruisce l'azione dagli argomenti dello strumento, validandoli.
pub fn parse(tool: &str, args: &Value) -> Result<Action, ToolError> {
    let name = || {
        let n = args["app"].as_str().unwrap_or_default().trim().to_string();
        if valid_app_name(&n) {
            Ok(n)
        } else {
            Err(err(
                "app.invalid",
                "Nome dell'applicazione non valido: usa solo il nome, senza percorsi né simboli",
            ))
        }
    };
    match tool {
        "app.open" => Ok(Action::Open(name()?)),
        "app.focus" => Ok(Action::Focus(name()?)),
        "app.windows" => Ok(Action::Windows),
        "app.type" => {
            let text = args["text"].as_str().unwrap_or_default();
            if text.is_empty() || text.chars().count() > MAX_TEXT || text.contains('\0') {
                return Err(err(
                    "app.invalid",
                    "Il testo deve avere da 1 a 500 caratteri",
                ));
            }
            if crate::cloud::contains_credentials(text) {
                return Err(err(
                    "app.sensitive",
                    "JARVIS non digita credenziali o chiavi: scrivile tu",
                ));
            }
            Ok(Action::Type(text.to_string()))
        }
        "app.key" => {
            let key = args["key"].as_str().unwrap_or_default().to_lowercase();
            let single = key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
            if !single && !SPECIAL_KEYS.contains(&key.as_str()) {
                return Err(err("app.invalid", "Tasto non ammesso: una lettera, una cifra, invio, tab, esc, spazio, canc o le frecce"));
            }
            let mut modifiers = Vec::new();
            for m in args["modifiers"].as_array().into_iter().flatten() {
                let m = m.as_str().and_then(modifier).ok_or_else(|| {
                    err(
                        "app.invalid",
                        "Modificatore non valido: command, control, option o shift",
                    )
                })?;
                if !modifiers.contains(&m.to_string()) {
                    modifiers.push(m.to_string());
                }
            }
            Ok(Action::Key { key, modifiers })
        }
        _ => Err(err("app.invalid", "Strumento sconosciuto")),
    }
}

/// Programma, argomenti e variabili d'ambiente da eseguire.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

fn osascript(script: &[&str], argv: &[&str]) -> Plan {
    let mut args = Vec::new();
    for line in script {
        args.push("-e".to_string());
        args.push((*line).to_string());
    }
    args.push("--".into());
    args.extend(argv.iter().map(|s| (*s).to_string()));
    Plan {
        program: "/usr/bin/osascript".into(),
        args,
        env: Vec::new(),
    }
}

fn powershell(script: &str, env: Vec<(String, String)>) -> Plan {
    Plan {
        program: "powershell.exe".into(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script.into(),
        ],
        env,
    }
}

fn mac_key_code(key: &str) -> Option<u32> {
    Some(match key {
        "return" => 36,
        "tab" => 48,
        "escape" => 53,
        "space" => 49,
        "delete" => 51,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        _ => return None,
    })
}

fn sendkeys_key(key: &str) -> String {
    match key {
        "return" => "{ENTER}".into(),
        "tab" => "{TAB}".into(),
        "escape" => "{ESC}".into(),
        "space" => " ".into(),
        "delete" => "{BACKSPACE}".into(),
        "left" => "{LEFT}".into(),
        "right" => "{RIGHT}".into(),
        "down" => "{DOWN}".into(),
        "up" => "{UP}".into(),
        other => other.to_string(),
    }
}

/// Piano di esecuzione per un sistema operativo (`macos` o `windows`).
pub fn plan(os: &str, action: &Action) -> Result<Plan, ToolError> {
    match (os, action) {
        ("macos", Action::Open(app)) => Ok(Plan { program: "/usr/bin/open".into(), args: vec!["-a".into(), app.clone()], env: vec![] }),
        ("macos", Action::Focus(app)) => Ok(osascript(
            &["on run argv", "tell application (item 1 of argv) to activate", "end run"],
            &[app],
        )),
        ("macos", Action::Windows) => Ok(osascript(
            &["tell application \"System Events\" to get name of (processes where background only is false)"],
            &[],
        )),
        ("macos", Action::Type(text)) => Ok(osascript(
            &["on run argv", "tell application \"System Events\" to keystroke (item 1 of argv)", "end run"],
            &[text],
        )),
        ("macos", Action::Key { key, modifiers }) => {
            let using = if modifiers.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = modifiers.iter().filter(|m| MODIFIERS.contains(&m.as_str())).map(|m| format!("{m} down")).collect();
                format!(" using {{{}}}", list.join(", "))
            };
            let line = match mac_key_code(key) {
                Some(code) => format!("tell application \"System Events\" to key code {code}{using}"),
                None => format!("tell application \"System Events\" to keystroke (item 1 of argv){using}"),
            };
            Ok(osascript(&["on run argv", &line, "end run"], &[key]))
        }
        ("windows", Action::Open(app)) => Ok(powershell(
            "Start-Process -FilePath $env:JARVIS_APP",
            vec![("JARVIS_APP".into(), app.clone())],
        )),
        ("windows", Action::Focus(app)) => Ok(powershell(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($env:JARVIS_APP)",
            vec![("JARVIS_APP".into(), app.clone())],
        )),
        ("windows", Action::Windows) => Ok(powershell(
            "Get-Process | Where-Object { $_.MainWindowTitle } | ForEach-Object { $_.ProcessName + '|' + $_.MainWindowTitle }",
            vec![],
        )),
        ("windows", Action::Type(text)) => Ok(powershell(
            "Add-Type -AssemblyName System.Windows.Forms; $t = $env:JARVIS_TEXT -replace '([+^%~(){}\\[\\]])','{$1}'; $t = $t -replace \"`r?`n\",'{ENTER}'; [System.Windows.Forms.SendKeys]::SendWait($t)",
            vec![("JARVIS_TEXT".into(), text.clone())],
        )),
        ("windows", Action::Key { key, modifiers }) => {
            let prefix: String = modifiers
                .iter()
                .map(|m| match m.as_str() {
                    "command" | "control" => "^",
                    "option" => "%",
                    _ => "+",
                })
                .collect();
            Ok(powershell(
                "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait($env:JARVIS_KEYS)",
                vec![("JARVIS_KEYS".into(), format!("{prefix}{}", sendkeys_key(key)))],
            ))
        }
        _ => Err(err("app.unavailable", "Il controllo delle applicazioni è disponibile su macOS e Windows")),
    }
}

fn run(plan: &Plan) -> Result<String, ToolError> {
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Il processo non eredita token né credenziali di JARVIS.
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
        "ProgramFiles(x86)",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    for (k, v) in &plan.env {
        cmd.env(k, v);
    }
    crate::hide_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|_| {
        err(
            "app.unavailable",
            "Avvio del comando di sistema non riuscito",
        )
    })?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err(
                    "app.timeout",
                    "L'applicazione non ha risposto in tempo",
                ));
            }
        }
    };
    let read = |stream: Option<&mut dyn Read>| {
        let mut text = String::new();
        if let Some(stream) = stream {
            let _ = stream.take(16_384).read_to_string(&mut text);
        }
        text
    };
    let stdout = read(child.stdout.as_mut().map(|s| s as &mut dyn Read));
    let stderr = read(child.stderr.as_mut().map(|s| s as &mut dyn Read));
    if status.success() {
        return Ok(stdout);
    }
    Err(classify_failure(&stderr))
}

/// Traduce l'errore del sistema in un messaggio con la correzione.
pub fn classify_failure(stderr: &str) -> ToolError {
    let t = stderr.to_lowercase();
    if t.contains("assistive")
        || t.contains("not allowed to send keystrokes")
        || t.contains("(1002)")
    {
        err("app.permission", "macOS non consente a JARVIS di controllare altre app: abilita JARVIS in Impostazioni di sistema → Privacy e sicurezza → Accessibilità")
    } else if t.contains("can't get application")
        || t.contains("application isn't running")
        || t.contains("unable to find application")
        || t.contains("cannot find the file")
        || t.contains("cannot find a process")
        || t.contains("not found")
    {
        err(
            "app.not_found",
            "Applicazione non trovata: controlla il nome esatto o che sia installata",
        )
    } else {
        err(
            "app.failed",
            "Il sistema ha rifiutato l'azione: controlla che l'app sia aperta e in primo piano",
        )
    }
}

/// Esegue l'azione sul sistema in uso.
pub fn execute(tool: &str, args: &Value) -> Result<Value, ToolError> {
    let action = parse(tool, args)?;
    let os = std::env::consts::OS;
    let planned = plan(os, &action)?;
    let out = run(&planned)?;
    Ok(match &action {
        Action::Open(app) => json!({"opened": app}),
        Action::Focus(app) => json!({"focused": app}),
        Action::Type(text) => json!({"typed_chars": text.chars().count()}),
        Action::Key { key, modifiers } => json!({"key": key, "modifiers": modifiers}),
        Action::Windows => json!({"windows": parse_windows(os, &out)}),
    })
}

/// Finestre/applicazioni aperte dall'output del sistema.
pub fn parse_windows(os: &str, out: &str) -> Vec<Value> {
    if os == "macos" {
        out.trim()
            .split(", ")
            .filter(|n| !n.is_empty())
            .take(60)
            .map(|n| json!({"app": n.trim()}))
            .collect()
    } else {
        out.lines()
            .filter_map(|l| l.split_once('|'))
            .take(60)
            .map(|(app, title)| json!({"app": app.trim(), "title": title.trim().chars().take(120).collect::<String>()}))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_cannot_smuggle_paths_or_shell_syntax() {
        for ok in [
            "Spotify",
            "Visual Studio Code",
            "Google Chrome",
            "7-Zip",
            "notepad++",
        ] {
            assert!(valid_app_name(ok), "{ok}");
        }
        for bad in [
            "",
            "../x",
            "/bin/sh",
            "a;b",
            "a&b",
            "a|b",
            "a`b",
            "a$b",
            "a\"b",
            "-rf",
            ".hidden",
            "C:\\x",
            &"x".repeat(61),
        ] {
            assert!(!valid_app_name(bad), "{bad}");
        }
    }

    #[test]
    fn arguments_are_validated_and_credentials_are_never_typed() {
        assert!(parse("app.open", &json!({"app": "Spotify"})).is_ok());
        assert_eq!(
            parse("app.open", &json!({"app": "a;b"})).unwrap_err().code,
            "app.invalid"
        );
        assert_eq!(
            parse("app.type", &json!({"text": "nvapi-AbCdEf0123456789xyz"}))
                .unwrap_err()
                .code,
            "app.sensitive"
        );
        assert!(parse("app.type", &json!({"text": "x".repeat(501)})).is_err());
        assert!(parse("app.type", &json!({"text": "Ciao a tutti"})).is_ok());
        let key = parse(
            "app.key",
            &json!({"key": "C", "modifiers": ["cmd", "Maiusc", "cmd"]}),
        )
        .unwrap();
        assert_eq!(
            key,
            Action::Key {
                key: "c".into(),
                modifiers: vec!["command".into(), "shift".into()]
            }
        );
        for bad in [
            json!({"key": "f4"}),
            json!({"key": "ab"}),
            json!({"key": "c", "modifiers": ["super"]}),
            json!({"key": "\\"}),
        ] {
            assert!(parse("app.key", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn untrusted_values_travel_as_arguments_or_environment_never_inside_scripts() {
        let hostile = "x\" & do shell script \"rm -rf ~\" & \"";
        let mac = plan("macos", &Action::Type(hostile.into())).unwrap();
        assert!(mac
            .args
            .iter()
            .take(mac.args.len() - 1)
            .all(|a| !a.contains("rm -rf")));
        assert_eq!(mac.args.last().unwrap(), hostile);
        assert!(mac.args.contains(&"--".to_string()));
        let win = plan("windows", &Action::Type(hostile.into())).unwrap();
        assert!(win.args.iter().all(|a| !a.contains("rm -rf")));
        assert_eq!(
            win.env,
            vec![("JARVIS_TEXT".to_string(), hostile.to_string())]
        );
        let open = plan("windows", &Action::Open("Spotify".into())).unwrap();
        assert_eq!(
            open.env,
            vec![("JARVIS_APP".to_string(), "Spotify".to_string())]
        );
        assert_eq!(
            plan("macos", &Action::Open("Spotify".into())).unwrap().args,
            vec!["-a", "Spotify"]
        );
    }

    #[test]
    fn keys_map_to_each_systems_syntax() {
        let cmd_c = Action::Key {
            key: "c".into(),
            modifiers: vec!["command".into()],
        };
        let mac = plan("macos", &cmd_c).unwrap();
        assert!(mac
            .args
            .iter()
            .any(|a| a.contains("keystroke (item 1 of argv) using {command down}")));
        let win = plan("windows", &cmd_c).unwrap();
        assert_eq!(win.env, vec![("JARVIS_KEYS".to_string(), "^c".to_string())]);
        let enter = Action::Key {
            key: "return".into(),
            modifiers: vec![],
        };
        assert!(plan("macos", &enter)
            .unwrap()
            .args
            .iter()
            .any(|a| a.contains("key code 36")));
        assert_eq!(plan("windows", &enter).unwrap().env[0].1, "{ENTER}");
        assert_eq!(plan("linux", &enter).unwrap_err().code, "app.unavailable");
    }

    #[test]
    fn failures_and_window_lists_are_interpreted() {
        assert_eq!(
            classify_failure("execution error: osascript is not allowed assistive access. (-1719)")
                .code,
            "app.permission"
        );
        assert_eq!(
            classify_failure("Unable to find application named 'Foo'").code,
            "app.not_found"
        );
        assert_eq!(classify_failure("boh").code, "app.failed");
        let mac = parse_windows("macos", "Finder, Safari, Spotify\n");
        assert_eq!(mac.len(), 3);
        let win = parse_windows("windows", "chrome|Google\nnotepad|Senza nome\nmalformed\n");
        assert_eq!(win.len(), 2);
        assert_eq!(win[0]["title"], "Google");
    }

    #[test]
    fn app_requests_are_recognised_but_ordinary_sentences_are_not() {
        for yes in [
            "apri l'app Spotify",
            "Avvia il programma Word",
            "porta in primo piano Chrome",
            "premi cmd+c",
            "scrivi nell'app: ciao",
            "quali finestre sono aperte",
            "apri Spotify",
        ] {
            assert!(is_app_request(yes), "{yes}");
        }
        for no in [
            "apri la workspace",
            "cerca una ricetta",
            "che ore sono",
            "apri note.txt",
        ] {
            assert!(!is_app_request(no), "{no}");
        }
    }
}
