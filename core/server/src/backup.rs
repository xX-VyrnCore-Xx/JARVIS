//! Backup cifrato e ripristino verificato (RNF-08, MEMORY_DATA_SYNC.md).
//!
//! Formato `jarvis-backup/1` (estensione `.jbak`): `JARVISB1` | sale (16 byte) | nonce (24 byte) |
//! archivio tar cifrato con XChaCha20-Poly1305. La chiave deriva dalla passphrase del proprietario
//! con Argon2id (64 MiB, 3 passaggi). Nessuna chiave viene salvata: senza passphrase il backup è illeggibile.
//!
//! L'archivio contiene `manifest.json` (file con SHA-256, testa della catena di audit), un'istantanea
//! coerente del database (`VACUUM INTO`), il registro di audit e la workspace. Il ripristino verifica
//! cifratura, impronte, catena di audit e `PRAGMA integrity_check` prima di sostituire i dati, e conserva
//! i dati precedenti in una cartella `…before-restore-<istante>`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"JARVISB1";
const FORMAT: &str = "jarvis-backup/1";
pub const MIN_PASSPHRASE: usize = 12;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;

fn key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| e.to_string())?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("generatore casuale del sistema");
    b
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let staging = path.with_file_name(format!(".jarvis-write-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = private_file(&staging)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result
}

pub(crate) fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Preferenze e identità da conservare anche attraverso gli aggiornamenti.
pub(crate) const DATA_SETTINGS: &[&str] = &[
    "setup.json",
    "updates.json",
    "cloud.json",
    "tls/core.crt",
    "tls/core.key",
];

/// File regolari della workspace (i link simbolici sono esclusi: potrebbero puntare fuori).
fn workspace_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

/// Crea il backup. `snapshot` scrive un'istantanea coerente del database nel percorso indicato.
pub fn create(
    data_dir: &Path,
    out_dir: &Path,
    passphrase: &str,
    snapshot: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(format!(
            "la passphrase deve avere almeno {MIN_PASSPHRASE} caratteri"
        ));
    }
    fs::create_dir_all(out_dir).map_err(|e| format!("cartella dei backup: {e}"))?;
    let tmp = out_dir.join(format!(
        ".snapshot-{}.sqlite3",
        uuid::Uuid::new_v4().simple()
    ));
    private_file(&tmp).map_err(|e| e.to_string())?;
    let db = snapshot(&tmp).and_then(|()| fs::read(&tmp).map_err(|e| e.to_string()));
    let _ = fs::remove_file(&tmp);
    let db = db?;

    let audit_path = data_dir.join("audit/core-security.jsonl");
    let audit = fs::read(&audit_path).unwrap_or_default();
    let events: Vec<Value> = String::from_utf8_lossy(&audit)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let chain_errors = jarvis_contracts::audit_chain_errors(&events);

    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("data/jarvis.sqlite3".into(), db),
        ("data/audit/core-security.jsonl".into(), audit),
    ];
    // Firme del registro (ADR-0030): la chiave privata resta nell'archivio dei segreti già incluso.
    if let Ok(sigs) = fs::read(data_dir.join("audit/core-security.jsonl.sig.jsonl")) {
        files.push(("data/audit/core-security.jsonl.sig.jsonl".into(), sigs));
    }
    // Chiave dei segreti dei connettori: senza di essa i segreti ripristinati sarebbero illeggibili.
    // Resta protetta dalla cifratura del backup.
    // Su macOS e Windows la chiave sta nel portachiavi (ADR-0021): il ripristino la riscrive come file e
    // l'apertura successiva la sposta di nuovo nel portachiavi.
    if let Some(key) = crate::secrets::read_key(data_dir) {
        files.push(("data/.secrets.key".into(), key));
    }
    for &name in DATA_SETTINGS {
        match fs::read(data_dir.join(name)) {
            Ok(data) => files.push((format!("data/{name}"), data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("lettura di {name}: {e}")),
        }
    }
    let workspace = data_dir.join("workspace");
    for path in workspace_files(&workspace) {
        let rel = path.strip_prefix(data_dir).map_err(|e| e.to_string())?;
        let name = format!(
            "data/{}",
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        );
        files.push((
            name,
            fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        ));
    }
    let total: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
    if total > MAX_BYTES {
        return Err("dati oltre 1 GiB: il formato del prototipo non li supporta".into());
    }
    let created_at = crate::now();
    let manifest = json!({
        "format": FORMAT,
        "created_at": created_at,
        "core_version": env!("CARGO_PKG_VERSION"),
        "files": files.iter().map(|(n, b)| json!({"path": n, "bytes": b.len(), "sha256": sha256(b)})).collect::<Vec<_>>(),
        "audit": {"events": events.len(), "head_hash": events.last().map(|e| e["hash"].clone()), "chain_valid": chain_errors.is_empty()},
    });

    let mut builder = tar::Builder::new(Vec::new());
    let mut append = |name: &str, bytes: &[u8]| -> Result<(), String> {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder
            .append_data(&mut header, name, bytes)
            .map_err(|e| e.to_string())
    };
    append(
        "manifest.json",
        serde_json::to_vec_pretty(&manifest)
            .expect("manifest")
            .as_slice(),
    )?;
    for (name, bytes) in &files {
        append(name, bytes)?;
    }
    let archive = builder.into_inner().map_err(|e| e.to_string())?;

    let salt: [u8; 16] = random();
    let nonce: [u8; 24] = random();
    let cipher = XChaCha20Poly1305::new(&key(passphrase, &salt)?.into());
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), archive.as_slice())
        .map_err(|_| "cifratura non riuscita".to_string())?;
    let mut out = Vec::with_capacity(8 + 16 + 24 + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);

    let id = uuid::Uuid::new_v4().simple();
    let path = out_dir.join(format!("jarvis-{}-{id}.jbak", stamp()));
    atomic_write(&path, &out).map_err(|e| format!("scrittura del backup: {e}"))?;
    Ok(json!({
        "path": path.to_string_lossy(), "file": path.file_name().map(|f| f.to_string_lossy().into_owned()),
        "bytes": out.len(), "files": files.len(), "created_at": created_at, "audit": manifest["audit"],
    }))
}

