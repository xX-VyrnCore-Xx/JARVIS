//! Aggiornamento dall'interno dell'app (ADR-0109): scarica l'installatore della release, ne verifica il SHA-256
//! con `SHA256SUMS` della stessa release e, su richiesta del proprietario, lo applica e riavvia l'app.
//!
//! - Solo HTTPS verso GitHub (`api.github.com`, `github.com`, `*.githubusercontent.com`); i reindirizzamenti sono
//!   seguiti a mano e riconvalidati; il token GitHub, se presente, viaggia solo verso `api.github.com`.
//! - Un solo scaricamento alla volta, massimo 400 MB, file `.part` rinominato solo dopo la verifica.
//! - L'installazione riverifica il file sul disco e parte solo se l'identificativo di versione coincide
//!   con quello scaricato. Windows: setup NSIS silenzioso per l'utente; macOS: copia di `JARVIS.app` dal `.dmg`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

const MAX_BYTES: u64 = 400 * 1024 * 1024;
const MAX_SUMS_BYTES: u64 = 256 * 1024;
const MAX_HOPS: usize = 4;

#[derive(Clone, Debug, Default)]
struct State {
    status: &'static str, // idle | downloading | ready | failed
    version: String,
    asset: String,
    bytes: u64,
    total: u64,
    error: String,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(State {
            status: "idle",
            ..State::default()
        })
    })
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

/// Stato del download, per `GET /api/updates`.
pub fn status() -> Value {
    let s = lock().clone();
    json!({
        "status": s.status, "version": s.version, "asset": s.asset,
        "bytes": s.bytes, "total": s.total,
        "error": if s.error.is_empty() { Value::Null } else { Value::String(s.error) },
        "supported": installer_kind(std::env::consts::OS, std::env::consts::ARCH).is_some(),
    })
}

/// Nome dell'installatore atteso per la piattaforma: `None` dove l'aggiornamento interno non è previsto.
pub fn installer_kind(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("x64-setup.exe"),
        ("macos", "aarch64") => Some("aarch64.dmg"),
        _ => None,
    }
}

/// Asset da scaricare dalla release `latest` (con `assets`), più il file `SHA256SUMS`.
#[derive(Debug)]
pub struct Plan {
    pub version: String,
    pub name: String,
    pub api_url: String,
    pub url: String,
    pub size: u64,
    pub sums_api_url: String,
    pub sums_url: String,
}

pub fn plan(latest: &Value, os: &str, arch: &str) -> Result<Plan, String> {
    let suffix = installer_kind(os, arch)
        .ok_or("L'aggiornamento interno non è disponibile su questo sistema: scarica la release dalla pagina GitHub")?;
    let tag = latest["tag"]
        .as_str()
        .ok_or("nessuna release disponibile")?;
    let version = tag.trim_start_matches('v').to_string();
    let wanted = format!("JARVIS_{version}_{suffix}");
    let assets = latest["assets"]
        .as_array()
        .ok_or("la release non elenca i file: ripeti il controllo")?;
    let find = |name: &str| assets.iter().find(|a| a["name"] == name);
    let asset = find(&wanted).ok_or_else(|| format!("la release non contiene {wanted}"))?;
    let sums =
        find("SHA256SUMS").ok_or("la release non contiene SHA256SUMS: aggiornamento rifiutato")?;
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().to_string();
    Ok(Plan {
        version,
        name: wanted,
        api_url: s(asset, "api_url"),
        url: s(asset, "download_url"),
        size: asset["size"].as_u64().unwrap_or(0),
        sums_api_url: s(sums, "api_url"),
        sums_url: s(sums, "download_url"),
    })
}

/// Righe `<sha256>  <nome>` di `sha256sum`.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (name.to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(hex(ctx.finish().as_ref()))
}

/// Solo HTTPS verso GitHub; nessuna credenziale nell'indirizzo.
pub fn allowed_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    #[cfg(test)]
    if url.scheme() == "http" && host == "127.0.0.1" {
        return true;
    }
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && (host == "api.github.com"
            || host == "github.com"
            || host.ends_with(".githubusercontent.com"))
}

