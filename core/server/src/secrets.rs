//! Archivio dei segreti dei connettori (SECURITY_AND_PRIVACY.md: segreti centralizzati e accesso limitato).
//!
//! Ogni segreto è cifrato con XChaCha20-Poly1305 (nonce casuale) e salvato in SQLite. La chiave da 32 byte
//! sta nel portachiavi del sistema dove disponibile (portachiavi di macOS, Gestione credenziali di Windows;
//! ADR-0021), altrimenti in `data/.secrets.key` con permessi 0600 (Linux e fallback). Un file di chiave
//! presente, per esempio dopo un ripristino o da una versione precedente, viene spostato nel portachiavi e
//! cancellato solo dopo la rilettura. I segreti non tornano mai al client, all'agente o ai log: solo il Core
//! li legge al momento della chiamata.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::Digest;

pub struct SecretStore {
    cipher: XChaCha20Poly1305,
    conn: Connection,
    location: &'static str,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Custodia esterna della chiave: il portachiavi del sistema in produzione, una finta nei test.
pub trait KeyVault {
    /// `Ok(None)` se la voce non esiste; `Err` se il portachiavi non risponde.
    fn get(&self) -> Result<Option<Vec<u8>>, String>;
    fn set(&self, key: &[u8]) -> Result<(), String>;
}

/// Voce del portachiavi legata alla cartella dei dati, così più installazioni non si sovrascrivono.
#[cfg(any(target_os = "macos", target_os = "windows"))]
struct SystemVault(keyring::Entry);

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl KeyVault for SystemVault {
    fn get(&self) -> Result<Option<Vec<u8>>, String> {
        match self.0.get_secret() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, key: &[u8]) -> Result<(), String> {
        self.0.set_secret(key).map_err(|e| e.to_string())
    }
}

/// Nome della voce: impronta del percorso assoluto della cartella dei dati.
pub fn vault_account(data_dir: &Path) -> String {
    let abs = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let digest = sha2::Sha256::digest(abs.to_string_lossy().as_bytes());
    format!("secrets-key-{}", &hex::encode(digest)[..16])
}

/// Portachiavi del sistema, se la piattaforma lo offre e `JARVIS_SECRETS_KEYCHAIN` non vale `off`.
pub fn system_vault(data_dir: &Path) -> Option<Box<dyn KeyVault>> {
    if std::env::var("JARVIS_SECRETS_KEYCHAIN").is_ok_and(|v| v == "off") {
        return None;
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        keyring::Entry::new("JARVIS", &vault_account(data_dir))
            .ok()
            .map(|e| Box::new(SystemVault(e)) as Box<dyn KeyVault>)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = data_dir;
        None
    }
}

fn key_file(data_dir: &Path) -> PathBuf {
    data_dir.join(".secrets.key")
}

fn write_key_file(path: &Path, key: &[u8]) -> Result<(), String> {
    fs::write(path, key).map_err(|e| format!("chiave dei segreti: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn new_key() -> Result<Vec<u8>, String> {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).map_err(|e| e.to_string())?;
    Ok(k.to_vec())
}

/// Sposta la chiave nel portachiavi e la rilegge: vero solo se la copia è identica.
fn store_in_vault(vault: &dyn KeyVault, key: &[u8]) -> bool {
    vault.set(key).is_ok() && vault.get().ok().flatten().as_deref() == Some(key)
}

/// Chiave dei segreti e luogo in cui è custodita (`keychain` o `file`).
///
/// `has_secrets` evita di creare una chiave nuova, che renderebbe illeggibili i segreti esistenti, quando il
/// portachiavi non risponde: in quel caso l'apertura fallisce con un messaggio chiaro.
pub fn load_key(
    data_dir: &Path,
    vault: Option<&dyn KeyVault>,
    has_secrets: bool,
) -> Result<(Vec<u8>, &'static str), String> {
    let path = key_file(data_dir);
    if let Ok(k) = fs::read(&path) {
        if k.len() != 32 {
            return Err("chiave dei segreti danneggiata".into());
        }
        if let Some(v) = vault {
            if store_in_vault(v, &k) {
                fs::remove_file(&path).map_err(|e| format!("chiave dei segreti: {e}"))?;
                return Ok((k, "keychain"));
            }
        }
        return Ok((k, "file"));
    }
    if let Some(v) = vault {
        match v.get() {
            Ok(Some(k)) if k.len() == 32 => return Ok((k, "keychain")),
            Ok(Some(_)) => return Err("chiave dei segreti nel portachiavi danneggiata".into()),
            Ok(None) if has_secrets => {
                return Err(
                    "chiave dei segreti assente: ripristina un backup oppure cancella i segreti dei connettori"
                        .into(),
                )
            }
            Ok(None) => {
                let k = new_key()?;
                if store_in_vault(v, &k) {
                    return Ok((k, "keychain"));
                }
            }
            Err(e) if has_secrets => {
                return Err(format!("portachiavi del sistema non disponibile: {e}"))
            }
            Err(_) => {}
        }
    }
    let k = new_key()?;
    write_key_file(&path, &k)?;
    Ok((k, "file"))
}

/// Chiave corrente per il backup cifrato, letta dal file o dal portachiavi.
pub fn read_key(data_dir: &Path) -> Option<Vec<u8>> {
    if let Ok(k) = fs::read(key_file(data_dir)) {
        return Some(k);
    }
    system_vault(data_dir)?.get().ok().flatten()
}

impl SecretStore {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let vault = system_vault(data_dir);
        Self::open_with(data_dir, vault.as_deref())
    }

    pub fn open_with(data_dir: &Path, vault: Option<&dyn KeyVault>) -> Result<Self, String> {
        let conn = Connection::open(data_dir.join("jarvis.sqlite3")).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS secrets (name TEXT PRIMARY KEY, nonce TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let (key, location) = load_key(data_dir, vault, count > 0)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|e| e.to_string())?;
        Ok(Self {
            cipher,
            conn,
            location,
        })
    }

    /// Dove è custodita la chiave: `keychain` oppure `file`.
    pub fn location(&self) -> &'static str {
        self.location
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
        let s = SecretStore::open_with(&dir, None).unwrap();
        assert_eq!(s.location(), "file");
        s.put("connector:meteo", "tok-segretissimo-123").unwrap();
        drop(s);
        let raw = fs::read(dir.join("jarvis.sqlite3")).unwrap();
        assert!(
            !String::from_utf8_lossy(&raw).contains("tok-segretissimo-123"),
            "mai in chiaro su disco"
        );
        let s = SecretStore::open_with(&dir, None).unwrap();
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

    /// Portachiavi finto: memorizza la chiave e può simulare un portachiavi che non risponde.
    #[derive(Default)]
    struct FakeVault {
        key: std::cell::RefCell<Option<Vec<u8>>>,
        broken: bool,
    }

    impl KeyVault for FakeVault {
        fn get(&self) -> Result<Option<Vec<u8>>, String> {
            if self.broken {
                return Err("bloccato".into());
            }
            Ok(self.key.borrow().clone())
        }
        fn set(&self, key: &[u8]) -> Result<(), String> {
            if self.broken {
                return Err("bloccato".into());
            }
            *self.key.borrow_mut() = Some(key.to_vec());
            Ok(())
        }
    }

    fn fresh(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-vault-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn key_moves_from_file_to_keychain_and_secrets_stay_readable() {
        let dir = fresh("migrate");
        let s = SecretStore::open_with(&dir, None).unwrap();
        s.put("connector:a", "valore").unwrap();
        drop(s);
        assert!(dir.join(".secrets.key").exists());

        let vault = FakeVault::default();
        let s = SecretStore::open_with(&dir, Some(&vault)).unwrap();
        assert_eq!(s.location(), "keychain");
        assert!(
            !dir.join(".secrets.key").exists(),
            "file della chiave cancellato dopo lo spostamento"
        );
        assert_eq!(s.get("connector:a").unwrap().as_deref(), Some("valore"));
        drop(s);
        let s = SecretStore::open_with(&dir, Some(&vault)).unwrap();
        assert_eq!(s.get("connector:a").unwrap().as_deref(), Some("valore"));
    }

    #[test]
    fn new_install_uses_keychain_and_never_overwrites_existing_secrets() {
        let dir = fresh("new");
        let vault = FakeVault::default();
        let s = SecretStore::open_with(&dir, Some(&vault)).unwrap();
        assert_eq!(s.location(), "keychain");
        assert!(!dir.join(".secrets.key").exists());
        s.put("connector:b", "x").unwrap();
        drop(s);

        // Portachiavi bloccato con segreti presenti: nessuna chiave nuova, errore esplicito.
        let broken = FakeVault {
            broken: true,
            ..Default::default()
        };
        let err = SecretStore::open_with(&dir, Some(&broken)).err().unwrap();
        assert!(err.contains("portachiavi"), "{err}");
        assert!(!dir.join(".secrets.key").exists());

        // Voce cancellata con segreti presenti: errore, non una chiave nuova.
        let empty = FakeVault::default();
        let err = SecretStore::open_with(&dir, Some(&empty)).err().unwrap();
        assert!(err.contains("assente"), "{err}");

        // Senza segreti, un portachiavi bloccato ripiega sul file.
        let dir = fresh("fallback");
        let s = SecretStore::open_with(&dir, Some(&broken)).unwrap();
        assert_eq!(s.location(), "file");
        assert!(dir.join(".secrets.key").exists());
    }

    #[test]
    fn vault_account_depends_on_data_dir() {
        let a = fresh("acc-a");
        let b = fresh("acc-b");
        assert_ne!(vault_account(&a), vault_account(&b));
        assert_eq!(vault_account(&a), vault_account(&a));
    }
}