fn safe_relative(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    path.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

/// File estratti da un backup: percorso nell'archivio e contenuto.
pub type Entries = Vec<(String, Vec<u8>)>;

/// Decifra e verifica un backup senza toccare i dati: restituisce manifest ed elenco dei file.
pub fn open(file: &Path, passphrase: &str) -> Result<(Value, Entries), String> {
    let bytes = fs::read(file).map_err(|e| format!("lettura del backup: {e}"))?;
    if bytes.len() < 48 || &bytes[..8] != MAGIC {
        return Err("il file non è un backup JARVIS".into());
    }
    let (salt, nonce, sealed) = (&bytes[8..24], &bytes[24..48], &bytes[48..]);
    let cipher = XChaCha20Poly1305::new(&key(passphrase, salt)?.into());
    let archive = cipher
        .decrypt(XNonce::from_slice(nonce), sealed)
        .map_err(|_| "passphrase errata oppure backup alterato".to_string())?;
    let mut manifest = None;
    let mut files = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut reader = tar::Archive::new(archive.as_slice());
    for entry in reader.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        safe_relative(&name)
            .ok_or_else(|| format!("percorso non ammesso nell'archivio: {name}"))?;
        if !entry.header().entry_type().is_file() || !names.insert(name.clone()) {
            return Err(format!("voce non regolare o duplicata: {name}"));
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        if name == "manifest.json" {
            manifest = Some(
                serde_json::from_slice::<Value>(&data)
                    .map_err(|_| "manifest non valido".to_string())?,
            );
        } else {
            if !name.starts_with("data/") || name.len() == "data/".len() {
                return Err(format!("percorso inatteso: {name}"));
            }
            files.push((name, data));
        }
    }
    let manifest = manifest.ok_or("manifest assente")?;
    if manifest["format"] != FORMAT {
        return Err(format!("formato non supportato: {}", manifest["format"]));
    }
    let expected: std::collections::BTreeMap<String, String> = manifest["files"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|f| {
                    (
                        f["path"].as_str().unwrap_or("").to_string(),
                        f["sha256"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if expected.len() != files.len() {
        return Err("numero di file diverso dal manifest".into());
    }
    for (name, data) in &files {
        if expected.get(name) != Some(&sha256(data)) {
            return Err(format!("impronta non corrispondente: {name}"));
        }
    }
    Ok((manifest, files))
}

/// Controlla SQLite, inclusa la presenza dello schema JARVIS, prima di sostituire i dati.
pub(crate) fn validate_database(path: &Path) -> Result<(), String> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("apertura del database: {e}"))?;
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("verifica del database: {e}"))?;
    if check != "ok" {
        return Err(format!("database non integro: {check}"));
    }
    let tables: u32 = conn.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('tasks','messages')",
        [], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    if tables != 2 {
        return Err("schema del database JARVIS assente".into());
    }
    Ok(())
}

fn inspect(staging: &Path, manifest: &Value, files: &Entries) -> Result<Value, String> {
    validate_database(&staging.join("jarvis.sqlite3"))?;
    let audit = files
        .iter()
        .find(|(name, _)| name == "data/audit/core-security.jsonl")
        .ok_or("registro di audit assente")?;
    let events: Vec<Value> = String::from_utf8_lossy(&audit.1)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("registro di audit non valido: {e}"))?;
    let chain = jarvis_contracts::audit_chain_errors(&events);
    if manifest["audit"]["chain_valid"] == true && !chain.is_empty() {
        return Err(format!("catena di audit non valida: {}", chain.join("; ")));
    }
    Ok(json!({
        "created_at": manifest["created_at"], "core_version": manifest["core_version"], "files": files.len(),
        "audit_events": events.len(), "audit_chain_valid": chain.is_empty(), "database": "ok",
    }))
}

/// Verifica un backup presente nella cartella dei backup, senza sostituire i dati.
pub fn verify_saved(dir: &Path, name: &str, passphrase: &str) -> Result<Value, String> {
    if name.contains(['/', '\\', ':']) || !name.ends_with(".jbak") || name == ".jbak" {
        return Err("nome del backup non valido".into());
    }
    let path = dir.join(name);
    if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return Err("backup non trovato".into());
    }
    let (manifest, files) = open(&path, passphrase)?;
    let db = files
        .iter()
        .find(|(name, _)| name == "data/jarvis.sqlite3")
        .ok_or("database assente")?;
    let staging = dir.join(format!(".verifica-{}", uuid::Uuid::new_v4().simple()));
    private_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        private_file(&staging.join("jarvis.sqlite3"))
            .and_then(|mut f| f.write_all(&db.1))
            .map_err(|e| format!("copia di verifica: {e}"))?;
        inspect(&staging, &manifest, &files)
    })();
    let _ = fs::remove_dir_all(staging);
    result
}

