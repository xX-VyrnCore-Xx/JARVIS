//! n8n isolato e gestito da JARVIS. Docker riceve solo comandi fissi e workflow da modelli interni.
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const IMAGE: &str = "docker.n8n.io/n8nio/n8n:2.48.7";
const JOBS: &str = "n8n-jobs.json";

fn docker_binary() -> PathBuf {
    #[cfg(target_os = "macos")]
    for path in [
        "/usr/local/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
    ] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("ProgramFiles") {
        let path = PathBuf::from(root).join("Docker/Docker/resources/bin/docker.exe");
        if path.is_file() {
            return path;
        }
    }
    "docker".into()
}

/// Docker viene usato solo sul computer che ospita il Core.
fn docker_host() -> String {
    #[cfg(unix)]
    {
        let mut sockets = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(home) = std::env::var_os("HOME") {
            sockets.push(PathBuf::from(home).join(".docker/run/docker.sock"));
        }
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            sockets.push(PathBuf::from(dir).join("docker.sock"));
        }
        sockets.push(PathBuf::from("/var/run/docker.sock"));
        if let Some(socket) = sockets
            .iter()
            .find(|socket| socket.is_absolute() && socket.exists())
        {
            return format!("unix://{}", socket.display());
        }
        "unix:///var/run/docker.sock".into()
    }
    #[cfg(windows)]
    {
        "npipe:////./pipe/docker_engine".into()
    }
}

fn docker(args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Result<String, String> {
    let mut cmd = Command::new(docker_binary());
    cmd.env_remove("DOCKER_HOST")
        .env_remove("DOCKER_CONTEXT")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .arg("--host")
        .arg(docker_host());
    crate::hide_console(&mut cmd);
    let mut child = cmd
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Docker non disponibile: {e}"))?;
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let read = |pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let mut pipe = pipe;
            let _ = (&mut pipe).take(128 * 1024).read_to_end(&mut data);
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            String::from_utf8_lossy(&data).into_owned()
        })
    };
    let stdout = read(Box::new(stdout));
    let stderr = read(Box::new(stderr));
    if let Some(input) = input {
        if let Err(error) = child.stdin.take().expect("stdin").write_all(input) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("invio al container non riuscito: {error}"));
        }
    }
    let until = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() >= until => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Docker non ha risposto entro il tempo previsto".into());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(out)
    } else {
        let detail = err
            .lines()
            .find(|line| !line.trim().is_empty() && !line.starts_with(char::is_whitespace))
            .or_else(|| out.lines().rev().find(|line| !line.trim().is_empty()))
            .unwrap_or("errore Docker");
        Err(detail.chars().take(320).collect())
    }
}

pub struct Manager {
    dir: PathBuf,
    name: String,
    mutex: Mutex<()>,
}

impl Manager {
    pub fn enabled(&self) -> bool {
        std::env::var("JARVIS_N8N").is_ok_and(|v| v == "1")
            || self.dir.join("n8n-enabled").is_file()
    }

    pub fn enable(&self) -> Result<Value, String> {
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        crate::backup::atomic_write(&self.dir.join("n8n-enabled"), b"1")
            .map_err(|e| e.to_string())?;
        Ok(self.status())
    }

    pub fn new(dir: PathBuf) -> Self {
        let identity = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        let hash = hex::encode(Sha256::digest(identity.as_os_str().as_encoded_bytes()));
        Self {
            dir,
            name: format!("jarvis-n8n-{}", &hash[..12]),
            mutex: Mutex::new(()),
        }
    }

