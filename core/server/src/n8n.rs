//! n8n isolato e gestito da JARVIS. Docker riceve solo comandi fissi e workflow da modelli interni.
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const IMAGE: &str = "docker.n8n.io/n8nio/n8n:2.48.7";
const JOBS: &str = "n8n-jobs.json";
const JOBS_MAX_BYTES: u64 = 32 * 1024;

fn docker(args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Result<String, String> {
    docker_with(args, input, timeout, None)
}

/// Come `docker`, con una cartella di configurazione Docker alternativa (`DOCKER_CONFIG`).
fn docker_with(
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
    config: Option<&std::path::Path>,
) -> Result<String, String> {
    let mut cmd = crate::container_runtime::docker_command();
    if let Some(config) = config {
        cmd.env("DOCKER_CONFIG", config);
    }
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
            let first_error = (&mut pipe).take(128 * 1024).read_to_end(&mut data).err();
            let drain_error = std::io::copy(&mut pipe, &mut std::io::sink()).err();
            match first_error.or(drain_error) {
                Some(error) => Err(error),
                None => Ok(String::from_utf8_lossy(&data).into_owned()),
            }
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
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= until => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Docker non ha risposto entro il tempo previsto".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "controllo del processo Docker non riuscito: {error}"
                ));
            }
        }
    };
    let out = stdout
        .join()
        .map_err(|_| "lettura stdout Docker interrotta".to_string())?
        .map_err(|error| format!("lettura stdout Docker non riuscita: {error}"))?;
    let err = stderr
        .join()
        .map_err(|_| "lettura stderr Docker interrotta".to_string())?
        .map_err(|error| format!("lettura stderr Docker non riuscita: {error}"))?;
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

/// Download dell'immagine in corso o concluso (ADR-0205).
#[derive(Default)]
struct Pull {
    status: &'static str,
    detail: String,
    started: Option<Instant>,
}

/// Dimensione indicativa del download dell'immagine n8n (stima, non misurata).
pub const IMAGE_ESTIMATE_MB: u32 = 450;

