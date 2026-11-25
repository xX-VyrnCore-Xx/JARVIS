//! Account degli utenti (ADR-0169): più persone possono usare JARVIS, ciascuna con la propria conversazione, i propri
//! ricordi e la propria personalità, **senza** accedere ai dati, agli strumenti o ai segreti del proprietario.
//!
//! Questo modulo conserva solo ciò che riguarda gli account: password (Argon2id, mai in chiaro), sessioni (il token
//! è mostrato una volta e nel database resta solo il suo hash), inviti monouso, messaggi e ricordi dell'utente
//! (cifrati con la chiave della chat del Core). Le regole di chi può registrarsi stanno in `users-settings.json`.

use std::path::Path;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::store::{Store, StoreResult};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL, created_at TEXT NOT NULL, last_login_at TEXT, disabled INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS user_sessions (token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL, created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL, last_seen_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS user_sessions_by_user ON user_sessions (user_id);
CREATE TABLE IF NOT EXISTS user_invites (code_hash TEXT PRIMARY KEY, created_at TEXT NOT NULL, expires_at TEXT NOT NULL,
    used_by TEXT);
CREATE TABLE IF NOT EXISTS user_messages (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, role TEXT NOT NULL,
    content TEXT NOT NULL, at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS user_messages_by_user ON user_messages (user_id, id);
CREATE TABLE IF NOT EXISTS user_memories (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, text TEXT NOT NULL,
    at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS user_memories_by_user ON user_memories (user_id, id);
";

const SETTINGS_FILE: &str = "users-settings.json";
pub const SESSION_DAYS: i64 = 30;
pub const MAX_USERS: i64 = 50;
pub const MAX_MESSAGES_PER_USER: i64 = 400;
pub const MAX_MEMORIES_PER_USER: i64 = 100;
pub const INVITE_HOURS: i64 = 72;

const COMMON_PASSWORDS: [&str; 12] = [
    "password12",
    "password123",
    "passwordpassword",
    "1234567890",
    "12345678910",
    "qwertyuiop",
    "qwerty12345",
    "iloveyou12",
    "letmein1234",
    "administrator",
    "welcome1234",
    "abcdefghij",
];

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub pw_hash: String,
    pub disabled: bool,
}

impl UserRow {
    /// Vista pubblica: mai l'impronta della password.
    pub fn view(&self) -> Value {
        json!({"id": self.id, "username": self.username, "display_name": self.display_name, "disabled": self.disabled})
    }
}

fn stamp(delta: chrono::Duration) -> String {
    (chrono::Utc::now() + delta).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Dati casuali in esadecimale (`bytes` byte).
fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::getrandom(&mut buffer).expect("generatore casuale del sistema");
    hex::encode(buffer)
}

// ------------------------------------------------------------------ Regole di accesso del proprietario

/// Impostazioni scelte dal proprietario: chi può registrarsi e se gli utenti possono usare i suoi modelli cloud.
pub fn settings(dir: &Path) -> Value {
    let doc: Value = std::fs::read(dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let registration = match doc["registration"].as_str() {
        Some("open") => "open",
        Some("closed") => "closed",
        _ => "invite",
    };
    json!({
        "registration": registration,
        // Usare i modelli cloud del proprietario ha un costo e invia i messaggi dell'utente a quel fornitore:
        // resta spento finché il proprietario non lo attiva.
        "cloud": doc["cloud"].as_bool().unwrap_or(false),
        "registrations": ["closed", "invite", "open"],
    })
}

pub fn set_settings(dir: &Path, patch: &Value) -> Result<Value, String> {
    let mut current = settings(dir);
    if let Some(value) = patch.get("registration") {
        match value.as_str() {
            Some(r @ ("closed" | "invite" | "open")) => current["registration"] = json!(r),
            _ => return Err("«registration»: closed, invite oppure open".into()),
        }
    }
    if let Some(value) = patch.get("cloud") {
        current["cloud"] = json!(value.as_bool().ok_or("«cloud» deve essere vero o falso")?);
    }
    std::fs::write(
        dir.join(SETTINGS_FILE),
        json!({"registration": current["registration"], "cloud": current["cloud"]}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(current)
}

// ------------------------------------------------------------------ Nome utente e password

pub fn valid_username(name: &str) -> bool {
    (3..=32).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        && name.as_bytes()[0].is_ascii_alphanumeric()
}

/// Nome mostrato: lettere, numeri, spazi, apostrofo, punto e trattino; nessun recapito.
pub fn valid_display_name(name: &str) -> bool {
    let name = name.trim();
    (1..=40).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '-' | '.'))
}

pub fn check_password(password: &str, username: &str) -> Result<(), String> {
    let length = password.chars().count();
    if !(10..=128).contains(&length) {
        return Err("La password deve avere da 10 a 128 caratteri".into());
    }
    let lower = password.to_lowercase();
    if lower.contains(username) && username.len() >= 3 {
        return Err("La password non deve contenere il nome utente".into());
    }
    if COMMON_PASSWORDS.contains(&lower.as_str())
        || password
            .chars()
            .all(|c| c == password.chars().next().unwrap_or(' '))
    {
        return Err("Password troppo comune: scegline una meno prevedibile".into());
    }
    if password.chars().any(char::is_control) {
        return Err("La password contiene caratteri non validi".into());
    }
    Ok(())
}

fn hasher() -> Argon2<'static> {
    // Parametri minimi consigliati da OWASP per Argon2id: 19 MiB, 2 passaggi, 1 corsia.
    let params = Params::new(19 * 1024, 2, 1, None).expect("parametri Argon2 validi");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Impronta PHC della password (salt casuale per ogni password).
pub fn hash_password(password: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| e.to_string())?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|parsed| {
        hasher()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Verifica contro un'impronta fittizia: un nome utente inesistente costa lo stesso tempo di una password sbagliata.
pub fn verify_dummy(password: &str) {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash_password("password-fittizia-0").unwrap_or_default());
    let _ = verify_password(hash, password);
}

// ------------------------------------------------------------------ Inviti

/// Crea un invito monouso; il codice in chiaro si mostra una volta sola (nel database resta il suo hash).
pub fn create_invite(store: &Store) -> StoreResult<(String, String)> {
    let code = random_hex(16);
    let expires = stamp(chrono::Duration::hours(INVITE_HOURS));
    store.conn.execute(
        "INSERT INTO user_invites (code_hash, created_at, expires_at, used_by) VALUES (?1, ?2, ?3, NULL)",
        params![digest(&code), crate::now(), expires],
    )?;
    // Inviti scaduti da oltre una settimana: si eliminano.
    store.conn.execute(
        "DELETE FROM user_invites WHERE expires_at < ?1",
        [stamp(chrono::Duration::days(-7))],
    )?;
    Ok((code, expires))
}

/// Consuma un invito valido e non usato; l'operazione è atomica.
pub fn consume_invite(store: &Store, code: &str, user_id: &str) -> StoreResult<bool> {
    if code.len() != 32 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let changed = store.conn.execute(
        "UPDATE user_invites SET used_by = ?1 WHERE code_hash = ?2 AND used_by IS NULL AND expires_at > ?3",
        params![user_id, digest(&code.to_ascii_lowercase()), crate::now()],
    )?;
    Ok(changed == 1)
}

pub fn invite_valid(store: &Store, code: &str) -> StoreResult<bool> {
    if code.len() != 32 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let found: Option<i64> = store
        .conn
        .query_row(
            "SELECT 1 FROM user_invites WHERE code_hash = ?1 AND used_by IS NULL AND expires_at > ?2",
            params![digest(&code.to_ascii_lowercase()), crate::now()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

// ------------------------------------------------------------------ Utenti e sessioni

pub fn user_count(store: &Store) -> StoreResult<i64> {
    store
        .conn
        .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: r.get(0)?,
        username: r.get(1)?,
        display_name: r.get(2)?,
        pw_hash: r.get(3)?,
        disabled: r.get::<_, i64>(4)? != 0,
    })
}

const COLUMNS: &str = "id, username, display_name, pw_hash, disabled";

pub fn user_by_name(store: &Store, username: &str) -> StoreResult<Option<UserRow>> {
    store
        .conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM users WHERE username = ?1"),
            [username],
            row,
        )
        .optional()
}

pub fn user_by_id(store: &Store, id: &str) -> StoreResult<Option<UserRow>> {
    store
        .conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM users WHERE id = ?1"),
            [id],
            row,
        )
        .optional()
}

/// Crea l'utente. `None` se il nome è già preso.
pub fn create_user(
    store: &Store,
    username: &str,
    display_name: &str,
    pw_hash: &str,
) -> StoreResult<Option<String>> {
    let id = format!("u{}", random_hex(8));
    let inserted = store.conn.execute(
        "INSERT OR IGNORE INTO users (id, username, display_name, pw_hash, created_at, disabled)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![id, username, display_name.trim(), pw_hash, crate::now()],
    )?;
    Ok((inserted == 1).then_some(id))
}

pub fn set_password(store: &Store, id: &str, pw_hash: &str) -> StoreResult<()> {
    store.conn.execute(
        "UPDATE users SET pw_hash = ?1 WHERE id = ?2",
        params![pw_hash, id],
    )?;
    Ok(())
}

pub fn set_display_name(store: &Store, id: &str, name: &str) -> StoreResult<()> {
    store.conn.execute(
        "UPDATE users SET display_name = ?1 WHERE id = ?2",
        params![name.trim(), id],
    )?;
    Ok(())
}

pub fn set_disabled(store: &Store, id: &str, disabled: bool) -> StoreResult<bool> {
    let changed = store.conn.execute(
        "UPDATE users SET disabled = ?1 WHERE id = ?2",
        params![disabled as i64, id],
    )?;
    if disabled {
        end_sessions(store, id)?;
    }
    Ok(changed == 1)
}

/// Elimina l'utente con sessioni, messaggi e ricordi. La cartella della personalità la toglie chi chiama.
pub fn delete_user(store: &Store, id: &str) -> StoreResult<bool> {
    for table in ["user_sessions", "user_messages", "user_memories"] {
        store
            .conn
            .execute(&format!("DELETE FROM {table} WHERE user_id = ?1"), [id])?;
    }
    Ok(store
        .conn
        .execute("DELETE FROM users WHERE id = ?1", [id])?
        == 1)
}

/// Nuova sessione: il token (64 caratteri esadecimali) si restituisce una volta sola.
pub fn new_session(store: &Store, user_id: &str) -> StoreResult<String> {
    let token = random_hex(32);
    let now = crate::now();
    store.conn.execute(
        "INSERT INTO user_sessions (token_hash, user_id, created_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?3)",
        params![digest(&token), user_id, now, stamp(chrono::Duration::days(SESSION_DAYS))],
    )?;
    store.conn.execute(
        "UPDATE users SET last_login_at = ?1 WHERE id = ?2",
        params![now, user_id],
    )?;
    // Sessioni scadute: si eliminano.
    store.conn.execute(
        "DELETE FROM user_sessions WHERE expires_at < ?1",
        [crate::now()],
    )?;
    Ok(token)
}

/// Utente della sessione, se il token è valido, non scaduto e l'account non è disattivato.
pub fn session_user(store: &Store, token: &str) -> StoreResult<Option<UserRow>> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let hash = digest(&token.to_ascii_lowercase());
    let found: Option<String> = store
        .conn
        .query_row(
            "SELECT user_id FROM user_sessions WHERE token_hash = ?1 AND expires_at > ?2",
            params![hash, crate::now()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(user_id) = found else {
        return Ok(None);
    };
    let Some(user) = user_by_id(store, &user_id)?.filter(|u| !u.disabled) else {
        return Ok(None);
    };
    store.conn.execute(
        "UPDATE user_sessions SET last_seen_at = ?1 WHERE token_hash = ?2",
        params![crate::now(), hash],
    )?;
    Ok(Some(user))
}

pub fn end_session(store: &Store, token: &str) -> StoreResult<()> {
    store.conn.execute(
        "DELETE FROM user_sessions WHERE token_hash = ?1",
        [digest(&token.to_ascii_lowercase())],
    )?;
    Ok(())
}

pub fn end_sessions(store: &Store, user_id: &str) -> StoreResult<()> {
    store
        .conn
        .execute("DELETE FROM user_sessions WHERE user_id = ?1", [user_id])?;
    Ok(())
}

/// Elenco per il proprietario: nessuna impronta, nessun contenuto.
pub fn list_users(store: &Store) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT u.id, u.username, u.display_name, u.created_at, u.last_login_at, u.disabled,
                (SELECT COUNT(*) FROM user_messages m WHERE m.user_id = u.id),
                (SELECT COUNT(*) FROM user_sessions s WHERE s.user_id = u.id AND s.expires_at > ?1)
         FROM users u ORDER BY u.created_at",
    )?;
    let rows = stmt.query_map([crate::now()], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?, "username": r.get::<_, String>(1)?, "display_name": r.get::<_, String>(2)?,
            "created_at": r.get::<_, String>(3)?, "last_login_at": r.get::<_, Option<String>>(4)?,
            "disabled": r.get::<_, i64>(5)? != 0, "messages": r.get::<_, i64>(6)?, "sessions": r.get::<_, i64>(7)?,
        }))
    })?;
    rows.collect()
}

