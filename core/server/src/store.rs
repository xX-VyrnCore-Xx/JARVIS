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
             CREATE INDEX IF NOT EXISTS workflow_runs_by_workflow ON workflow_runs (workflow_id, started_at);",
        )?;
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
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO messages (role, content, task_id, at) VALUES (?1, ?2, ?3, ?4)",
            params![role, content, task_id, at],
        )?;
        Ok(())
    }

    /// Ultimi `limit` messaggi in ordine cronologico.
    pub fn messages_recent(&self, limit: u32) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, task_id, at FROM (SELECT * FROM messages ORDER BY seq DESC LIMIT ?1) ORDER BY seq",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(
                json!({"role": r.get::<_, String>(0)?, "content": r.get::<_, String>(1)?,
                      "task_id": r.get::<_, Option<String>>(2)?, "at": r.get::<_, String>(3)?}),
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
}
