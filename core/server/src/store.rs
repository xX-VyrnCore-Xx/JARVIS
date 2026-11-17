//! Persistenza locale SQLite: memoria del proprietario, task e conversazione (MEMORY_DATA_SYNC.md).

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub struct Store {
    conn: Connection,
}

pub type StoreResult<T> = Result<T, rusqlite::Error>;

impl Store {
    pub fn open(path: &Path) -> StoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS memory (id TEXT PRIMARY KEY, text TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS tasks (task_id TEXT PRIMARY KEY, doc TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS messages (seq INTEGER PRIMARY KEY AUTOINCREMENT, role TEXT NOT NULL,
                 content TEXT NOT NULL, task_id TEXT, at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS workflows (workflow_id TEXT PRIMARY KEY, doc TEXT NOT NULL, next_run_at TEXT,
                 created_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS workflow_runs (run_id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL,
                 started_at TEXT NOT NULL, doc TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS workflow_runs_by_workflow ON workflow_runs (workflow_id, started_at);
             CREATE TABLE IF NOT EXISTS workflow_versions (workflow_id TEXT NOT NULL, version INTEGER NOT NULL,
                 saved_at TEXT NOT NULL, doc TEXT NOT NULL, PRIMARY KEY (workflow_id, version));
             CREATE TABLE IF NOT EXISTS notifications (notification_id TEXT PRIMARY KEY, created_at TEXT NOT NULL,
                 dedupe_key TEXT, read_at TEXT, doc TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS notifications_unread ON notifications (read_at, dedupe_key);
             CREATE TABLE IF NOT EXISTS connectors (connector_id TEXT PRIMARY KEY, manifest TEXT NOT NULL,
                 installed_at TEXT NOT NULL, authorized_at TEXT);
             CREATE TABLE IF NOT EXISTS metrics_snapshots (at TEXT PRIMARY KEY, doc TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS devices (device_id TEXT PRIMARY KEY, token_sha256 TEXT NOT NULL UNIQUE,
                 doc TEXT NOT NULL);",
        )?;
        // Migrazione additiva: database creati prima delle fonti Web (RF-06).
        let has_sources = conn.prepare("SELECT sources FROM messages LIMIT 0").is_ok();
        if !has_sources {
            conn.execute_batch("ALTER TABLE messages ADD COLUMN sources TEXT;")?;
        }
        Ok(Self { conn })
    }

    pub fn memory_add(&self, id: &str, text: &str, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO memory (id, text, created_at) VALUES (?1, ?2, ?3)",
            params![id, text, at],
        )?;
        Ok(())
    }

    pub fn memory_list(&self) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, text, created_at FROM memory ORDER BY created_at, id")?;
        let rows = stmt.query_map([], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "text": r.get::<_, String>(1)?, "created_at": r.get::<_, String>(2)?}))
        })?;
        rows.collect()
    }

    pub fn memory_get(&self, id: &str) -> StoreResult<Option<String>> {
        self.conn
            .query_row("SELECT text FROM memory WHERE id = ?1", [id], |r| r.get(0))
            .optional()
    }

    pub fn memory_delete(&self, id: &str) -> StoreResult<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM memory WHERE id = ?1", [id])?
            > 0)
    }

    pub fn task_put(&self, doc: &Value, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO tasks (task_id, doc, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET doc = excluded.doc, updated_at = excluded.updated_at",
            params![doc["task_id"].as_str(), doc.to_string(), at],
        )?;
        Ok(())
    }

    pub fn tasks_recent(&self, limit: u32) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc FROM tasks ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)))
            .collect()
    }

    pub fn message_add(
        &self,
        role: &str,
        content: &str,
        task_id: &str,
        at: &str,
        sources: Option<&Value>,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO messages (role, content, task_id, at, sources) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![role, content, task_id, at, sources.map(|s| s.to_string())],
        )?;
        Ok(())
    }

    /// Ultimi `limit` messaggi in ordine cronologico.
    pub fn messages_recent(&self, limit: u32) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, task_id, at, sources FROM (SELECT * FROM messages ORDER BY seq DESC LIMIT ?1) ORDER BY seq",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(
                json!({"role": r.get::<_, String>(0)?, "content": r.get::<_, String>(1)?,
                      "task_id": r.get::<_, Option<String>>(2)?, "at": r.get::<_, String>(3)?,
                      "sources": r.get::<_, Option<String>>(4)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Array(vec![]))}),
            )
        })?;
        rows.collect()
    }

    // ------------------------------------------------------------------ workflow

    pub fn workflow_put(&self, doc: &Value, next_run_at: Option<&str>) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO workflows (workflow_id, doc, next_run_at, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workflow_id) DO UPDATE SET doc = excluded.doc, next_run_at = excluded.next_run_at",
            params![doc["workflow_id"].as_str(), doc.to_string(), next_run_at, doc["created_at"].as_str()],
        )?;
        Ok(())
    }

    /// Workflow con `next_run_at`. `key` può essere l'id completo o un prefisso univoco (almeno 4 caratteri).
    pub fn workflow_find(&self, key: &str) -> StoreResult<Vec<(Value, Option<String>)>> {
        if key.len() < 4 || !key.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT doc, next_run_at FROM workflows WHERE workflow_id LIKE ?1 || '%'")?;
        let rows = stmt.query_map([key.to_ascii_lowercase()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|r| r.map(|(d, n)| (serde_json::from_str(&d).unwrap_or(Value::Null), n)))
            .collect()
    }

    pub fn workflows(&self) -> StoreResult<Vec<(Value, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc, next_run_at FROM workflows ORDER BY created_at")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|r| r.map(|(d, n)| (serde_json::from_str(&d).unwrap_or(Value::Null), n)))
            .collect()
    }

    pub fn workflows_due(&self, now: &str) -> StoreResult<Vec<(Value, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT doc, next_run_at FROM workflows WHERE next_run_at IS NOT NULL AND next_run_at <= ?1 ORDER BY next_run_at",
        )?;
        let rows = stmt.query_map([now], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| r.map(|(d, n)| (serde_json::from_str(&d).unwrap_or(Value::Null), n)))
            .collect()
    }

    /// Conserva ogni versione approvata (la cronologia resta anche dopo l'eliminazione del workflow).
    pub fn workflow_version_put(&self, doc: &Value) -> StoreResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO workflow_versions (workflow_id, version, saved_at, doc) VALUES (?1, ?2, ?3, ?4)",
            params![doc["workflow_id"].as_str(), doc["version"].as_i64(), crate::now(), doc.to_string()],
        )?;
        Ok(())
    }

    pub fn workflow_versions(&self, id: &str) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT doc FROM workflow_versions WHERE workflow_id = ?1 ORDER BY version DESC",
        )?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)))
            .collect()
    }

    pub fn workflow_delete(&self, id: &str) -> StoreResult<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM workflows WHERE workflow_id = ?1", [id])?
            > 0)
    }

    pub fn run_put(&self, doc: &Value) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO workflow_runs (run_id, workflow_id, started_at, doc) VALUES (?1, ?2, ?3, ?4)",
            params![doc["run_id"].as_str(), doc["workflow_id"].as_str(), doc["started_at"].as_str(), doc.to_string()],
        )?;
        Ok(())
    }

    pub fn runs(&self, workflow_id: &str, limit: u32) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare("SELECT doc FROM workflow_runs WHERE workflow_id = ?1 ORDER BY started_at DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![workflow_id, limit], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)))
            .collect()
    }

    // ------------------------------------------------------------------ notifiche

    pub fn notification_unread_exists(&self, dedupe_key: &str) -> StoreResult<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM notifications WHERE dedupe_key = ?1 AND read_at IS NULL LIMIT 1",
                [dedupe_key],
                |_| Ok(()),
            )
            .optional()
            .map(|r| r.is_some())
    }

    pub fn notification_put(&self, doc: &Value, keep: u32) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO notifications (notification_id, created_at, dedupe_key, read_at, doc) VALUES (?1, ?2, ?3, NULL, ?4)",
            params![doc["notification_id"].as_str(), doc["created_at"].as_str(), doc["dedupe_key"].as_str(), doc.to_string()],
        )?;
        // Conservazione: restano le `keep` notifiche più recenti.
        self.conn.execute(
            "DELETE FROM notifications WHERE notification_id NOT IN (SELECT notification_id FROM notifications ORDER BY created_at DESC LIMIT ?1)",
            [keep],
        )?;
        Ok(())
    }

    fn with_read(doc: String, read_at: Option<String>) -> Value {
        let mut v: Value = serde_json::from_str(&doc).unwrap_or(Value::Null);
        if let Some(r) = read_at {
            v["read_at"] = r.into();
        }
        v
    }

    pub fn notifications(
        &self,
        limit: u32,
        unread_only: bool,
        since: Option<&str>,
    ) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT doc, read_at FROM notifications WHERE (?1 = 0 OR read_at IS NULL) AND (?2 IS NULL OR created_at > ?2)
             ORDER BY created_at DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![unread_only as i32, since, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|r| r.map(|(d, read)| Self::with_read(d, read)))
            .collect()
    }

    pub fn notifications_unread(&self) -> StoreResult<u32> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM notifications WHERE read_at IS NULL",
            [],
            |r| r.get(0),
        )
    }

    pub fn notification_read(&self, id: &str, at: &str) -> StoreResult<bool> {
        Ok(self.conn.execute(
            "UPDATE notifications SET read_at = ?2 WHERE notification_id = ?1 AND read_at IS NULL",
            params![id, at],
        )? > 0)
    }

    pub fn notification_read_by_key(&self, dedupe_key: &str, at: &str) -> StoreResult<usize> {
        self.conn.execute(
            "UPDATE notifications SET read_at = ?2 WHERE dedupe_key = ?1 AND read_at IS NULL",
            params![dedupe_key, at],
        )
    }

    pub fn notifications_read_all(&self, at: &str) -> StoreResult<usize> {
        self.conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL",
            [at],
        )
    }

    /// Istantanea coerente del database in un nuovo file (per i backup).
    pub fn connector_put(&self, manifest: &Value, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO connectors (connector_id, manifest, installed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(connector_id) DO UPDATE SET manifest = excluded.manifest",
            params![manifest["id"].as_str(), manifest.to_string(), at],
        )?;
        Ok(())
    }

    pub fn connector_set_authorized(&self, id: &str, at: Option<&str>) -> StoreResult<bool> {
        Ok(self.conn.execute(
            "UPDATE connectors SET authorized_at = ?2 WHERE connector_id = ?1",
            params![id, at],
        )? > 0)
    }

    /// (manifest, installato il, autorizzato il)
    pub fn connectors(&self) -> StoreResult<Vec<(Value, String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT manifest, installed_at, authorized_at FROM connectors ORDER BY connector_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                serde_json::from_str(&r.get::<_, String>(0)?).unwrap_or(Value::Null),
                r.get(1)?,
                r.get(2)?,
            ))
        })?;
        rows.collect()
    }

    pub fn connector_delete(&self, id: &str) -> StoreResult<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM connectors WHERE connector_id = ?1", [id])?
            > 0)
    }

    /// Salva un'istantanea delle metriche e cancella quelle più vecchie di `keep_since`.
    pub fn metrics_put(&self, at: &str, doc: &Value, keep_since: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO metrics_snapshots (at, doc) VALUES (?1, ?2)",
            params![at, doc.to_string()],
        )?;
        self.conn
            .execute("DELETE FROM metrics_snapshots WHERE at < ?1", [keep_since])?;
        Ok(())
    }

    pub fn metrics_since(&self, since: &str) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc FROM metrics_snapshots WHERE at >= ?1 ORDER BY at LIMIT 5000")?;
        let rows = stmt.query_map([since], |r| {
            Ok(serde_json::from_str(&r.get::<_, String>(0)?).unwrap_or(Value::Null))
        })?;
        rows.collect()
    }

    pub fn device_put(&self, doc: &Value, token_sha256: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO devices (device_id, token_sha256, doc) VALUES (?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET doc = excluded.doc",
            params![doc["device_id"].as_str(), token_sha256, doc.to_string()],
        )?;
        Ok(())
    }

    pub fn device_by_token(&self, token_sha256: &str) -> StoreResult<Option<Value>> {
        self.conn
            .query_row(
                "SELECT doc FROM devices WHERE token_sha256 = ?1",
                [token_sha256],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map(|d| d.and_then(|d| serde_json::from_str(&d).ok()))
    }

    pub fn device_retire_token(&self, id: &str) -> StoreResult<()> {
        self.conn.execute(
            "UPDATE devices SET token_sha256 = 'revocato:' || device_id WHERE device_id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn devices(&self) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc FROM devices ORDER BY device_id")?;
        let rows = stmt.query_map([], |r| {
            Ok(serde_json::from_str(&r.get::<_, String>(0)?).unwrap_or(Value::Null))
        })?;
        rows.collect()
    }

    pub fn snapshot_to(&self, path: &Path) -> StoreResult<()> {
        self.conn
            .execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        Ok(())
    }
}