/// Apre l'asset seguendo i reindirizzamenti a mano. Il token va solo ad `api.github.com`.
fn open(api_url: &str, url: &str, token: Option<&str>) -> Result<ureq::Response, String> {
    let agent = crate::net::builder(Duration::from_secs(600)).build();
    let (mut current, mut private) = match token {
        Some(_) if allowed_url(api_url) && api_url.starts_with("https://api.github.com/") => {
            (api_url.to_string(), true)
        }
        _ => (url.to_string(), false),
    };
    for _ in 0..=MAX_HOPS {
        if !allowed_url(&current) {
            return Err("indirizzo di download non ammesso".into());
        }
        let mut req = agent
            .get(&current)
            .set("Accept", "application/octet-stream");
        if private {
            req = req.set(
                "Authorization",
                &format!("Bearer {}", token.unwrap_or_default()),
            );
        }
        match req.call() {
            Ok(r) if (300..400).contains(&r.status()) => {
                let next = r
                    .header("Location")
                    .ok_or("reindirizzamento senza destinazione")?;
                current = url::Url::parse(&current)
                    .and_then(|base| base.join(next))
                    .map_err(|_| "reindirizzamento non valido")?
                    .to_string();
                private = false;
            }
            Ok(r) => return Ok(r),
            Err(ureq::Error::Status(code, r)) if (300..400).contains(&code) => {
                let next = r
                    .header("Location")
                    .ok_or("reindirizzamento senza destinazione")?;
                current = url::Url::parse(&current)
                    .and_then(|base| base.join(next))
                    .map_err(|_| "reindirizzamento non valido")?
                    .to_string();
                private = false;
            }
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("GitHub ha risposto HTTP {code}"))
            }
            Err(ureq::Error::Transport(t)) => {
                return Err(format!(
                    "connessione a GitHub non riuscita: {}",
                    crate::net::classify(&t).hint()
                ))
            }
        }
    }
    Err("troppi reindirizzamenti".into())
}

fn fail(message: impl Into<String>) {
    let mut s = lock();
    s.status = "failed";
    s.error = message.into();
}

pub fn updates_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("updates")
}

/// Avvia il download in un thread. Errore se ne è già in corso uno.
pub fn start_download(data_dir: &Path, plan: Plan, token: Option<String>) -> Result<(), String> {
    {
        let mut s = lock();
        if s.status == "downloading" {
            return Err("un download è già in corso".into());
        }
        *s = State {
            status: "downloading",
            version: plan.version.clone(),
            asset: plan.name.clone(),
            total: plan.size,
            ..State::default()
        };
    }
    let dir = updates_dir(data_dir);
    std::thread::spawn(move || {
        if let Err(message) = download(&dir, &plan, token.as_deref()) {
            fail(message);
        }
    });
    Ok(())
}

fn download(dir: &Path, plan: &Plan, token: Option<&str>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|_| "cartella degli aggiornamenti non creabile")?;
    // Un solo installatore alla volta: i vecchi si eliminano.
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    let mut sums = String::new();
    open(&plan.sums_api_url, &plan.sums_url, token)?
        .into_reader()
        .take(MAX_SUMS_BYTES)
        .read_to_string(&mut sums)
        .map_err(|_| "SHA256SUMS non leggibile")?;
    let expected = parse_sums(&sums)
        .remove(&plan.name)
        .ok_or("SHA256SUMS non contiene l'installatore: aggiornamento rifiutato")?;
    let response = open(&plan.api_url, &plan.url, token)?;
    if let Some(len) = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len > MAX_BYTES {
            return Err("il file supera il limite di 400 MB".into());
        }
        lock().total = len;
    }
    let part = dir.join(format!("{}.part", plan.name));
    let mut file = std::fs::File::create(&part).map_err(|_| "file temporaneo non creabile")?;
    let mut reader = response.into_reader().take(MAX_BYTES + 1);
    let mut buf = vec![0u8; 1 << 16];
    let mut written = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|_| "download interrotto")?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > MAX_BYTES {
            let _ = std::fs::remove_file(&part);
            return Err("il file supera il limite di 400 MB".into());
        }
        file.write_all(&buf[..n])
            .map_err(|_| "scrittura su disco non riuscita")?;
        lock().bytes = written;
    }
    file.sync_all().ok();
    drop(file);
    let actual = sha256_file(&part).map_err(|_| "verifica non riuscita")?;
    if actual != expected {
        let _ = std::fs::remove_file(&part);
        return Err("la verifica SHA-256 è fallita: file scartato".into());
    }
    let ready = dir.join(&plan.name);
    std::fs::rename(&part, &ready).map_err(|_| "file verificato non salvabile")?;
    std::fs::write(
        dir.join("ready.json"),
        json!({"version": plan.version, "asset": plan.name, "sha256": actual}).to_string(),
    )
    .map_err(|_| "registro del download non salvabile")?;
    let mut s = lock();
    s.status = "ready";
    s.bytes = written;
    s.total = written;
    Ok(())
}