    fn jobs(&self) -> Vec<Value> {
        std::fs::read(self.dir.join(JOBS))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn state(&self) -> Result<Option<bool>, String> {
        let out = docker(
            &[
                "container",
                "ls",
                "-a",
                "--filter",
                &format!("name=^/{}$", self.name),
                "--format",
                "{{.State}} {{.Label \"local.jarvis.n8n\"}}",
            ],
            None,
            Duration::from_secs(4),
        )?;
        let mut parts = out.split_whitespace();
        match (parts.next(), parts.next()) {
            (None, _) => Ok(None),
            (Some(state), Some("1")) => Ok(Some(state == "running")),
            _ => Err("Il nome del container è già in uso da un'altra applicazione".into()),
        }
    }

    pub fn status(&self) -> Value {
        let jobs = self.jobs();
        if !self.enabled() {
            return json!({"enabled": false, "running": false, "items": jobs, "image": IMAGE});
        }
        match self.state() {
            Ok(state) => {
                json!({"enabled": true, "running": state == Some(true), "docker_connected": true, "container_created": state.is_some(), "items": jobs, "image": IMAGE})
            }
            Err(error) => {
                json!({"enabled": true, "running": false, "docker_connected": false, "items": jobs, "error": error, "image": IMAGE})
            }
        }
    }

    pub fn set_running(&self, running: bool) -> Result<Value, String> {
        self.ensure_enabled()?;
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        match (running, self.state()?) {
            (true, None) => {
                let volume = format!("{}:/home/node/.n8n", self.name);
                docker(&["run", "-d", "--name", &self.name, "--label", "local.jarvis.n8n=1",
                    "--network", "none", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                    "--pids-limit", "256", "--memory", "768m", "--cpus", "1.5", "--restart", "unless-stopped", "--volume", &volume,
                    "-e", "N8N_LISTEN_ADDRESS=127.0.0.1",
                    "-e", "N8N_DIAGNOSTICS_ENABLED=false", "-e", "N8N_VERSION_NOTIFICATIONS_ENABLED=false",
                    "-e", "N8N_TEMPLATES_ENABLED=false", "-e", "N8N_COMMUNITY_PACKAGES_ENABLED=false",
                    "-e", "N8N_BLOCK_ENV_ACCESS_IN_NODE=true",
                    "-e", "NODES_INCLUDE=[\"n8n-nodes-base.scheduleTrigger\",\"n8n-nodes-base.noOp\"]",
                    "-e", "EXECUTIONS_DATA_SAVE_ON_SUCCESS=all", "-e", "EXECUTIONS_DATA_MAX_AGE=24",
                    IMAGE], None, Duration::from_secs(180))?;
            }
            (true, Some(false)) => {
                docker(
                    &["update", "--restart", "unless-stopped", &self.name],
                    None,
                    Duration::from_secs(10),
                )?;
                docker(&["start", &self.name], None, Duration::from_secs(25))?;
            }
            (false, Some(true)) => {
                docker(
                    &["stop", "--time", "12", &self.name],
                    None,
                    Duration::from_secs(20),
                )?;
            }
            _ => (),
        }
        if running {
            let probe = "fetch('http://127.0.0.1:5678/healthz/readiness').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))";
            let until = Instant::now() + Duration::from_secs(35);
            loop {
                if docker(
                    &["exec", &self.name, "node", "-e", probe],
                    None,
                    Duration::from_secs(4),
                )
                .is_ok()
                {
                    break;
                }
                if Instant::now() >= until {
                    return Err("Docker ha avviato il container, ma n8n non è ancora pronto. Controlla lo stato e riprova.".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        Ok(self.status())
    }

    fn ensure_enabled(&self) -> Result<(), String> {
        if self.enabled() {
            Ok(())
        } else {
            Err("Abilita n8n con JARVIS_N8N=1 prima di avviare JARVIS".into())
        }
    }

    fn paused<T>(&self, change: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let check = r#"async function ready() {
          for (let i = 0; i < 120; i++) {
            try { if ((await fetch("http://127.0.0.1:5678/healthz/readiness", {signal:AbortSignal.timeout(800)})).ok) return; } catch {}
            await new Promise(done => setTimeout(done, 1000));
          }
          console.error("n8n non ha ancora concluso l’avvio"); process.exitCode = 1;
        } ready();"#;
        docker(
            &["exec", &self.name, "node", "-e", check],
            None,
            Duration::from_secs(135),
        )
        .map_err(|e| format!("Preparazione di n8n: {e}"))?;
        docker(
            &["stop", "--time", "12", &self.name],
            None,
            Duration::from_secs(45),
        )
        .map_err(|e| format!("Arresto di n8n: {e}"))?;
        let result = change();
        let restarted = docker(&["start", &self.name], None, Duration::from_secs(40));
        match (result, restarted) {
            (Ok(value), Ok(_)) => Ok(value),
            (Ok(_), Err(error)) => Err(format!(
                "Modifica registrata. Riavvio di n8n non riuscito: {error}"
            )),
            (Err(error), Ok(_)) => Err(error),
            (Err(change), Err(restart)) => {
                Err(format!("{change}. Riavvio di n8n non riuscito: {restart}"))
            }
        }
    }

    fn admin(&self, action: &str, id: &str, input: Option<&[u8]>) -> Result<(), String> {
        let volume = format!("{}:/home/node/.n8n", self.name);
        // Il server è fermo durante la modifica. Il container di servizio usa lo stesso volume in esclusiva.
        let script = r#"const {spawnSync} = require("node:child_process");
const {writeFileSync, rmSync} = require("node:fs");
const [action, id] = process.argv.slice(1);
const call = (...args) => { const run = spawnSync("n8n", args, {stdio:"inherit"}); if (run.status !== 0) process.exit(run.status || 1); };
if (action === "create") {
  const path = "/tmp/jarvis-workflow.json";
  writeFileSync(path, require("node:fs").readFileSync(0), {mode:0o600});
  call("import:workflow", "--input", path);
  rmSync(path);
  call("publish:workflow", "--id=" + id);
} else { call("unpublish:workflow", "--id=" + id); }"#;
        let companion = format!("{}-admin-{}", self.name, uuid::Uuid::new_v4().simple());
        let result = docker(
            &[
                "run",
                "--rm",
                "--name",
                &companion,
                "-i",
                "--network",
                "none",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--volume",
                &volume,
                "-e",
                "N8N_DIAGNOSTICS_ENABLED=false",
                "-e",
                "N8N_VERSION_NOTIFICATIONS_ENABLED=false",
                "--entrypoint",
                "node",
                IMAGE,
                "-e",
                script,
                action,
                id,
            ],
            input,
            Duration::from_secs(180),
        );
        if result.is_err() {
            let _ = docker(&["rm", "-f", &companion], None, Duration::from_secs(8));
        }
        result
            .map(|_| ())
            .map_err(|e| format!("Gestione della routine n8n: {e}"))
    }

    pub fn add(&self, input: &Value) -> Result<Value, String> {
        self.ensure_enabled()?;
        let name = input["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| {
                !name.is_empty()
                    && name.chars().count() <= 80
                    && !name.chars().any(char::is_control)
            })
            .ok_or("nome da 1 a 80 caratteri")?;
        let minutes = input["minutes"]
            .as_u64()
            .filter(|n| (1..=59).contains(n))
            .ok_or("intervallo da 1 a 59 minuti")?;
        let kind = input["kind"]
            .as_str()
            .filter(|kind| matches!(*kind, "audit" | "reminder"))
            .ok_or("tipo di routine non disponibile")?;
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        if self.state()? != Some(true) {
            return Err("Avvia il container n8n prima di creare la routine".into());
        }
        let mut jobs = self.jobs();
        if jobs.len() >= 20 {
            return Err("Sono ammesse al massimo 20 routine".into());
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let bytes = workflow(&id, name, minutes).to_string();
        self.paused(|| {
            self.admin("create", &id, Some(bytes.as_bytes()))?;
            jobs.push(json!({"id": id, "name": name, "kind": kind, "minutes": minutes}));
            if let Err(error) = self.save(&jobs) {
                let _ = self.admin("remove", &id, None);
                return Err(error);
            }
            Ok(())
        })?;
        Ok(self.status())
    }

    pub fn remove(&self, id: &str) -> Result<Value, String> {
        self.ensure_enabled()?;
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        let mut jobs = self.jobs();
        let index = jobs
            .iter()
            .position(|job| job["id"] == id)
            .ok_or("routine inesistente")?;
        if self.state()? != Some(true) {
            return Err("Avvia il container n8n prima di rimuovere la routine".into());
        }
        self.paused(|| {
            self.admin("remove", id, None)?;
            jobs.remove(index);
            self.save(&jobs)
        })?;
        Ok(self.status())
    }

    fn save(&self, jobs: &[Value]) -> Result<(), String> {
        let bytes = serde_json::to_vec(jobs).map_err(|e| e.to_string())?;
        crate::backup::atomic_write(&self.dir.join(JOBS), &bytes).map_err(|e| e.to_string())
    }

    pub fn completed(&self) -> Vec<Value> {
        if !self.enabled() {
            return Vec::new();
        }
        let Ok(_lock) = self.mutex.try_lock() else {
            return Vec::new();
        };
        let mut jobs = self.jobs();
        if jobs.is_empty() || self.state() != Ok(Some(true)) {
            return Vec::new();
        }
        // Legge solo l’esito delle routine del catalogo, con connessione SQLite in sola lettura.
        let script = r#"const db = new (require("node:sqlite").DatabaseSync)("/home/node/.n8n/database.sqlite", {readOnly:true});
const query = db.prepare("SELECT max(id) AS latest FROM execution_entity WHERE workflowId = ? AND status = 'success'");
console.log(JSON.stringify(process.argv.slice(1).map(id => [id, query.get(id).latest])));"#;
        let mut args = vec!["exec", &self.name, "node", "-e", script];
        args.extend(jobs.iter().filter_map(|job| job["id"].as_str()));
        let Ok(data) = docker(&args, None, Duration::from_secs(8)) else {
            return Vec::new();
        };
        let Ok(executions) = serde_json::from_str::<Vec<(String, Option<u64>)>>(&data) else {
            return Vec::new();
        };
        let mut completed = Vec::new();
        for job in &mut jobs {
            let latest = executions
                .iter()
                .find(|(id, _)| job["id"] == id.as_str())
                .and_then(|(_, exec)| *exec);
            if let Some(latest) = latest.filter(|n| *n > job["last_run"].as_u64().unwrap_or(0)) {
                job["last_run"] = json!(latest);
                completed.push(job.clone());
            }
        }
        completed
    }

    pub fn acknowledge(&self, completed: &[Value]) -> Result<(), String> {
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        let mut jobs = self.jobs();
        for job in &mut jobs {
            let latest = completed
                .iter()
                .find(|done| done["id"] == job["id"])
                .and_then(|done| done["last_run"].as_u64());
            if let Some(latest) =
                latest.filter(|n| *n > job["last_run"].as_u64().unwrap_or_default())
            {
                job["last_run"] = json!(latest);
            }
        }
        self.save(&jobs)
    }
}

fn workflow(id: &str, name: &str, minutes: u64) -> Value {
    json!([{
        "id": id, "name": name, "active": false, "settings": {"executionOrder": "v1"},
        "nodes": [
            {"id": "schedule", "name": "Pianificazione", "type": "n8n-nodes-base.scheduleTrigger",
             "typeVersion": 1.2, "position": [0, 0],
             "parameters": {"rule": {"interval": [{"field": "minutes", "minutesInterval": minutes}]}}},
            {"id": "pulse", "name": "Segnale a JARVIS", "type": "n8n-nodes-base.noOp",
             "typeVersion": 1, "position": [260, 0], "parameters": {}}
        ],
        "connections": {"Pianificazione": {"main": [[{"node": "Segnale a JARVIS", "type": "main", "index": 0}]]}}
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_workflow_from_schedule_and_fixed_nodes() {
        let doc = workflow("test", "routine", 5);
        let entry = &doc[0];
        assert_eq!(entry["active"], false);
        assert_eq!(
            entry["nodes"][0]["parameters"]["rule"]["interval"][0]["minutesInterval"],
            5
        );
        assert_eq!(entry["nodes"][1]["type"], "n8n-nodes-base.noOp");
    }
}