// ------------------------------------------------------------------ Messaggi e ricordi dell'utente

pub fn add_message(store: &Store, user_id: &str, role: &str, text: &str) -> StoreResult<()> {
    let sealed = store.seal_chat(&format!("usermsg:{user_id}"), text)?;
    store.conn.execute(
        "INSERT INTO user_messages (user_id, role, content, at) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, role, sealed, crate::now()],
    )?;
    store.conn.execute(
        "DELETE FROM user_messages WHERE user_id = ?1 AND id <= (
             SELECT id FROM user_messages WHERE user_id = ?1 ORDER BY id DESC LIMIT 1 OFFSET ?2)",
        params![user_id, MAX_MESSAGES_PER_USER],
    )?;
    Ok(())
}

/// Ultimi messaggi, dal più vecchio al più recente.
pub fn history(store: &Store, user_id: &str, limit: i64) -> StoreResult<Vec<Value>> {
    let mut stmt = store.conn.prepare(
        "SELECT id, role, content, at FROM user_messages WHERE user_id = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map(params![user_id, limit], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (id, role, content, at) in rows.into_iter().rev() {
        let text = store.open_chat(&format!("usermsg:{user_id}"), content)?;
        out.push(json!({"id": id, "role": role, "content": text, "at": at}));
    }
    Ok(out)
}