pub struct Manager {
    dir: PathBuf,
    name: String,
    mutex: Mutex<()>,
    pull: Mutex<Pull>,
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
            pull: Mutex::new(Pull::default()),
        }
    }

    /// L'immagine è già sul computer?
    pub fn image_present(&self) -> bool {
        docker(&["image", "inspect", IMAGE], None, Duration::from_secs(6)).is_ok()
    }

    fn set_pull(&self, status: &'static str, detail: impl Into<String>) {
        let mut p = self.pull.lock().unwrap_or_else(|e| e.into_inner());
        p.status = status;
        p.detail = detail.into();
        p.started = (status == "pulling").then(Instant::now);
    }

    /// Scarica l'immagine in background, **solo dopo la conferma del proprietario** (la rotta è sua), e con
    /// `start_after` avvia poi il container. Uno stato `pulling`, `ready` o `failed` è visibile in `status()`.
    pub fn start_pull(self: &Arc<Self>, start_after: bool) -> Result<Value, String> {
        self.ensure_enabled()?;
        {
            let p = self.pull.lock().unwrap_or_else(|e| e.into_inner());
            if p.status == "pulling" {
                return Err("Il download dell'immagine è già in corso".into());
            }
        }
        if self.image_present() {
            if start_after {
                return self.set_running(true);
            }
            self.set_pull("ready", "Immagine già presente");
            return Ok(self.status());
        }
        self.set_pull("pulling", format!("Download di {IMAGE}"));
        let me = self.clone();
        std::thread::spawn(move || {
            me.run_pull(start_after, || {
                // Immagine pubblica: configurazione Docker privata (niente helper delle credenziali né
                // `~/.docker` non scrivibile), e causa probabile nell'errore.
                let config = crate::container_runtime::private_config_dir(&me.dir)?;
                docker_with(
                    &["pull", IMAGE],
                    None,
                    Duration::from_secs(1200),
                    Some(&config),
                )
                .map(|_| ())
                .map_err(|e| crate::container_runtime::explain_pull_error(&e))
            })
        });
        Ok(self.status())
    }

    /// Corpo del download, con l'azione di scaricamento iniettabile (prove senza Docker).
    fn run_pull(&self, start_after: bool, puller: impl FnOnce() -> Result<(), String>) {
        match puller() {
            Err(error) => self.set_pull("failed", error),
            Ok(()) if start_after => match self.set_running(true) {
                Ok(_) => self.set_pull("ready", "Immagine scaricata e n8n avviato"),
                Err(error) => self.set_pull(
                    "failed",
                    format!("Immagine scaricata, ma l'avvio non è riuscito: {error}"),
                ),
            },
            Ok(()) => self.set_pull("ready", "Immagine scaricata"),
        }
    }

    fn pull_view(&self) -> Value {
        let p = self.pull.lock().unwrap_or_else(|e| e.into_inner());
        json!({"status": if p.status.is_empty() { "idle" } else { p.status }, "detail": p.detail,
            "elapsed_s": p.started.map(|s| s.elapsed().as_secs())})
    }

    fn jobs(&self) -> Result<Vec<Value>, String> {
        read_jobs(&self.dir.join(JOBS))
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
        let (jobs, jobs_error) = match self.jobs() {
            Ok(jobs) => (jobs, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let mut status = if !self.enabled() {
            json!({"enabled": false, "running": false, "items": jobs, "image": IMAGE})
        } else {
            match self.state() {
                Ok(state) => {
                    json!({"enabled": true, "running": state == Some(true), "docker_connected": true, "container_created": state.is_some(), "items": jobs, "image": IMAGE})
                }
                Err(error) => {
                    // Perché Docker non risponde, con la correzione più probabile (ADR-0113).
                    let (health, _) = crate::container_runtime::diagnose();
                    json!({"enabled": true, "running": false, "docker_connected": false, "items": jobs, "error": error, "image": IMAGE,
                           "docker": {"state": health.code(), "hint": health.hint()}})
                }
            }
        };
        if let Some(error) = jobs_error {
            status["jobs_error"] = json!(error);
        }
        if status["enabled"] == true && status["docker_connected"] != false {
            status["image_present"] = json!(self.image_present());
        }
        status["image_estimate_mb"] = json!(IMAGE_ESTIMATE_MB);
        status["pull"] = self.pull_view();
        status
    }

    pub fn set_running(&self, running: bool) -> Result<Value, String> {
        self.ensure_enabled()?;
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        match (running, self.state()?) {
            (true, None) if !self.image_present() => {
                return Err(format!(
                    "image_missing: l'immagine di n8n non è sul computer (circa {IMAGE_ESTIMATE_MB} MB). Conferma il download per proseguire."
                ));
            }
            (true, None) => {
                let volume = format!("{}:/home/node/.n8n", self.name);
                docker(&["run", "-d", "--init", "--read-only", "--user", "1000:1000",
                    "--tmpfs", "/tmp:rw,noexec,nosuid,nodev,size=64m",
                    "--tmpfs", "/home/node/.cache:rw,noexec,nosuid,nodev,size=64m,uid=1000,gid=1000,mode=0700",
                    "--log-driver", "local", "--log-opt", "max-size=10m", "--log-opt", "max-file=3",
                    "--name", &self.name, "--label", "local.jarvis.n8n=1",
                    "--network", "none", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                    "--pids-limit", "256", "--memory", "768m", "--cpus", "1.5", "--restart", "unless-stopped", "--volume", &volume,
                    "-e", "N8N_LISTEN_ADDRESS=127.0.0.1",
                    "-e", "N8N_DIAGNOSTICS_ENABLED=false", "-e", "N8N_VERSION_NOTIFICATIONS_ENABLED=false",
                    "-e", "N8N_TEMPLATES_ENABLED=false", "-e", "N8N_COMMUNITY_PACKAGES_ENABLED=false",
                    "-e", "N8N_BLOCK_ENV_ACCESS_IN_NODE=true", "-e", "N8N_RUNNERS_ENABLED=false",
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
                "--init",
                "--read-only",
                "--user",
                "1000:1000",
                "--tmpfs",
                "/tmp:rw,noexec,nosuid,nodev,size=64m",
                "--tmpfs",
                "/home/node/.cache:rw,noexec,nosuid,nodev,size=64m,uid=1000,gid=1000,mode=0700",
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
                "-e",
                "N8N_RUNNERS_ENABLED=false",
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
        let mut jobs = self.jobs()?;
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
        let mut jobs = self.jobs()?;
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

    pub fn completed(&self) -> Result<Vec<Value>, String> {
        if !self.enabled() {
            return Ok(Vec::new());
        }
        let _lock = match self.mutex.try_lock() {
            Ok(lock) => lock,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(Vec::new()),
            Err(std::sync::TryLockError::Poisoned(error)) => {
                return Err(format!("blocco n8n non disponibile: {error}"))
            }
        };
        let mut jobs = self.jobs()?;
        if jobs.is_empty() || self.state()? != Some(true) {
            return Ok(Vec::new());
        }
        // Legge solo l’esito delle routine del catalogo, con connessione SQLite in sola lettura.
        let script = r#"const db = new (require("node:sqlite").DatabaseSync)("/home/node/.n8n/database.sqlite", {readOnly:true});
const query = db.prepare("SELECT max(id) AS latest FROM execution_entity WHERE workflowId = ? AND status = 'success'");
console.log(JSON.stringify(process.argv.slice(1).map(id => [id, query.get(id).latest])));"#;
        let mut args = vec!["exec", &self.name, "node", "-e", script];
        args.extend(jobs.iter().filter_map(|job| job["id"].as_str()));
        let data = docker(&args, None, Duration::from_secs(8))
            .map_err(|error| format!("lettura delle esecuzioni n8n: {error}"))?;
        let executions = serde_json::from_str::<Vec<(String, Option<u64>)>>(&data)
            .map_err(|error| format!("risposta delle esecuzioni n8n non valida: {error}"))?;
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
        Ok(completed)
    }

    pub fn acknowledge(&self, completed: &[Value]) -> Result<(), String> {
        let _lock = self.mutex.lock().map_err(|e| e.to_string())?;
        let mut jobs = self.jobs()?;
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

fn read_jobs(path: &std::path::Path) -> Result<Vec<Value>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("lettura delle routine n8n non riuscita: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(JOBS_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("lettura delle routine n8n non riuscita: {error}"))?;
    if bytes.len() as u64 > JOBS_MAX_BYTES {
        return Err("archivio delle routine n8n troppo grande".into());
    }
    let jobs: Vec<Value> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("archivio delle routine n8n non valido: {error}"))?;
    for (index, job) in jobs.iter().enumerate() {
        let valid_id = job["id"]
            .as_str()
            .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let valid_name = job["name"].as_str().is_some_and(|name| {
            !name.trim().is_empty()
                && name.chars().count() <= 80
                && !name.chars().any(char::is_control)
        });
        let valid_kind = matches!(job["kind"].as_str(), Some("audit" | "reminder"));
        let valid_minutes = job["minutes"]
            .as_u64()
            .is_some_and(|minutes| (1..=59).contains(&minutes));
        let valid_last_run = job["last_run"].is_null() || job["last_run"].as_u64().is_some();
        if !valid_id || !valid_name || !valid_kind || !valid_minutes || !valid_last_run {
            return Err(format!(
                "archivio delle routine n8n non valido: elemento {}",
                index + 1
            ));
        }
    }
    Ok(jobs)
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

    #[test]
    fn missing_jobs_file_means_no_jobs() {
        let path = std::env::temp_dir().join(format!(
            "jarvis-n8n-missing-{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        assert!(read_jobs(&path).unwrap().is_empty());
    }

    #[test]
    fn corrupt_jobs_file_is_reported_instead_of_reset() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-n8n-jobs-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(JOBS);
        std::fs::write(&path, b"{").unwrap();
        assert!(read_jobs(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persisted_jobs_are_validated_before_use() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-n8n-jobs-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(JOBS);
        std::fs::write(
            &path,
            br#"[{"id":"invalid","name":"routine","kind":"reminder","minutes":5}]"#,
        )
        .unwrap();
        assert!(read_jobs(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn status_surfaces_a_corrupt_jobs_file() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-n8n-status-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(JOBS), b"{").unwrap();
        let manager = Manager::new(dir.clone());
        let status = manager.status();
        assert_eq!(status["items"], json!([]));
        assert!(status["jobs_error"].as_str().is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persisted_jobs_accept_the_current_saved_shape() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-n8n-valid-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(JOBS);
        std::fs::write(
            &path,
            br#"[{"id":"0123456789abcdef0123456789abcdef","name":"routine","kind":"reminder","minutes":5,"last_run":12}]"#,
        )
        .unwrap();
        let jobs = read_jobs(&path).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["last_run"], 12);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_jobs_file_is_rejected_without_full_read() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-n8n-large-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(JOBS);
        std::fs::write(&path, vec![b' '; (JOBS_MAX_BYTES + 1) as usize]).unwrap();
        assert!(read_jobs(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "richiede Docker locale e l'immagine docker.n8n.io/n8nio/n8n:2.48.7"]
    fn hardened_container_can_import_and_remove_a_routine() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-n8n-integration-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("n8n-enabled"), b"1").unwrap();
        let manager = Manager::new(dir.clone());
        let cleanup_name = manager.name.clone();

        let result = (|| {
            let status = manager.set_running(true)?;
            assert_eq!(status["running"], true);
            let status = manager.add(&json!({
                "name": "routine di test",
                "minutes": 1,
                "kind": "reminder"
            }))?;
            let id = status["items"][0]["id"].as_str().unwrap();
            manager.remove(id)?;
            let status = manager.set_running(false)?;
            assert_eq!(status["running"], false);
            Ok::<(), String>(())
        })();

        if let Err(error) = &result {
            let logs = docker(
                &["logs", "--tail", "40", &cleanup_name],
                None,
                Duration::from_secs(8),
            )
            .unwrap_or_else(|log_error| log_error);
            eprintln!("[n8n test] {error}; log container: {logs}");
        }
        let _ = docker(
            &["rm", "-f", "-v", &cleanup_name],
            None,
            Duration::from_secs(15),
        );
        let _ = docker(
            &["volume", "rm", &cleanup_name],
            None,
            Duration::from_secs(15),
        );
        std::fs::remove_dir_all(dir).unwrap();
        result.unwrap();
    }

    #[test]
    fn the_image_download_reports_its_state_and_can_start_n8n_afterwards() {
        let dir = std::env::current_dir().unwrap().join(format!(
            "target/jarvis-n8n-pull-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = Manager::new(dir.clone());
        assert_eq!(manager.pull_view()["status"], "idle");
        manager.set_pull("pulling", "Download");
        assert_eq!(manager.pull_view()["status"], "pulling");
        manager.run_pull(false, || Ok(()));
        assert_eq!(manager.pull_view()["status"], "ready");
        manager.run_pull(false, || Err("rete assente".into()));
        let failed = manager.pull_view();
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["detail"], "rete assente");
        // Con l'avvio richiesto ma n8n non abilitato l'avvio fallisce e lo stato lo dice.
        manager.run_pull(true, || Ok(()));
        assert_eq!(manager.pull_view()["status"], "failed");
        assert!(manager.pull_view()["detail"]
            .as_str()
            .unwrap()
            .contains("avvio non è riuscito"));
        assert!(
            Arc::new(Manager::new(dir.clone()))
                .start_pull(false)
                .is_err(),
            "n8n va prima abilitato"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