/// Installatore pronto e ancora integro: (percorso, versione).
pub fn ready_installer(data_dir: &Path) -> Result<(PathBuf, String), String> {
    let dir = updates_dir(data_dir);
    let meta: Value = std::fs::read(dir.join("ready.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("nessun aggiornamento scaricato")?;
    let name = meta["asset"].as_str().unwrap_or_default();
    if name.is_empty() || name.contains(['/', '\\']) || !name.starts_with("JARVIS_") {
        return Err("registro dell'aggiornamento non valido".into());
    }
    let path = dir.join(name);
    let actual = sha256_file(&path).map_err(|_| "installatore non trovato")?;
    if Some(actual.as_str()) != meta["sha256"].as_str() {
        return Err("l'installatore non corrisponde alla verifica: scaricalo di nuovo".into());
    }
    Ok((
        path,
        meta["version"].as_str().unwrap_or_default().to_string(),
    ))
}

fn safe_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("percorso non valido")?;
    if text.chars().any(|c| {
        matches!(
            c,
            '"' | '%' | '&' | '^' | '`' | '$' | '\n' | '\r' | '\'' | '!' | ';' | '|' | '<' | '>'
        )
    }) {
        return Err(
            "il percorso dell'app contiene caratteri non supportati per l'aggiornamento automatico"
                .into(),
        );
    }
    Ok(text.to_string())
}

/// Radice dell'app installata ricavata dal percorso del Core in esecuzione.
pub fn app_location(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .or_else(|| {
            // Windows: <app>\resources\jarvis-core.exe
            let resources = exe.parent()?;
            (resources.file_name()? == "resources")
                .then(|| resources.parent().map(Path::to_path_buf))?
        })
}

/// Script che attende la chiusura dell'app, installa e riavvia.
pub fn install_script(os: &str, installer: &Path, app: &Path) -> Result<String, String> {
    let installer = safe_path(installer)?;
    let app = safe_path(app)?;
    match os {
        "windows" => Ok(format!(
            "@echo off\r\nping -n 4 127.0.0.1 >nul\r\nstart \"\" /wait \"{installer}\" /S\r\nif exist \"{app}\\jarvis-desktop.exe\" start \"\" \"{app}\\jarvis-desktop.exe\"\r\n"
        )),
        "macos" => Ok(format!(
            "#!/bin/sh\nsleep 3\nmnt=$(mktemp -d /tmp/jarvis-update.XXXXXX) || exit 1\n\
             hdiutil attach -nobrowse -readonly -mountpoint \"$mnt\" \"{installer}\" >/dev/null || exit 1\n\
             if [ -d \"$mnt/JARVIS.app\" ] && rm -rf \"{app}.new\" && ditto \"$mnt/JARVIS.app\" \"{app}.new\"; then\n\
             rm -rf \"{app}.old\"; mv \"{app}\" \"{app}.old\" && mv \"{app}.new\" \"{app}\" && rm -rf \"{app}.old\"\n\
             else rm -rf \"{app}.new\"; fi\n\
             hdiutil detach \"$mnt\" -force >/dev/null 2>&1; rmdir \"$mnt\" 2>/dev/null\n\
             open \"{app}\"\n"
        )),
        _ => Err("aggiornamento interno non disponibile su questo sistema".into()),
    }
}

/// Avvia lo script in un processo staccato. L'app deve chiudersi subito dopo la risposta.
pub fn apply(data_dir: &Path) -> Result<Value, String> {
    let (installer, version) = ready_installer(data_dir)?;
    let exe = std::env::current_exe().map_err(|_| "percorso del Core non disponibile")?;
    let app = app_location(&exe)
        .ok_or("installazione non riconosciuta: aggiorna scaricando la release")?;
    let os = std::env::consts::OS;
    let script = install_script(os, &installer, &app)?;
    let dir = updates_dir(data_dir);
    let path = dir.join(if os == "windows" {
        "apply.cmd"
    } else {
        "apply.sh"
    });
    std::fs::write(&path, script).map_err(|_| "script di aggiornamento non scrivibile")?;
    let mut cmd = if os == "windows" {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/C", "call"]).arg(&path);
        c
    } else {
        let mut c = std::process::Command::new("/bin/sh");
        c.arg(&path);
        c
    };
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        cmd.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|_| "avvio dell'installazione non riuscito")?;
    Ok(json!({"installing": true, "version": version, "restart": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Value {
        json!({"tag": "v9.9.9", "assets": [
            {"name": "JARVIS_9.9.9_x64-setup.exe", "api_url": "https://api.github.com/repos/o/r/releases/assets/1", "download_url": "https://github.com/o/r/releases/download/v9.9.9/JARVIS_9.9.9_x64-setup.exe", "size": 10},
            {"name": "JARVIS_9.9.9_aarch64.dmg", "api_url": "https://api.github.com/repos/o/r/releases/assets/2", "download_url": "https://github.com/o/r/releases/download/v9.9.9/JARVIS_9.9.9_aarch64.dmg", "size": 20},
            {"name": "SHA256SUMS", "api_url": "https://api.github.com/repos/o/r/releases/assets/3", "download_url": "https://github.com/o/r/releases/download/v9.9.9/SHA256SUMS", "size": 1},
        ]})
    }

    #[test]
    fn the_right_installer_is_chosen_for_each_platform() {
        assert_eq!(
            plan(&release(), "windows", "x86_64").unwrap().name,
            "JARVIS_9.9.9_x64-setup.exe"
        );
        assert_eq!(
            plan(&release(), "macos", "aarch64").unwrap().name,
            "JARVIS_9.9.9_aarch64.dmg"
        );
        assert!(plan(&release(), "linux", "x86_64").is_err());
        let mut no_sums = release();
        no_sums["assets"].as_array_mut().unwrap().pop();
        assert!(plan(&no_sums, "windows", "x86_64")
            .unwrap_err()
            .contains("SHA256SUMS"));
    }

    #[test]
    fn checksums_are_parsed_strictly() {
        let h = "a".repeat(64);
        let sums = parse_sums(&format!(
            "{h}  JARVIS_1_x64-setup.exe\n{h} *b.dmg\nxyz  bad\n"
        ));
        assert_eq!(sums.len(), 2);
        assert_eq!(sums["JARVIS_1_x64-setup.exe"], h);
        assert!(sums.contains_key("b.dmg"));
    }

    #[test]
    fn only_https_github_hosts_are_allowed() {
        for ok in [
            "https://api.github.com/repos/o/r/releases/assets/1",
            "https://github.com/o/r/releases/download/v1/x",
            "https://objects.githubusercontent.com/a",
            "https://release-assets.githubusercontent.com/a",
        ] {
            assert!(allowed_url(ok), "{ok}");
        }
        for bad in [
            "http://github.com/x",
            "https://evilgithub.com/x",
            "https://github.com.evil.org/x",
            "https://user:pw@github.com/x",
            "https://example.com/x",
            "file:///etc/passwd",
        ] {
            assert!(!allowed_url(bad), "{bad}");
        }
    }

    #[test]
    fn the_app_location_is_derived_from_the_core_path() {
        assert_eq!(
            app_location(Path::new(
                "/Applications/JARVIS.app/Contents/Resources/jarvis-core"
            )),
            Some(PathBuf::from("/Applications/JARVIS.app"))
        );
        assert_eq!(
            app_location(Path::new(
                "C:/Users/x/AppData/Local/Programs/JARVIS/resources/jarvis-core.exe"
            )),
            Some(PathBuf::from("C:/Users/x/AppData/Local/Programs/JARVIS"))
        );
        assert_eq!(
            app_location(Path::new("/home/u/jarvis/target/debug/jarvis")),
            None
        );
    }

    #[test]
    fn scripts_wait_install_and_restart_and_refuse_unsafe_paths() {
        let win = install_script(
            "windows",
            Path::new("C:/u/up/JARVIS_1_x64-setup.exe"),
            Path::new("C:/u/JARVIS"),
        )
        .unwrap();
        assert!(win.contains("/wait") && win.contains("/S") && win.contains("jarvis-desktop.exe"));
        let mac = install_script(
            "macos",
            Path::new("/d/JARVIS.dmg"),
            Path::new("/Applications/JARVIS.app"),
        )
        .unwrap();
        assert!(mac.contains("hdiutil attach") && mac.contains("ditto") && mac.contains("open "));
        assert!(install_script("windows", Path::new("C:/a&b/x.exe"), Path::new("C:/u")).is_err());
        assert!(install_script(
            "macos",
            Path::new("/a$b/x.dmg"),
            Path::new("/Applications/JARVIS.app")
        )
        .is_err());
        assert!(install_script("linux", Path::new("/x"), Path::new("/y")).is_err());
    }

    #[test]
    fn a_tampered_installer_is_refused_before_installation() {
        let dir = std::env::temp_dir().join(format!("jarvis-up-{}", uuid::Uuid::new_v4().simple()));
        let up = updates_dir(&dir);
        std::fs::create_dir_all(&up).unwrap();
        std::fs::write(up.join("JARVIS_1.0.0_x64-setup.exe"), b"contenuto").unwrap();
        let hash = sha256_file(&up.join("JARVIS_1.0.0_x64-setup.exe")).unwrap();
        std::fs::write(
            up.join("ready.json"),
            json!({"version": "1.0.0", "asset": "JARVIS_1.0.0_x64-setup.exe", "sha256": hash})
                .to_string(),
        )
        .unwrap();
        assert_eq!(ready_installer(&dir).unwrap().1, "1.0.0");
        std::fs::write(up.join("JARVIS_1.0.0_x64-setup.exe"), b"manomesso").unwrap();
        assert!(ready_installer(&dir)
            .unwrap_err()
            .contains("non corrisponde"));
        std::fs::write(
            up.join("ready.json"),
            json!({"asset": "../x", "sha256": "0"}).to_string(),
        )
        .unwrap();
        assert!(ready_installer(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut out = stream;
                match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(
                            out,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = out.write_all(body);
                    }
                    None => {
                        let _ = write!(out, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        base
    }

    fn plan_for(base: &str) -> Plan {
        Plan {
            version: "9.9.9".into(),
            name: "JARVIS_9.9.9_x64-setup.exe".into(),
            api_url: String::new(),
            url: format!("{base}/setup.exe"),
            size: 0,
            sums_api_url: String::new(),
            sums_url: format!("{base}/SHA256SUMS"),
        }
    }

    #[test]
    fn a_verified_download_becomes_ready_and_a_mismatch_is_discarded() {
        let dir = std::env::temp_dir().join(format!("jarvis-dl-{}", uuid::Uuid::new_v4().simple()));
        let body = b"installatore sintetico".to_vec();
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        ctx.update(&body);
        let good = hex(ctx.finish().as_ref());
        let base = serve(vec![
            ("/setup.exe", body.clone()),
            (
                "/SHA256SUMS",
                format!("{good}  JARVIS_9.9.9_x64-setup.exe\n").into_bytes(),
            ),
        ]);
        download(&dir.join("updates"), &plan_for(&base), None).unwrap();
        assert_eq!(ready_installer(&dir).unwrap().1, "9.9.9");

        let base = serve(vec![
            ("/setup.exe", body),
            (
                "/SHA256SUMS",
                format!("{}  JARVIS_9.9.9_x64-setup.exe\n", "0".repeat(64)).into_bytes(),
            ),
        ]);
        let err = download(&dir.join("updates"), &plan_for(&base), None).unwrap_err();
        assert!(err.contains("SHA-256"), "{err}");
        assert!(
            ready_installer(&dir).is_err(),
            "nessun installatore non verificato resta pronto"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
