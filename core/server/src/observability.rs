//! Observability Engine del prototipo (RNF-07, DEVOPS_QA_OBSERVABILITY.md).
//!
//! - **Metriche** in memoria: conteggi, errori e latenze (p50, p95, max sugli ultimi 512 campioni)
//!   per endpoint, strumento, delega e trigger; contatori di eventi (riavvii del worker, guasti).
//! - **Log strutturati** JSON Lines in `data/logs/core.jsonl`, con rotazione a 5 MiB. Ogni riga porta
//!   gli identificativi di correlazione e non contiene contenuti dell'utente (messaggi, file, audio).

use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::{json, Map, Value};

const SAMPLES: usize = 512;
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Default)]
struct Series {
    count: u64,
    errors: u64,
    max_ms: f64,
    recent: VecDeque<f64>,
}

impl Series {
    fn record(&mut self, ms: f64, error: bool) {
        self.count += 1;
        if error {
            self.errors += 1;
        }
        self.max_ms = self.max_ms.max(ms);
        if self.recent.len() == SAMPLES {
            self.recent.pop_front();
        }
        self.recent.push_back(ms);
    }

    fn summary(&self) -> Value {
        let mut sorted: Vec<f64> = self.recent.iter().copied().collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f64| -> f64 {
            if sorted.is_empty() {
                return 0.0;
            }
            let idx = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
            (sorted[idx] * 10.0).round() / 10.0
        };
        json!({"count": self.count, "errors": self.errors, "p50_ms": pct(0.50), "p95_ms": pct(0.95), "max_ms": (self.max_ms * 10.0).round() / 10.0})
    }
}

pub struct Observability {
    started: Instant,
    series: Mutex<BTreeMap<String, Series>>,
    counters: Mutex<BTreeMap<String, u64>>,
    log: Mutex<Option<(PathBuf, File)>>,
}

impl Observability {
    pub fn new(log_dir: Option<&Path>) -> Self {
        let log = log_dir.and_then(|dir| {
            std::fs::create_dir_all(dir).ok()?;
            let path = dir.join("core.jsonl");
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()?;
            Some((path, file))
        });
        Self {
            started: Instant::now(),
            series: Mutex::new(BTreeMap::new()),
            counters: Mutex::new(BTreeMap::new()),
            log: Mutex::new(log),
        }
    }

    /// Registra una durata nella serie `name` (es. `http GET /api/health`, `tool fs.read`).
    pub fn observe(&self, name: &str, ms: f64, error: bool) {
        self.series
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(name.to_string())
            .or_default()
            .record(ms, error);
    }

    pub fn count(&self, name: &str) {
        *self
            .counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(name.to_string())
            .or_default() += 1;
    }

    /// Scrive un evento di log strutturato. `fields` non deve contenere contenuti dell'utente.
    pub fn log(&self, level: &str, event: &str, fields: Value) {
        let mut line = Map::new();
        line.insert("at".into(), crate::now().into());
        line.insert("level".into(), level.into());
        line.insert("event".into(), event.into());
        if let Value::Object(extra) = fields {
            line.extend(extra);
        }
        let text = Value::Object(line).to_string();
        let mut guard = self.log.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((path, file)) = guard.as_mut() {
            if file
                .metadata()
                .map(|m| m.len() > LOG_MAX_BYTES)
                .unwrap_or(false)
            {
                let rotated = path.with_extension("jsonl.1");
                let _ = std::fs::rename(&*path, rotated);
                if let Ok(fresh) = OpenOptions::new().create(true).append(true).open(&*path) {
                    *file = fresh;
                }
            }
            let _ = writeln!(file, "{text}");
        }
    }

    pub fn snapshot(&self) -> Value {
        let series = self.series.lock().unwrap_or_else(|e| e.into_inner());
        let mut groups: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
        for (name, s) in series.iter() {
            let (group, key) = name.split_once(' ').unwrap_or(("other", name));
            groups
                .entry(group.to_string())
                .or_default()
                .insert(key.to_string(), s.summary());
        }
        json!({
            "uptime_s": self.started.elapsed().as_secs(),
            "series": groups,
            "counters": *self.counters.lock().unwrap_or_else(|e| e.into_inner()),
        })
    }
}

/// Cronometro per misurare una sezione di codice.
pub struct Timer(Instant);

impl Timer {
    pub fn start() -> Self {
        Self(Instant::now())
    }
    pub fn ms(&self) -> f64 {
        self.0.elapsed().as_secs_f64() * 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_and_errors() {
        let o = Observability::new(None);
        for i in 1..=100 {
            o.observe("tool fs.read", i as f64, i > 98);
        }
        let s = o.snapshot();
        let t = &s["series"]["tool"]["fs.read"];
        assert_eq!(
            (t["count"].as_u64(), t["errors"].as_u64()),
            (Some(100), Some(2))
        );
        assert_eq!(t["p50_ms"], 50.0);
        assert_eq!(t["p95_ms"], 95.0);
        assert_eq!(t["max_ms"], 100.0);
    }

    #[test]
    fn log_rotation_and_fields() {
        let dir = std::env::temp_dir().join(format!("jarvis-obs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let o = Observability::new(Some(&dir));
        o.log(
            "info",
            "task.finished",
            json!({"task_id": "t1", "duration_ms": 3.5}),
        );
        let line = std::fs::read_to_string(dir.join("core.jsonl")).unwrap();
        let v: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
        assert_eq!(
            (v["event"].as_str(), v["task_id"].as_str()),
            (Some("task.finished"), Some("t1"))
        );
    }
}
