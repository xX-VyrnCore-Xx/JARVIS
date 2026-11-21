//! Azioni desktop richieste esplicitamente. Ogni destinazione è definita dal Core.

use std::path::Path;

use serde_json::{json, Value};

use crate::tools::ToolError;

pub(crate) const TOOL: &str = "pc.open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Workspace,
    Calculator,
    Settings,
    Activity,
}

impl Target {
    pub(crate) fn from_request(message: &str) -> Option<Self> {
        let command = message
            .trim()
            .trim_end_matches(['.', '!', '?'])
            .to_lowercase();
        match command
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str()
        {
            "apri workspace" | "apri la workspace" | "apri la cartella di jarvis" => {
                Some(Self::Workspace)
            }
            "apri calcolatrice" | "apri la calcolatrice" => Some(Self::Calculator),
            "apri impostazioni di sistema" | "apri le impostazioni di sistema" => {
                Some(Self::Settings)
            }
            "apri monitoraggio attività" | "apri gestione attività" | "apri task manager" => {
                Some(Self::Activity)
            }
            _ => None,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Calculator => "calculator",
            Self::Settings => "settings",
            Self::Activity => "activity",
        }
    }

    pub(crate) fn parse(id: &str) -> Option<Self> {
        match id {
            "workspace" => Some(Self::Workspace),
            "calculator" => Some(Self::Calculator),
            "settings" => Some(Self::Settings),
            "activity" => Some(Self::Activity),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Workspace => "la cartella di JARVIS",
            Self::Calculator => "la Calcolatrice",
            Self::Settings => "le Impostazioni di sistema",
            Self::Activity => "il monitor delle attività",
        }
    }
}

fn unavailable(message: &str) -> ToolError {
    ToolError {
        code: "pc.unavailable",
        message: message.into(),
    }
}

pub(crate) fn open(target: Target, workspace: &Path) -> Result<Value, ToolError> {
    let mut cmd = command(target, workspace)?;
    crate::hide_console(&mut cmd);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // I processi avviati non devono ereditare token o credenziali di JARVIS.
    cmd.env_clear();
    for key in [
        "HOME",
        "USERPROFILE",
        "SystemRoot",
        "WINDIR",
        "TMP",
        "TEMP",
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| unavailable("Avvio dell’applicazione di sistema non riuscito"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(json!({ "target": target.id(), "label": target.label() }))
}

#[cfg(target_os = "macos")]
fn command(target: Target, workspace: &Path) -> Result<std::process::Command, ToolError> {
    let mut cmd = std::process::Command::new("/usr/bin/open");
    match target {
        Target::Workspace => {
            cmd.arg("--").arg(workspace);
        }
        Target::Calculator => {
            cmd.args(["-b", "com.apple.calculator"]);
        }
        Target::Settings => {
            cmd.args(["-b", "com.apple.systempreferences"]);
        }
        Target::Activity => {
            cmd.args(["-b", "com.apple.ActivityMonitor"]);
        }
    }
    Ok(cmd)
}

#[cfg(windows)]
fn command(target: Target, workspace: &Path) -> Result<std::process::Command, ToolError> {
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| unavailable("Cartella di sistema di Windows non disponibile"))?;
    let executable = match target {
        Target::Workspace | Target::Settings => "explorer.exe",
        Target::Calculator => "System32/calc.exe",
        Target::Activity => "System32/Taskmgr.exe",
    };
    let mut cmd = std::process::Command::new(system.join(executable));
    match target {
        Target::Workspace => {
            cmd.arg(workspace);
        }
        Target::Settings => {
            cmd.arg("ms-settings:");
        }
        _ => {}
    }
    Ok(cmd)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn command(_: Target, _: &Path) -> Result<std::process::Command, ToolError> {
    Err(unavailable(
        "Le azioni desktop sono disponibili su macOS e Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_desktop_targets_are_accepted() {
        assert_eq!(
            Target::from_request("Apri la calcolatrice!"),
            Some(Target::Calculator)
        );
        assert_eq!(
            Target::from_request("Apri la cartella di JARVIS"),
            Some(Target::Workspace)
        );
        assert_eq!(
            Target::from_request("dimmi come aprire la calcolatrice"),
            None
        );
        assert_eq!(Target::from_request("apri calcolatrice e fai altro"), None);
        assert_eq!(Target::parse("calc.exe"), None);
    }
}