pub fn recent_user_messages(store: &Store, user_id: &str, since: &str) -> StoreResult<i64> {
    store.conn.query_row(
        "SELECT COUNT(*) FROM user_messages WHERE user_id = ?1 AND role = 'user' AND at >= ?2",
        params![user_id, since],
        |r| r.get(0),
    )
}

pub fn clear_history(store: &Store, user_id: &str) -> StoreResult<()> {
    store
        .conn
        .execute("DELETE FROM user_messages WHERE user_id = ?1", [user_id])?;
    Ok(())
}

pub fn add_memory(store: &Store, user_id: &str, text: &str) -> StoreResult<Option<i64>> {
    let count: i64 = store.conn.query_row(
        "SELECT COUNT(*) FROM user_memories WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if count >= MAX_MEMORIES_PER_USER {
        return Ok(None);
    }
    let sealed = store.seal_chat(&format!("usermem:{user_id}"), text)?;
    store.conn.execute(
        "INSERT INTO user_memories (user_id, text, at) VALUES (?1, ?2, ?3)",
        params![user_id, sealed, crate::now()],
    )?;
    Ok(Some(store.conn.last_insert_rowid()))
}

pub fn memories(store: &Store, user_id: &str) -> StoreResult<Vec<(i64, String)>> {
    let mut stmt = store
        .conn
        .prepare("SELECT id, text FROM user_memories WHERE user_id = ?1 ORDER BY id")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([user_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(id, text)| Ok((id, store.open_chat(&format!("usermem:{user_id}"), text)?)))
        .collect()
}

/// Elimina un ricordo **solo** se appartiene all'utente indicato.
pub fn delete_memory(store: &Store, user_id: &str, id: i64) -> StoreResult<bool> {
    Ok(store.conn.execute(
        "DELETE FROM user_memories WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )? == 1)
}

pub fn clear_memories(store: &Store, user_id: &str) -> StoreResult<()> {
    store
        .conn
        .execute("DELETE FROM user_memories WHERE user_id = ?1", [user_id])?;
    Ok(())
}

/// Comandi espliciti nella chat dell'utente, riconosciuti senza interrogare un modello.
#[derive(Debug, PartialEq)]
pub enum Command {
    Remember(String),
    ForgetAll,
    /// «chiamami Marco»: nuova forma di rivolgersi.
    CallMe(String),
    /// «dammi del tu» (falso) o «dammi del lei» (vero).
    Formal(bool),
    /// «sii più diretto / cordiale / formale».
    Tone(&'static str),
}

pub fn parse_command(message: &str) -> Option<Command> {
    let text = message.trim();
    let lower = text.to_lowercase();
    for prefix in ["ricorda che ", "ricordati che ", "ricorda: "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let start = text.len() - rest.len();
            let value = text[start..].trim().trim_end_matches('.').trim();
            if !value.is_empty() {
                return Some(Command::Remember(value.chars().take(300).collect()));
            }
        }
    }
    let bare = lower.trim_end_matches(['.', '!']);
    match bare {
        "dimentica tutto" | "dimentica tutti i ricordi" | "cancella i miei ricordi" => {
            return Some(Command::ForgetAll)
        }
        "dammi del tu" | "dammi pure del tu" | "usa il tu" | "dammi del tu per favore" => {
            return Some(Command::Formal(false))
        }
        "dammi del lei" | "dammi pure del lei" | "usa il lei" | "dammi del lei per favore" => {
            return Some(Command::Formal(true))
        }
        "sii più diretto" | "sii più breve" | "rispondi in modo più diretto" => {
            return Some(Command::Tone("diretto"))
        }
        "sii più cordiale" | "sii più caloroso" | "rispondi in modo più cordiale" => {
            return Some(Command::Tone("cordiale"))
        }
        "sii più formale" | "rispondi in modo più formale" => {
            return Some(Command::Tone("formale"))
        }
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix("chiamami ") {
        let start = text.len() - rest.len();
        let name = text[start..].trim().trim_end_matches(['.', '!']).trim();
        if !name.is_empty() && crate::persona::valid_address(name) {
            return Some(Command::CallMe(name.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_display_names_and_passwords_are_validated() {
        for ok in ["mario", "m.rossi", "anna_b-2", "abc"] {
            assert!(valid_username(ok), "{ok}");
        }
        for bad in [
            "ab",
            "Mario",
            "-mario",
            "ma rio",
            "mario@x.it",
            &"a".repeat(33),
            "",
        ] {
            assert!(!valid_username(bad), "{bad}");
        }
        assert!(valid_display_name("Anna Maria D'Angelo"));
        assert!(
            !valid_display_name("anna@example.org")
                && !valid_display_name("")
                && !valid_display_name(&"x".repeat(41))
        );
        assert!(check_password("una frase lunga e strana", "mario").is_ok());
        assert!(check_password("corta", "mario").is_err());
        assert!(
            check_password("mario-mario-1", "mario").is_err(),
            "contiene il nome utente"
        );
        assert!(
            check_password("password123", "mario").is_err(),
            "password comune"
        );
        assert!(
            check_password("aaaaaaaaaaaa", "mario").is_err(),
            "caratteri tutti uguali"
        );
        assert!(
            check_password(&"x1".repeat(70), "mario").is_err(),
            "oltre 128"
        );
    }

    #[test]
    fn password_hashes_are_salted_verified_and_never_plain() {
        let first = hash_password("una frase lunga e strana").unwrap();
        let second = hash_password("una frase lunga e strana").unwrap();
        assert_ne!(first, second, "salt diverso a ogni password");
        assert!(first.starts_with("$argon2id$") && !first.contains("frase"));
        assert!(verify_password(&first, "una frase lunga e strana"));
        assert!(!verify_password(&first, "una frase lunga e strana!"));
        assert!(!verify_password("non un hash", "x"));
        verify_dummy("qualsiasi");
    }

    #[test]
    fn registration_settings_default_to_invite_only_and_no_cloud() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-acc-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let defaults = settings(&dir);
        assert_eq!(defaults["registration"], "invite");
        assert_eq!(defaults["cloud"], false);
        let changed =
            set_settings(&dir, &json!({"registration": "closed", "cloud": true})).unwrap();
        assert_eq!(
            (changed["registration"].as_str(), changed["cloud"].as_bool()),
            (Some("closed"), Some(true))
        );
        assert_eq!(settings(&dir)["registration"], "closed");
        assert!(set_settings(&dir, &json!({"registration": "tutti"})).is_err());
        assert!(set_settings(&dir, &json!({"cloud": "sì"})).is_err());
    }

    #[test]
    fn commands_are_recognised_in_the_users_chat() {
        assert_eq!(
            parse_command("Ricorda che amo il caffè."),
            Some(Command::Remember("amo il caffè".into()))
        );
        assert_eq!(parse_command("ricorda che "), None);
        assert_eq!(parse_command("Dimentica tutto!"), Some(Command::ForgetAll));
        assert_eq!(parse_command("che ore sono?"), None);
        assert_eq!(
            parse_command("Chiamami Marco."),
            Some(Command::CallMe("Marco".into()))
        );
        assert_eq!(parse_command("chiamami m4rco@x"), None);
        assert_eq!(parse_command("dammi del tu"), Some(Command::Formal(false)));
        assert_eq!(parse_command("Dammi del lei!"), Some(Command::Formal(true)));
        assert_eq!(
            parse_command("sii più cordiale"),
            Some(Command::Tone("cordiale"))
        );
    }
}
