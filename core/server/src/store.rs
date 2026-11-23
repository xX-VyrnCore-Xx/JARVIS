//! Persistenza locale SQLite: memoria del proprietario, task e conversazione (MEMORY_DATA_SYNC.md).

use std::path::Path;
use std::sync::Arc;

use crate::chat_crypto::ChatCipher;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub struct Store {
    pub(crate) conn: Connection,
    chat_cipher: Option<Arc<ChatCipher>>,
}

pub type StoreResult<T> = Result<T, rusqlite::Error>;

/// Versione dello schema (ADR-0037): 1 = schema fino a 1.3.x con migrazioni additive; 3 = tracce dei task (ADR-0039); 4 = passkey (ADR-0042); 5 = note e cose da fare (ADR-0046).
pub const SCHEMA_VERSION: i64 = 8;

pub(crate) struct ConversationTurn {
    pub user: String,
    pub assistant: Option<String>,
    pub cloud_provider: Option<String>,
}

fn crypto_error(message: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(message)))
}

fn message_purpose(role: &str, task: Option<&str>, at: &str) -> String {
    format!("message:{role}:{}:{at}", task.unwrap_or_default())
}

impl Store {
    pub fn open(path: &Path) -> StoreResult<Self> {
        let conn = Connection::open(path)?;
        // WAL con sincronizzazione NORMAL (durevole ai crash dell'app, ADR-0037), attesa sui blocchi invece di
        // errori «database is locked» quando il nodo o il backup leggono in parallelo, temporanei in memoria.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;
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
                 doc TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS notify_channels (channel_id TEXT PRIMARY KEY, doc TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cloud_feedback (task_id TEXT PRIMARY KEY, provider TEXT NOT NULL,
                 model TEXT NOT NULL, rating TEXT, created_at TEXT NOT NULL, rated_at TEXT);
             CREATE INDEX IF NOT EXISTS cloud_feedback_by_model ON cloud_feedback (provider, model, rated_at);
             CREATE INDEX IF NOT EXISTS tasks_by_updated ON tasks (updated_at);
             CREATE INDEX IF NOT EXISTS messages_by_task ON messages (task_id);
             CREATE INDEX IF NOT EXISTS workflows_due ON workflows (next_run_at);
             CREATE TABLE IF NOT EXISTS task_spans (seq INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL,
                 at TEXT NOT NULL, span TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS task_spans_by_task ON task_spans (task_id, seq);
             CREATE TABLE IF NOT EXISTS passkeys (credential_id TEXT PRIMARY KEY, device_id TEXT NOT NULL,
                 public_key BLOB NOT NULL, sign_count INTEGER NOT NULL, created_at TEXT NOT NULL, last_used_at TEXT);
             CREATE INDEX IF NOT EXISTS passkeys_by_device ON passkeys (device_id);
             CREATE TABLE IF NOT EXISTS notes (id TEXT PRIMARY KEY, title TEXT NOT NULL, text TEXT NOT NULL,
                 tags TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS todos (id TEXT PRIMARY KEY, text TEXT NOT NULL, due_at TEXT, priority TEXT NOT NULL,
                 created_at TEXT NOT NULL, done_at TEXT, notified_at TEXT);
             CREATE INDEX IF NOT EXISTS todos_open ON todos (done_at, due_at);",
        )?;
        // Migrazione additiva: database creati prima delle fonti Web (RF-06).
        let has_sources = conn.prepare("SELECT sources FROM messages LIMIT 0").is_ok();
        if !has_sources {
            conn.execute_batch("ALTER TABLE messages ADD COLUMN sources TEXT;")?;
        }
        if conn
            .prepare("SELECT cloud_provider FROM messages LIMIT 0")
            .is_err()
        {
            conn.execute_batch("ALTER TABLE messages ADD COLUMN cloud_provider TEXT;")?;
        }
        let store = Self {
            conn,
            chat_cipher: None,
        };
        store.memory_migrate()?;
        crate::skills::migrate(&store)?;
        store
            .conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        // Statistiche del pianificatore di query aggiornate solo dove servono (raccomandazione di SQLite).
        store.conn.execute_batch("PRAGMA optimize=0x10002;")?;
        Ok(store)
    }

    pub(crate) fn protect_chat(&mut self, cipher: Arc<ChatCipher>) -> StoreResult<()> {
        self.conn.execute_batch(
            "PRAGMA secure_delete=ON;
             CREATE TABLE IF NOT EXISTS chat_crypto_migration (stage INTEGER NOT NULL);",
        )?;
        let stage = self
            .conn
            .query_row("SELECT stage FROM chat_crypto_migration", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?
            .unwrap_or(0);
        if stage == 0 {
            let tx = self.conn.transaction()?;
            {
                let mut select = tx.prepare("SELECT rowid, task_id, doc FROM tasks WHERE rowid > ?1 ORDER BY rowid LIMIT 256")?;
                let mut last = 0;
                loop {
                    let rows = select
                        .query_map([last], |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    if rows.is_empty() {
                        break;
                    }
                    for (row, id, text) in rows {
                        let sealed = cipher
                            .seal(&format!("task:{id}"), &text)
                            .map_err(crypto_error)?;
                        tx.execute(
                            "UPDATE tasks SET doc=?1 WHERE rowid=?2",
                            params![sealed, row],
                        )?;
                        last = row;
                    }
                }
            }
            {
                let mut select = tx.prepare(
                    "SELECT seq, role, task_id, at, content, sources FROM messages
                    WHERE seq > ?1 ORDER BY seq LIMIT 256",
                )?;
                let mut last = 0;
                loop {
                    let rows = select
                        .query_map([last], |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, Option<String>>(5)?,
                            ))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    if rows.is_empty() {
                        break;
                    }
                    for (seq, role, task, at, content, sources) in rows {
                        let purpose = message_purpose(&role, task.as_deref(), &at);
                        let seal = |name: &str, text: &str| {
                            cipher
                                .seal(&format!("{purpose}:{name}"), text)
                                .map_err(crypto_error)
                        };
                        let content = seal("content", &content)?;
                        let sources = sources
                            .as_deref()
                            .map(|text| seal("sources", text))
                            .transpose()?;
                        tx.execute(
                            "UPDATE messages SET content=?1, sources=?2 WHERE seq=?3",
                            params![content, sources, seq],
                        )?;
                        last = seq;
                    }
                }
            }
            tx.execute("INSERT INTO chat_crypto_migration (stage) VALUES (1)", [])?;
            tx.commit()?;
        }
        if stage < 2 {
            self.conn.execute_batch("VACUUM;")?;
            let busy = self
                .conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    row.get::<_, i64>(0)
                })?;
            if busy != 0 {
                return Err(crypto_error(
                    "Impossibile ripulire il WAL della conversazione".into(),
                ));
            }
            self.conn
                .execute("UPDATE chat_crypto_migration SET stage=2", [])?;
        }
        self.chat_cipher = Some(cipher);
        Ok(())
    }

    fn seal_chat(&self, purpose: &str, value: &str) -> StoreResult<String> {
        self.chat_cipher.as_ref().map_or_else(
            || Ok(value.to_string()),
            |cipher| cipher.seal(purpose, value).map_err(crypto_error),
        )
    }

    fn open_chat(&self, purpose: &str, value: String) -> StoreResult<String> {
        match &self.chat_cipher {
            Some(cipher) => cipher.open(purpose, &value).map_err(crypto_error),
            None => Ok(value),
        }
    }

    /// Registra una passkey (chiave pubblica non compressa) per un dispositivo (ADR-0042).
    pub fn passkey_add(
        &self,
        credential_id: &str,
        device_id: &str,
        public_key: &[u8],
        sign_count: u32,
        at: &str,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO passkeys (credential_id, device_id, public_key, sign_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![credential_id, device_id, public_key, sign_count, at],
        )?;
        Ok(())
    }

    /// Passkey di un dispositivo: (id della credenziale, chiave pubblica, contatore, creata, ultimo uso).
    #[allow(clippy::type_complexity)]
    pub fn passkeys_for(
        &self,
        device_id: &str,
    ) -> StoreResult<Vec<(String, Vec<u8>, u32, String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT credential_id, public_key, sign_count, created_at, last_used_at FROM passkeys WHERE device_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map([device_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.collect()
    }

    /// Aggiorna contatore e ultimo uso dopo un'asserzione valida.
    pub fn passkey_used(&self, credential_id: &str, sign_count: u32, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "UPDATE passkeys SET sign_count = ?2, last_used_at = ?3 WHERE credential_id = ?1",
            params![credential_id, sign_count, at],
        )?;
        Ok(())
    }

    /// Elimina una passkey; vero se esisteva.
    pub fn passkey_delete(&self, credential_id: &str) -> StoreResult<bool> {
        Ok(self.conn.execute(
            "DELETE FROM passkeys WHERE credential_id = ?1",
            [credential_id],
        )? > 0)
    }

    /// Aggiunge un intervallo alla traccia di un task (ADR-0039). Le tracce più vecchie di `keep_since`
    /// vengono rimosse insieme, così la tabella non cresce senza limite.
    pub fn span_add(&self, task_id: &str, at: &str, span: &Value) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO task_spans (task_id, at, span) VALUES (?1, ?2, ?3)",
            params![task_id, at, span.to_string()],
        )?;
        Ok(())
    }