/// Ripristina in `data_dir` con il Core fermo. I dati sostituiti restano nella cartella di recupero.
pub fn restore(file: &Path, passphrase: &str, data_dir: &Path) -> Result<Value, String> {
    let (manifest, files) = open(file, passphrase)?;
    let name = data_dir
        .file_name()
        .ok_or("cartella dati non valida")?
        .to_string_lossy();
    let parent = data_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().simple();
    let staging = parent.join(format!(".jarvis-restore-{id}"));
    let aside = parent.join(format!("{name}.before-restore-{}-{id}", stamp()));
    private_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        for (name, data) in &files {
            let rel = Path::new(name)
                .strip_prefix("data")
                .map_err(|_| format!("percorso inatteso: {name}"))?;
            let target = staging.join(rel);
            fs::create_dir_all(target.parent().ok_or("cartella non valida")?)
                .map_err(|e| e.to_string())?;
            private_file(&target)
                .and_then(|mut file| file.write_all(data))
                .map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(staging.join("workspace")).map_err(|e| e.to_string())?;
        let mut summary = inspect(&staging, &manifest, &files)?;
        let previous = crate::rollback::replace_data(data_dir, &staging, &aside)?;
        summary["restored_from"] = json!(file.to_string_lossy());
        summary["previous_data"] = json!(previous.then(|| aside.to_string_lossy()));
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

/// Elenco dei backup presenti in una cartella (più recenti prima).
pub fn list(dir: &Path) -> Vec<Value> {
    let mut items: Vec<Value> = fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().is_ok_and(|kind| kind.is_file()) && e.path().extension().is_some_and(|x| x == "jbak"))
                .map(|e| json!({"file": e.file_name().to_string_lossy(), "bytes": e.metadata().map(|m| m.len()).unwrap_or(0)}))
                .collect()
        })
        .unwrap_or_default();
    items.sort_by(|a, b| b["file"].as_str().cmp(&a["file"].as_str()));
    items
}
