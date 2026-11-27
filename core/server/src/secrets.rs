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
    pub(crate) chat_cipher: std::sync::Arc<crate::chat_crypto::ChatCipher>,
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

/// Voce del portachiavi legata alla cartella dei dati, così più installazioni non si sovrascrivono. Se la voce
/// principale manca si cercano quelle con nomi alternativi dello stesso percorso (ADR-0231) e, trovata, la chiave
/// viene copiata nella principale: un percorso scritto in un'altra forma non deve far perdere i segreti.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
struct SystemVault {
    primary: keyring::Entry,
    alternates: Vec<keyring::Entry>,
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl KeyVault for SystemVault {
    fn get(&self) -> Result<Option<Vec<u8>>, String> {
        match self.primary.get_secret() {
            Ok(k) => return Ok(Some(k)),
            Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
        for entry in &self.alternates {
            if let Ok(k) = entry.get_secret() {
                if k.len() == 32 {
                    let _ = self.primary.set_secret(&k);
                    return Ok(Some(k));
                }
            }
        }
        Ok(None)
    }
    fn set(&self, key: &[u8]) -> Result<(), String> {
        self.primary.set_secret(key).map_err(|e| e.to_string())
    }
}

fn account_for(path: &Path) -> String {
    let digest = sha2::Sha256::digest(path.to_string_lossy().as_bytes());
    format!("secrets-key-{}", &hex::encode(digest)[..16])
}

/// Nome della voce: impronta del percorso assoluto della cartella dei dati.
pub fn vault_account(data_dir: &Path) -> String {
    let abs = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    account_for(&abs)
}

/// Nomi alternativi dello stesso percorso: come è stato scritto e, su macOS, con o senza il prefisso `/private`
/// (le cartelle temporanee e `/var` ne sono un esempio). Il nome principale non è incluso.
pub fn alternate_vault_accounts(data_dir: &Path) -> Vec<String> {
    let primary = vault_account(data_dir);
    let canonical = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let mut forms: Vec<PathBuf> = vec![data_dir.to_path_buf()];
    let text = canonical.to_string_lossy().to_string();
    if let Some(rest) = text.strip_prefix("/private") {
        forms.push(PathBuf::from(rest));
    } else if text.starts_with("/var") || text.starts_with("/tmp") || text.starts_with("/etc") {
        forms.push(PathBuf::from(format!("/private{text}")));
    }
    let mut out: Vec<String> = Vec::new();
    for form in forms {
        let account = account_for(&form);
        if account != primary && !out.contains(&account) {
            out.push(account);
        }
    }
    out
}

/// Portachiavi del sistema, se la piattaforma lo offre e `JARVIS_SECRETS_KEYCHAIN` non vale `off`.
pub fn system_vault(data_dir: &Path) -> Option<Box<dyn KeyVault>> {
    if std::env::var("JARVIS_SECRETS_KEYCHAIN").is_ok_and(|v| v == "off") {
        return None;
    }
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        // Linux senza sessione D-Bus (server, servizi di sistema): nessun portachiavi, si usa il file 0600.
        #[cfg(target_os = "linux")]
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS")?;
        let primary = keyring::Entry::new("JARVIS", &vault_account(data_dir)).ok()?;
        let alternates = alternate_vault_accounts(data_dir)
            .iter()
            .filter_map(|account| keyring::Entry::new("JARVIS", account).ok())
            .collect();
        Some(Box::new(SystemVault {
            primary,
            alternates,
        }) as Box<dyn KeyVault>)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = data_dir;
        None
    }
}

fn key_file(data_dir: &Path) -> PathBuf {
    data_dir.join(".secrets.key")
}

fn write_key_file(path: &Path, key: &[u8]) -> Result<(), String> {
    crate::backup::atomic_write(path, key).map_err(|e| format!("chiave dei segreti: {e}"))
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
        let protected = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'chat_crypto_migration')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?;
        let (key, location) = load_key(data_dir, vault, count > 0 || protected)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|e| e.to_string())?;
        Ok(Self {
            cipher,
            chat_cipher: std::sync::Arc::new(crate::chat_crypto::ChatCipher::from_secret(&key)),
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
        if nonce.len() != 24 {
            return Err("nonce del segreto non valido".into());
        }
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
        // I nomi alternativi non includono mai il principale e non si ripetono.
        let alternates = alternate_vault_accounts(&a);
        assert!(!alternates.contains(&vault_account(&a)));
        let mut unique = alternates.clone();
        unique.dedup();
        assert_eq!(unique, alternates);
        // Con il prefisso /private la forma senza prefisso è fra le alternative (e viceversa).
        let with_prefix = alternate_vault_accounts(Path::new("/private/var/jarvis-inesistente"));
        assert!(with_prefix.contains(&account_for(Path::new("/var/jarvis-inesistente"))));
        let without = alternate_vault_accounts(Path::new("/var/jarvis-inesistente"));
        assert!(without.contains(&account_for(Path::new("/private/var/jarvis-inesistente"))));
        assert_eq!(vault_account(&a), vault_account(&a));
    }

    /// Linux con un bus D-Bus che non risponde (sessione chiusa, servizio assente): si ripiega sul file
    /// invece di bloccare l'avvio, purché non ci siano segreti cifrati con una chiave del portachiavi.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_without_secret_service_falls_back_to_the_file() {
        let dir = fresh("linux");
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/jarvis-test-bus",
        );
        let vault = system_vault(&dir);
        assert!(
            vault.is_some(),
            "con una sessione D-Bus dichiarata si prova il portachiavi"
        );
        let s = SecretStore::open_with(&dir, vault.as_deref()).unwrap();
        assert_eq!(s.location(), "file");
        std::env::remove_var("DBUS_SESSION_BUS_ADDRESS");
        assert!(
            system_vault(&dir).is_none(),
            "senza sessione D-Bus niente portachiavi"
        );
    }
}