    /// Intervalli registrati per un task, in ordine.
    pub fn spans(&self, task_id: &str) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT span FROM task_spans WHERE task_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([task_id], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)))
            .collect()
    }

    /// Elimina le tracce registrate prima di `before` (conservazione).
    pub fn spans_prune(&self, before: &str) -> StoreResult<usize> {
        self.conn
            .execute("DELETE FROM task_spans WHERE at < ?1", [before])
    }

    /// Conserva le ultime `keep` esecuzioni di ogni automazione (un workflow ogni minuto ne produrrebbe
    /// oltre 500 000 l'anno) e le cose da fare completate da più di 90 giorni vengono eliminate.
    pub fn history_prune(&self, keep: u32, done_before: &str) -> StoreResult<usize> {
        let runs = self.conn.execute(
            "DELETE FROM workflow_runs WHERE run_id IN (SELECT run_id FROM (SELECT run_id, ROW_NUMBER() OVER
                 (PARTITION BY workflow_id ORDER BY started_at DESC) AS n FROM workflow_runs) WHERE n > ?1)",
            [keep],
        )?;
        let todos = self.conn.execute(
            "DELETE FROM todos WHERE done_at IS NOT NULL AND done_at < ?1",
            [done_before],
        )?;
        Ok(runs + todos)
    }

    /// Versione dello schema registrata nel file (`PRAGMA user_version`).
    pub fn schema_version(&self) -> StoreResult<i64> {
        self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))
    }

    /// Manutenzione periodica: statistiche del pianificatore e WAL riportato a dimensione zero.
    pub fn maintenance(&self) -> StoreResult<()> {
        self.conn
            .execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(TRUNCATE);")
    }

    /// Stato del database per la sezione Sistema: integrità rapida, versione dello schema e dimensioni.
    pub fn health(&self) -> StoreResult<Value> {
        let check: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let count = |table: &str| -> StoreResult<i64> {
            self.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        };
        Ok(json!({
            "ok": check == "ok", "check": check, "schema_version": self.schema_version()?,
            "size_bytes": pages * page_size,
            "rows": {"notes": count("notes")?, "todos": count("todos")?, "memory": count("memory")?, "messages": count("messages")?, "tasks": count("tasks")?, "spans": count("task_spans")?,
                     "workflows": count("workflows")?, "notifications": count("notifications")?},
        }))
    }

    pub fn channel_put(&self, doc: &Value) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO notify_channels (channel_id, doc) VALUES (?1, ?2)
             ON CONFLICT(channel_id) DO UPDATE SET doc = excluded.doc",
            params![doc["channel_id"].as_str(), doc.to_string()],
        )?;
        Ok(())
    }

    pub fn channels(&self) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc FROM notify_channels ORDER BY channel_id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows
            .filter_map(Result::ok)
            .filter_map(|d| serde_json::from_str(&d).ok())
            .collect())
    }

    pub fn channel_delete(&self, id: &str) -> StoreResult<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM notify_channels WHERE channel_id = ?1", [id])?
            > 0)
    }

    pub fn task_put(&self, doc: &Value, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO tasks (task_id, doc, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET doc = excluded.doc, updated_at = excluded.updated_at",
            params![doc["task_id"].as_str(),
                self.seal_chat(&format!("task:{}", doc["task_id"].as_str().unwrap_or_default()), &doc.to_string())?, at],
        )?;
        Ok(())
    }

    pub fn tasks_recent(&self, limit: u32) -> StoreResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT task_id, doc FROM tasks ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| {
            let id: String = r.get(0)?;
            let text = self.open_chat(&format!("task:{id}"), r.get(1)?)?;
            serde_json::from_str(&text).map_err(|e| crypto_error(e.to_string()))
        })?;
        rows.collect()
    }

    pub fn message_add(
        &self,
        role: &str,
        content: &str,
        task_id: &str,
        at: &str,
        sources: Option<&Value>,
        cloud_provider: Option<&str>,
    ) -> StoreResult<()> {
        let purpose = message_purpose(role, Some(task_id), at);
        self.conn.execute(
            "INSERT INTO messages (role, content, task_id, at, sources, cloud_provider)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                role,
                self.seal_chat(&format!("{purpose}:content"), content)?,
                task_id,
                at,
                sources
                    .map(|s| self.seal_chat(&format!("{purpose}:sources"), &s.to_string()))
                    .transpose()?,
                cloud_provider
            ],
        )?;
        Ok(())
    }

    /// Ultimi `limit` messaggi in ordine cronologico.
    pub fn messages_recent(&self, limit: u32) -> StoreResult<Vec<Value>> {
        self.conversation_recent(limit, false)
    }

    pub(crate) fn conversation_recent(&self, limit: u32, owner: bool) -> StoreResult<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.role, m.content, m.task_id, m.at, m.sources, m.seq,
                    f.provider, f.model, f.rating
             FROM (SELECT * FROM messages ORDER BY seq DESC LIMIT ?1) AS m
             LEFT JOIN cloud_feedback AS f ON ?2 AND m.role = 'assistant' AND f.task_id = m.task_id
             ORDER BY m.seq",
        )?;
        let rows = stmt.query_map(params![limit, owner], |r| {
            let role: String = r.get(0)?;
            let task: Option<String> = r.get(2)?;
            let at: String = r.get(3)?;
            let purpose = message_purpose(&role, task.as_deref(), &at);
            let content = self.open_chat(&format!("{purpose}:content"), r.get(1)?)?;
            let sources = r
                .get::<_, Option<String>>(4)?
                .map(|value| {
                    let text = self.open_chat(&format!("{purpose}:sources"), value)?;
                    serde_json::from_str::<Value>(&text).map_err(|e| crypto_error(e.to_string()))
                })
                .transpose()?
                .unwrap_or(Value::Array(vec![]));
            let mut message = json!({"seq": r.get::<_, i64>(5)?, "role": role, "content": content,
                "task_id": task, "at": at, "sources": sources});
            if let Some(provider) = r.get::<_, Option<String>>(6)? {
                message["feedback"] = json!({
                    "provider": provider,
                    "model": r.get::<_, String>(7)?,
                    "rating": r.get::<_, Option<String>>(8)?,
                });
            }
            Ok(message)
        })?;
        rows.collect()
    }

    /// Fonti del turno immediatamente precedente, incluse eventuali risposte dopo conferma.
    pub(crate) fn last_turn_sources(&self) -> StoreResult<Vec<Value>> {
        let row = self.conn.query_row(
            "SELECT task_id, at, sources FROM messages
             WHERE task_id = (SELECT task_id FROM messages WHERE role = 'user' ORDER BY seq DESC LIMIT 1)
                 AND role = 'assistant'
             ORDER BY seq DESC LIMIT 1",
            [], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)),
        ).optional()?;
        let Some((task, at, Some(sources))) = row else {
            return Ok(Vec::new());
        };
        let purpose = message_purpose("assistant", task.as_deref(), &at);
        let text = self.open_chat(&format!("{purpose}:sources"), sources)?;
        serde_json::from_str(&text).map_err(|e| crypto_error(e.to_string()))
    }

    /// Ultimi turni, dal più recente. Solo una risposta cloud testuale ha la provenienza registrata.
    pub(crate) fn recent_cloud_turns(&self) -> StoreResult<Vec<ConversationTurn>> {
        let mut stmt = self.conn.prepare(
            "SELECT user.content, user.task_id, user.at, answer.content, answer.at, answer.cloud_provider
             FROM (SELECT seq, task_id, content, at FROM messages WHERE role = 'user'
                   ORDER BY seq DESC LIMIT 8) AS user
             LEFT JOIN messages AS answer ON answer.seq = (
                 SELECT seq FROM messages WHERE task_id = user.task_id AND role = 'assistant'
                 ORDER BY seq DESC LIMIT 1
             )
             ORDER BY user.seq DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let task: Option<String> = r.get(1)?;
            let at: String = r.get(2)?;
            let user = message_purpose("user", task.as_deref(), &at);
            let assistant = r
                .get::<_, Option<String>>(3)?
                .zip(r.get::<_, Option<String>>(4)?)
                .map(|(value, at)| {
                    let purpose = message_purpose("assistant", task.as_deref(), &at);
                    self.open_chat(&format!("{purpose}:content"), value)
                })
                .transpose()?;
            Ok(ConversationTurn {
                user: self.open_chat(&format!("{user}:content"), r.get(0)?)?,
                assistant,
                cloud_provider: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub(crate) fn cloud_feedback_candidate(
        &self,
        task: &str,
        provider: &str,
        model: &str,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO cloud_feedback(task_id, provider, model, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![task, provider, model, crate::now()],
        )?;
        self.conn.execute(
            "DELETE FROM cloud_feedback WHERE task_id IN
             (SELECT task_id FROM cloud_feedback ORDER BY coalesce(rated_at, created_at) DESC, rowid DESC LIMIT -1 OFFSET 1024)",
            [],
        )?;
        Ok(())
    }

    pub(crate) fn rate_cloud_reply(&self, task: &str, rating: &str) -> StoreResult<bool> {
        Ok(self.conn.execute(
            "UPDATE cloud_feedback SET rating = ?1,
              rated_at = CASE WHEN rating = ?1 THEN rated_at ELSE ?2 END WHERE task_id = ?3",
            params![rating, crate::now(), task],
        )? == 1)
    }

    pub(crate) fn cloud_quality(
        &self,
        provider: &str,
        model: &str,
    ) -> StoreResult<crate::cloud::QualityPreference> {
        self.conn.query_row(
            "SELECT count(CASE WHEN rating='helpful' THEN 1 END),
                    count(CASE WHEN rating='concise' THEN 1 END),
                    count(CASE WHEN rating='detailed' THEN 1 END),
                    count(CASE WHEN rating='precise' THEN 1 END)
             FROM (SELECT rating FROM cloud_feedback WHERE provider = ?1 AND model = ?2 AND rating IS NOT NULL
                   ORDER BY rated_at DESC, rowid DESC LIMIT 12)",
            params![provider, model],
            |row| Ok(crate::cloud::QualityPreference::from_counts(row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
    }

    pub(crate) fn cloud_feedback_profiles(&self, provider: &str) -> StoreResult<Vec<Value>> {
        let mut statement = self.conn.prepare(
            "WITH recent AS (
                SELECT model, rating, rated_at,
                       row_number() OVER (PARTITION BY model ORDER BY rated_at DESC, rowid DESC) AS position
                FROM cloud_feedback WHERE provider = ?1 AND rating IS NOT NULL
             )
             SELECT model, count(*), count(CASE WHEN rating='helpful' THEN 1 END),
                    count(CASE WHEN rating='concise' THEN 1 END),
                    count(CASE WHEN rating='detailed' THEN 1 END),
                    count(CASE WHEN rating='precise' THEN 1 END)
             FROM recent WHERE position <= 12
             GROUP BY model ORDER BY max(rated_at) DESC, model",
        )?;
        let profiles = statement.query_map([provider], |row| {
            let preference = crate::cloud::QualityPreference::from_counts(
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            );
            Ok(json!({
                "model": row.get::<_, String>(0)?,
                "count": row.get::<_, u32>(1)?,
                "length": preference.length_id(),
                "precision": preference.emphasizes_precision(),
            }))
        })?;
        profiles.collect()
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

    /// Aggiorna la presenza solo se il token è ancora attivo.
    pub fn device_touch(&self, doc: &Value, token_sha256: &str) -> StoreResult<bool> {
        self.conn
            .execute(
                "UPDATE devices SET doc = ?1 WHERE token_sha256 = ?2",
                params![doc.to_string(), token_sha256],
            )
            .map(|updated| updated != 0)
    }

    /// Registra la revoca e ritira il token nella stessa transazione, su una connessione indipendente.
    pub(crate) fn revoke_device(
        db: &Path,
        id: &str,
        revoked_at: &str,
    ) -> StoreResult<Option<(Value, bool)>> {
        let mut conn =
            Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(raw) = tx
            .query_row(
                "SELECT doc FROM devices WHERE device_id = ?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut doc: Value = serde_json::from_str(&raw).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        })?;
        let changed = doc.get("revoked_at").is_none();
        if changed {
            doc["revoked_at"] = revoked_at.into();
            tx.execute(
                "UPDATE devices SET doc = ?1, token_sha256 = 'revocato:' || device_id WHERE device_id = ?2",
                params![doc.to_string(), id],
            )?;
        }
        tx.commit()?;
        Ok(Some((doc, changed)))
    }

    /// Dispositivi con l'impronta del token (per registrare i nodi all'avvio).
    pub fn devices_with_hash(&self) -> StoreResult<Vec<(Value, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT doc, token_sha256 FROM devices ORDER BY device_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                serde_json::from_str(&r.get::<_, String>(0)?).unwrap_or(Value::Null),
                r.get(1)?,
            ))
        })?;
        rows.collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_are_replaced_and_isolated_by_provider_and_model() {
        use crate::cloud::QualityPreference;
        let store = Store::open(Path::new(":memory:")).unwrap();
        assert!(!store.rate_cloud_reply("missing", "precise").unwrap());
        store
            .cloud_feedback_candidate("task", "provider-a", "model-a")
            .unwrap();
        assert!(store.rate_cloud_reply("task", "concise").unwrap());
        assert_eq!(
            store.cloud_quality("provider-a", "model-a").unwrap(),
            QualityPreference::from_counts(0, 1, 0, 0)
        );
        assert_eq!(
            store.cloud_quality("provider-a", "model-b").unwrap(),
            QualityPreference::default()
        );
        assert_eq!(
            store.cloud_quality("provider-b", "model-a").unwrap(),
            QualityPreference::default()
        );
        assert_eq!(
            store.cloud_feedback_profiles("provider-a").unwrap(),
            vec![json!({"model":"model-a", "count":1, "length":"concise", "precision":false})]
        );
        assert!(store
            .cloud_feedback_profiles("provider-b")
            .unwrap()
            .is_empty());
        assert!(store.rate_cloud_reply("task", "helpful").unwrap());
        assert_eq!(
            store.cloud_quality("provider-a", "model-a").unwrap(),
            QualityPreference::default()
        );
    }

    #[test]
    fn history_keeps_the_latest_runs_and_recent_todos() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-store-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Store::open(&dir.join("jarvis.db")).unwrap();
        for wf in ["a", "b"] {
            for i in 0..5 {
                s.conn
                    .execute(
                        "INSERT INTO workflow_runs (run_id, workflow_id, started_at, doc) VALUES (?1, ?2, ?3, '{}')",
                        params![format!("{wf}{i}"), wf, format!("2026-01-0{}T00:00:00.000Z", i + 1)],
                    )
                    .unwrap();
            }
        }
        for (id, done) in [
            ("old", Some("2020-01-01T00:00:00.000Z")),
            ("new", Some("2026-11-01T00:00:00.000Z")),
            ("open", None),
        ] {
            s.conn
                .execute(
                    "INSERT INTO todos (id, text, priority, created_at, done_at) VALUES (?1, 'x', 'normale', '2020-01-01T00:00:00.000Z', ?2)",
                    params![id, done],
                )
                .unwrap();
        }
        assert_eq!(
            s.history_prune(2, "2026-08-01T00:00:00.000Z").unwrap(),
            6 + 1
        );
        let mut runs: Vec<String> = s
            .conn
            .prepare("SELECT run_id FROM workflow_runs ORDER BY run_id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        runs.sort();
        assert_eq!(runs, ["a3", "a4", "b3", "b4"]);
        let todos: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(todos, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
