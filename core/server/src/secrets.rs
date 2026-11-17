//! Archivio dei segreti dei connettori (SECURITY_AND_PRIVACY.md: segreti centralizzati e accesso limitato).
//!
//! Ogni segreto è cifrato con XChaCha20-Poly1305 (nonce casuale) e salvato in SQLite; la chiave da 32 byte
//! sta in `data/.secrets.key` con permessi 0600. I segreti non tornano mai al client, all'agente o ai log:
//! solo il Core li legge al momento della chiamata. Decisione da completare: chiave nel portachiavi del
//! sistema operativo (ADR-0013).

use std::fs;
use std::path::Path;

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};

pub struct SecretStore {
    cipher: XChaCha20Poly1305,
    conn: Connection,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

impl SecretStore {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let key_path = data_dir.join(".secrets.key");
        let key: Vec<u8> = match fs::read(&key_path) {
            Ok(k) if k.len() == 32 => k,
            Ok(_) => return Err("chiave dei segreti danneggiata".into()),
            Err(_) => {
                let mut k = [0u8; 32];
                getrandom::getrandom(&mut k).map_err(|e| e.to_string())?;
                fs::write(&key_path, k).map_err(|e| format!("chiave dei segreti: {e}"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
                        .map_err(|e| e.to_string())?;
                }
                k.to_vec()
            }
        };
        let conn = Connection::open(data_dir.join("jarvis.sqlite3")).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS secrets (name TEXT PRIMARY KEY, nonce TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|e| e.to_string())?;
        Ok(Self { cipher, conn })
    }

    pub fn put(&self, name: &str, value: &str) -> Result<(), String> {
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce).map_err(|e| e.to_string())?;
        let sealed = self
            .cipher
            .encrypt(XNonce::from_slice(&nonce), value.as_bytes())
            .map_err(|_| "cifratura del segreto non riuscita".to_string())?;
        self.conn
            .execute(
                "INSERT INTO secrets (name, nonce, value, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET nonce = excluded.nonce, value = excluded.value, updated_at = excluded.updated_at",
                params![name, b64().encode(nonce), b64().encode(sealed), crate::now()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Option<String>, String> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT nonce, value FROM secrets WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some((nonce, value)) = row else {
            return Ok(None);
        };
        let nonce = b64().decode(nonce).map_err(|e| e.to_string())?;
        let sealed = b64().decode(value).map_err(|e| e.to_string())?;
        let plain = self
            .cipher
            .decrypt(XNonce::from_slice(&nonce), sealed.as_slice())
            .map_err(|_| "segreto non decifrabile (chiave cambiata o dato alterato)".to_string())?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    pub fn delete(&self, name: &str) -> Result<bool, String> {
        Ok(self
            .conn
            .execute("DELETE FROM secrets WHERE name = ?1", [name])
            .map_err(|e| e.to_string())?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_encrypted_at_rest_and_survive_reopen() {
        let dir = std::env::temp_dir().join(format!("jarvis-secrets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let s = SecretStore::open(&dir).unwrap();
        s.put("connector:meteo", "tok-segretissimo-123").unwrap();
        drop(s);
        let raw = fs::read(dir.join("jarvis.sqlite3")).unwrap();
        assert!(
            !String::from_utf8_lossy(&raw).contains("tok-segretissimo-123"),
            "mai in chiaro su disco"
        );
        let s = SecretStore::open(&dir).unwrap();
        assert_eq!(
            s.get("connector:meteo").unwrap().as_deref(),
            Some("tok-segretissimo-123")
        );
        assert!(s.delete("connector:meteo").unwrap());
        assert_eq!(s.get("connector:meteo").unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.join(".secrets.key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
