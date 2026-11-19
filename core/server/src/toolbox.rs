//! Strumenti di uso quotidiano (ADR-0046): calcolo, unità, date, ricerca e gestione dei file nella workspace,
//! dischi e meteo. Funzioni pure o confinate nella workspace; il meteo passa dalla policy Web.

use std::path::Path;

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

// ---------------------------------------------------------------------- math.calc

/// Parser a discesa ricorsiva: niente eval, niente variabili, solo numeri, operatori e funzioni note.
struct Calc<'a> {
    s: &'a [u8],
    i: usize,
}

impl Calc<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn expr(&mut self, depth: u32) -> Result<f64, String> {
        if depth > 64 {
            return Err("espressione troppo annidata".into());
        }
        let mut v = self.term(depth)?;
        while let Some(c @ (b'+' | b'-')) = self.peek() {
            self.i += 1;
            let r = self.term(depth)?;
            v = if c == b'+' { v + r } else { v - r };
        }
        Ok(v)
    }
    fn term(&mut self, depth: u32) -> Result<f64, String> {
        let mut v = self.unary(depth)?;
        while let Some(c @ (b'*' | b'/' | b'%' | b'x' | b':')) = self.peek() {
            self.i += 1;
            let r = self.unary(depth)?;
            v = match c {
                b'*' | b'x' => v * r,
                b'%' => v % r,
                _ => {
                    if r == 0.0 {
                        return Err("divisione per zero".into());
                    }
                    v / r
                }
            };
        }
        Ok(v)
    }
    /// Il segno lega meno della potenza: -3^2 = -(3^2).
    fn unary(&mut self, depth: u32) -> Result<f64, String> {
        if depth > 64 {
            return Err("espressione troppo annidata".into());
        }
        match self.peek() {
            Some(b'-') => {
                self.i += 1;
                Ok(-self.unary(depth + 1)?)
            }
            Some(b'+') => {
                self.i += 1;
                self.unary(depth + 1)
            }
            _ => self.power(depth),
        }
    }
    /// Potenza associativa a destra; l'esponente può avere un segno (2^-1).
    fn power(&mut self, depth: u32) -> Result<f64, String> {
        let base = self.atom(depth)?;
        if self.peek() == Some(b'^') {
            self.i += 1;
            let exp = self.unary(depth + 1)?;
            return Ok(base.powf(exp));
        }
        Ok(base)
    }
    fn atom(&mut self, depth: u32) -> Result<f64, String> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.expr(depth + 1)?;
                if self.peek() != Some(b')') {
                    return Err("parentesi non chiusa".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' || c == b',' => {
                let start = self.i;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_digit() || matches!(self.s[self.i], b'.' | b','))
                {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or_default()
                    .replace(',', ".");
                let mut v: f64 = text
                    .parse()
                    .map_err(|_| format!("numero non valido: {text}"))?;
                // Percentuale: «20%» vale 0,2 quando non è seguita da un operando.
                if self.peek() == Some(b'%') {
                    let save = self.i;
                    self.i += 1;
                    match self.peek() {
                        Some(c) if c.is_ascii_digit() || c == b'(' => self.i = save,
                        _ => v /= 100.0,
                    }
                }
                Ok(v)
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_alphanumeric() {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                match name.as_str() {
                    "pi" => return Ok(std::f64::consts::PI),
                    "e" => return Ok(std::f64::consts::E),
                    _ => {}
                }
                if self.peek() != Some(b'(') {
                    return Err(format!("nome sconosciuto: {name}"));
                }
                let x = self.atom(depth + 1)?;
                Ok(match name.as_str() {
                    "sqrt" | "radq" => x.sqrt(),
                    "sin" => x.sin(),
                    "cos" => x.cos(),
                    "tan" => x.tan(),
                    "log" => x.log10(),
                    "ln" => x.ln(),
                    "abs" => x.abs(),
                    "round" => x.round(),
                    "floor" => x.floor(),
                    "ceil" => x.ceil(),
                    _ => return Err(format!("funzione sconosciuta: {name}")),
                })
            }
            _ => Err("espressione incompleta".into()),
        }
    }
}

/// Numero leggibile: intero se non ha parte decimale, altrimenti arrotondato a 10 cifre decimali.
fn number(v: f64) -> Value {
    let r = (v * 1e10).round() / 1e10;
    if r.fract() == 0.0 && r.abs() < 1e15 {
        json!(r as i64)
    } else {
        json!(r)
    }
}

pub fn calc(expression: &str) -> Result<Value, ToolError> {
    let mut p = Calc {
        s: expression.as_bytes(),
        i: 0,
    };
    let v = p.expr(0).map_err(|m| err("math.invalid", m))?;
    if p.peek().is_some() {
        return Err(err(
            "math.invalid",
            format!("carattere inatteso in posizione {}", p.i + 1),
        ));
    }
    if !v.is_finite() {
        return Err(err("math.invalid", "risultato non finito"));
    }
    Ok(json!({"expression": expression, "result": number(v)}))
}

// ---------------------------------------------------------------------- units.convert

/// Fattore verso l'unità base della grandezza.
fn unit(u: &str) -> Option<(&'static str, f64)> {
    let u = u.trim().to_lowercase();
    Some(match u.as_str() {
        "mm" => ("lunghezza", 0.001),
        "cm" => ("lunghezza", 0.01),
        "m" | "metri" | "metro" => ("lunghezza", 1.0),
        "km" => ("lunghezza", 1000.0),
        "in" | "pollici" | "pollice" => ("lunghezza", 0.0254),
        "ft" | "piedi" | "piede" => ("lunghezza", 0.3048),
        "yd" | "iarde" => ("lunghezza", 0.9144),
        "mi" | "miglia" | "miglio" => ("lunghezza", 1609.344),
        "nmi" => ("lunghezza", 1852.0),
        "mg" => ("massa", 1e-6),
        "g" | "grammi" => ("massa", 0.001),
        "kg" | "chili" => ("massa", 1.0),
        "t" | "tonnellate" => ("massa", 1000.0),
        "oz" | "once" => ("massa", 0.028_349_523_125),
        "lb" | "libbre" => ("massa", 0.453_592_37),
        "ml" => ("volume", 0.001),
        "cl" => ("volume", 0.01),
        "l" | "litri" => ("volume", 1.0),
        "m3" => ("volume", 1000.0),
        "gal" | "galloni" => ("volume", 3.785_411_784),
        "kmh" | "km/h" => ("velocità", 1.0 / 3.6),
        "ms" | "m/s" => ("velocità", 1.0),
        "mph" => ("velocità", 0.447_04),
        "nodi" | "kn" => ("velocità", 0.514_444),
        "m2" => ("area", 1.0),
        "km2" => ("area", 1e6),
        "ha" | "ettari" => ("area", 1e4),
        "acri" | "acre" => ("area", 4_046.856_422_4),
        "b" | "byte" => ("dati", 1.0),
        "kb" => ("dati", 1e3),
        "mb" => ("dati", 1e6),
        "gb" => ("dati", 1e9),
        "tb" => ("dati", 1e12),
        "kib" => ("dati", 1024.0),
        "mib" => ("dati", 1_048_576.0),
        "gib" => ("dati", 1_073_741_824.0),
        "s" | "secondi" => ("tempo", 1.0),
        "min" | "minuti" => ("tempo", 60.0),
        "h" | "ore" => ("tempo", 3600.0),
        "giorni" | "d" => ("tempo", 86400.0),
        "settimane" => ("tempo", 604_800.0),
        "c" | "°c" | "celsius" => ("temperatura", 0.0),
        "f" | "°f" | "fahrenheit" => ("temperatura", 0.0),
        "k" | "kelvin" => ("temperatura", 0.0),
        _ => return None,
    })
}

pub fn convert(value: f64, from: &str, to: &str) -> Result<Value, ToolError> {
    let (qf, ff) =
        unit(from).ok_or_else(|| err("units.unknown", format!("unità sconosciuta: {from}")))?;
    let (qt, ft) =
        unit(to).ok_or_else(|| err("units.unknown", format!("unità sconosciuta: {to}")))?;
    if qf != qt {
        return Err(err(
            "units.incompatible",
            format!("{from} ({qf}) e {to} ({qt}) non sono confrontabili"),
        ));
    }
    let result = if qf == "temperatura" {
        let c = match from.trim().to_lowercase().trim_start_matches('°') {
            "f" | "fahrenheit" => (value - 32.0) * 5.0 / 9.0,
            "k" | "kelvin" => value - 273.15,
            _ => value,
        };
        match to.trim().to_lowercase().trim_start_matches('°') {
            "f" | "fahrenheit" => c * 9.0 / 5.0 + 32.0,
            "k" | "kelvin" => c + 273.15,
            _ => c,
        }
    } else {
        value * ff / ft
    };
    Ok(
        json!({"value": value, "from": from, "to": to, "quantity": qf, "result": (result * 1e8).round() / 1e8}),
    )
}

// ---------------------------------------------------------------------- date.calc

const DAYS: [&str; 7] = [
    "lunedì",
    "martedì",
    "mercoledì",
    "giovedì",
    "venerdì",
    "sabato",
    "domenica",
];

pub fn date_calc(args: &Value) -> Result<Value, ToolError> {
    use chrono::{Datelike, NaiveDate};
    let parse = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| err("date.invalid", format!("data non valida (AAAA-MM-GG): {s}")))
    };
    let date = parse(args["date"].as_str().unwrap_or_default())?;
    let mut out = json!({"date": date.to_string(), "weekday": DAYS[date.weekday().num_days_from_monday() as usize]});
    if let Some(until) = args["until"].as_str() {
        let other = parse(until)?;
        out["until"] = other.to_string().into();
        out["days_between"] = (other - date).num_days().into();
    }
    let mut result = date;
    if let Some(m) = args["add_months"].as_i64() {
        result = if m >= 0 {
            result.checked_add_months(chrono::Months::new(m as u32))
        } else {
            result.checked_sub_months(chrono::Months::new((-m) as u32))
        }
        .ok_or_else(|| err("date.invalid", "data fuori intervallo"))?;
    }
    if let Some(d) = args["add_days"].as_i64() {
        result = result
            .checked_add_signed(chrono::Duration::days(d))
            .ok_or_else(|| err("date.invalid", "data fuori intervallo"))?;
    }
    if result != date {
        out["result"] = result.to_string().into();
        out["result_weekday"] = DAYS[result.weekday().num_days_from_monday() as usize].into();
    }
    Ok(out)
}

// ---------------------------------------------------------------------- file nella workspace

const MAX_SCAN: usize = 5000;

pub fn fs_search(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let name = args["name"].as_str().unwrap_or_default().to_lowercase();
    let text = args["text"].as_str().unwrap_or_default().to_lowercase();
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize;
    let root = workspace
        .canonicalize()
        .map_err(|e| err("fs.unavailable", e.to_string()))?;
    let mut stack = vec![root.clone()];
    let (mut found, mut scanned) = (Vec::new(), 0usize);
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            scanned += 1;
            if scanned > MAX_SCAN || found.len() >= limit {
                break;
            }
            let Ok(meta) = e.metadata() else { continue };
            // Niente collegamenti simbolici: la ricerca non esce dalla workspace.
            if meta.file_type().is_symlink() {
                continue;
            }
            let path = e.path();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if !name.is_empty() && !rel.to_lowercase().contains(&name) {
                continue;
            }
            let mut hit = json!({"path": rel, "bytes": meta.len()});
            if !text.is_empty() {
                if meta.len() > 1_000_000 {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some(pos) = content.to_lowercase().find(&text) else {
                    continue;
                };
                let line = content[..pos].matches('\n').count() + 1;
                let snippet: String = content[pos..]
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(160)
                    .collect();
                hit["line"] = line.into();
                hit["snippet"] = snippet.into();
            }
            found.push(hit);
        }
    }
    Ok(json!({"results": found, "scanned": scanned.min(MAX_SCAN), "truncated": scanned > MAX_SCAN}))
}

pub fn fs_mkdir(workspace: &Path, rel: &str) -> Result<Value, ToolError> {
    let path = resolve(workspace, rel, false)?;
    if path.exists() {
        return Err(err("fs.exists", format!("esiste già: {rel}")));
    }
    std::fs::create_dir_all(&path).map_err(|e| err("fs.unavailable", e.to_string()))?;
    Ok(json!({"path": rel, "created": true}))
}

pub fn fs_move(workspace: &Path, from: &str, to: &str) -> Result<Value, ToolError> {
    let src = resolve(workspace, from, true)?;
    let dst = resolve(workspace, to, false)?;
    if dst.exists() {
        return Err(err(
            "fs.exists",
            format!("la destinazione esiste già: {to}"),
        ));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| err("fs.unavailable", e.to_string()))?;
    }
    std::fs::rename(&src, &dst).map_err(|e| err("fs.unavailable", e.to_string()))?;
    Ok(json!({"from": from, "to": to}))
}

pub fn fs_delete(workspace: &Path, rel: &str) -> Result<Value, ToolError> {
    let path = resolve(workspace, rel, true)?;
    let root = workspace
        .canonicalize()
        .map_err(|e| err("fs.unavailable", e.to_string()))?;
    if path == root {
        return Err(err(
            "fs.outside_workspace",
            "non si elimina la workspace stessa",
        ));
    }
    if path.is_dir() {
        std::fs::remove_dir(&path)
            .map_err(|_| err("fs.not_empty", format!("la cartella non è vuota: {rel}")))?;
    } else {
        std::fs::remove_file(&path).map_err(|e| err("fs.unavailable", e.to_string()))?;
    }
    Ok(json!({"path": rel, "deleted": true}))
}

// ---------------------------------------------------------------------- dischi

pub fn disks() -> Result<Value, ToolError> {
    let gb = |b: f64| (b / 1e9 * 10.0).round() / 10.0;
    let mut out = Vec::new();
    if cfg!(windows) {
        let o = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,Size,FreeSpace | ConvertTo-Json"])
            .output()
            .map_err(|e| err("system.unavailable", e.to_string()))?;
        let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
        let list = if v.is_array() {
            v.as_array().cloned().unwrap_or_default()
        } else {
            vec![v]
        };
        for d in list.iter().filter(|d| d["Size"].is_number()) {
            let (size, free) = (
                d["Size"].as_f64().unwrap_or(0.0),
                d["FreeSpace"].as_f64().unwrap_or(0.0),
            );
            out.push(json!({"mount": d["DeviceID"], "total_gb": gb(size), "free_gb": gb(free), "used_percent": ((size - free) / size * 100.0).round()}));
        }
    } else {
        let o = std::process::Command::new("df")
            .args(["-kP"])
            .output()
            .map_err(|e| err("system.unavailable", e.to_string()))?;
        for line in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 6
                || !f[0].starts_with('/')
                || f[5].starts_with("/snap")
                || f[5].starts_with("/System/Volumes/") && f[5] != "/System/Volumes/Data"
            {
                continue;
            }
            let (total, avail) = (
                f[1].parse::<f64>().unwrap_or(0.0) * 1024.0,
                f[3].parse::<f64>().unwrap_or(0.0) * 1024.0,
            );
            if total <= 0.0 {
                continue;
            }
            out.push(json!({"mount": f[5], "total_gb": gb(total), "free_gb": gb(avail), "used_percent": f[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0)}));
        }
    }
    Ok(json!({"disks": out}))
}

// ---------------------------------------------------------------------- meteo (Open-Meteo)

const WEATHER_HOSTS: [&str; 2] = ["geocoding-api.open-meteo.com", "api.open-meteo.com"];

fn weather_text(code: i64) -> &'static str {
    match code {
        0 => "sereno",
        1 | 2 => "poco nuvoloso",
        3 => "coperto",
        45 | 48 => "nebbia",
        51..=57 => "pioviggine",
        61..=67 => "pioggia",
        71..=77 => "neve",
        80..=82 => "rovesci",
        85 | 86 => "rovesci di neve",
        95..=99 => "temporale",
        _ => "variabile",
    }
}

fn get_json(web: &crate::web::WebConfig, url: &str) -> Result<Value, ToolError> {
    let parsed = url::Url::parse(url).map_err(|e| err("web.invalid_url", e.to_string()))?;
    // Solo gli host di Open-Meteo (o quello di `JARVIS_WEATHER_URL`, per i test e i mirror del proprietario).
    let mut hosts: Vec<String> = WEATHER_HOSTS.iter().map(|h| h.to_string()).collect();
    if std::env::var("JARVIS_WEATHER_URL").is_ok() {
        hosts.extend(parsed.host_str().map(str::to_string));
    }
    let (status, _, body) = crate::web::request(web, "GET", &parsed, &[], None, &hosts)?;
    if status >= 400 {
        return Err(err(
            "web.unreachable",
            format!("Open-Meteo ha risposto {status}"),
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|_| err("web.unreachable", "risposta di Open-Meteo non valida"))
}

pub fn weather(web: &crate::web::WebConfig, place: &str, days: u64) -> Result<Value, ToolError> {
    if !web.enabled {
        return Err(err(
            "policy.web_disabled",
            crate::policy::explain("policy.web_disabled"),
        ));
    }
    let base = std::env::var("JARVIS_WEATHER_URL").ok();
    let geo_url = format!(
        "{}/v1/search?count=1&language=it&format=json&name={}",
        base.clone()
            .unwrap_or_else(|| "https://geocoding-api.open-meteo.com".into()),
        urlencode(place)
    );
    let geo = get_json(web, &geo_url)?;
    let loc = geo["results"][0].clone();
    let (Some(lat), Some(lon)) = (loc["latitude"].as_f64(), loc["longitude"].as_f64()) else {
        return Err(err(
            "weather.not_found",
            format!("località non trovata: {place}"),
        ));
    };
    let fc_url = format!(
        "{}/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days={}",
        base.unwrap_or_else(|| "https://api.open-meteo.com".into()),
        days.clamp(1, 7)
    );
    let fc = get_json(web, &fc_url)?;
    let c = &fc["current"];
    let d = &fc["daily"];
    let daily: Vec<Value> = d["time"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "date": t, "sky": weather_text(d["weather_code"][i].as_i64().unwrap_or(-1)),
                "min_c": d["temperature_2m_min"][i], "max_c": d["temperature_2m_max"][i],
                "rain_probability": d["precipitation_probability_max"][i],
            })
        })
        .collect();
    Ok(json!({
        "place": loc["name"], "region": loc["admin1"], "country": loc["country"],
        "current": {"temperature_c": c["temperature_2m"], "feels_like_c": c["apparent_temperature"], "humidity": c["relative_humidity_2m"],
                    "wind_kmh": c["wind_speed_10m"], "sky": weather_text(c["weather_code"].as_i64().unwrap_or(-1))},
        "daily": daily, "source": "Open-Meteo", "retrieved_at": crate::now(),
    }))
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---------------------------------------------------------------------- note e cose da fare

use crate::store::Store;

fn db(e: rusqlite::Error) -> ToolError {
    err("notes.unavailable", e.to_string())
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

pub fn notes_add(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let id = short_id();
    let tags: Vec<String> = args["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str().map(|s| s.trim().to_lowercase()))
        .collect();
    let now = crate::now();
    store
        .conn
        .execute(
            "INSERT INTO notes (id, title, text, tags, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            rusqlite::params![id, args["title"].as_str().unwrap_or(""), args["text"].as_str().unwrap_or(""), json!(tags).to_string(), now],
        )
        .map_err(db)?;
    Ok(json!({"id": id, "title": args["title"], "tags": tags}))
}

fn note_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let tags: String = r.get(3)?;
    Ok(
        json!({"id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "text": r.get::<_, String>(2)?,
              "tags": serde_json::from_str::<Value>(&tags).unwrap_or(json!([])), "created_at": r.get::<_, String>(4)?}),
    )
}

pub fn notes_list(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 50) as i64;
    let tag = args["tag"]
        .as_str()
        .map(|t| format!("%\"{}\"%", t.trim().to_lowercase()));
    let mut stmt = store
        .conn
        .prepare("SELECT id, title, text, tags, created_at FROM notes WHERE (?1 IS NULL OR tags LIKE ?1) ORDER BY created_at DESC LIMIT ?2")
        .map_err(db)?;
    let rows = stmt
        .query_map(rusqlite::params![tag, limit], note_row)
        .map_err(db)?;
    Ok(json!({"notes": rows.collect::<Result<Vec<_>, _>>().map_err(db)?}))
}

pub fn notes_search(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 50) as i64;
    let words: Vec<String> = args["query"]
        .as_str()
        .unwrap_or_default()
        .split_whitespace()
        .map(|w| format!("%{}%", w.to_lowercase()))
        .take(6)
        .collect();
    let mut stmt = store
        .conn
        .prepare("SELECT id, title, text, tags, created_at FROM notes ORDER BY created_at DESC")
        .map_err(db)?;
    let rows = stmt.query_map([], note_row).map_err(db)?;
    let mut hits = Vec::new();
    for row in rows {
        let n = row.map_err(db)?;
        let hay = format!("{} {} {}", n["title"], n["text"], n["tags"]).to_lowercase();
        if words.iter().all(|w| hay.contains(w.trim_matches('%'))) {
            hits.push(n);
            if hits.len() as i64 >= limit {
                break;
            }
        }
    }
    Ok(json!({"notes": hits}))
}

pub fn notes_delete(store: &Store, id: &str) -> Result<Value, ToolError> {
    let n = store
        .conn
        .execute("DELETE FROM notes WHERE id = ?1", [id])
        .map_err(db)?;
    if n == 0 {
        return Err(err("notes.not_found", format!("nessuna nota con id {id}")));
    }
    Ok(json!({"id": id, "deleted": true}))
}

/// Scadenza «AAAA-MM-GG» o «AAAA-MM-GG HH:MM» in ora locale -> RFC 3339 UTC.
fn due_utc(due: &str) -> Result<String, ToolError> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(&due.replace('T', " "), "%Y-%m-%d %H:%M")
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(9, 0, 0).unwrap_or_default())
        })
        .map_err(|_| err("todo.invalid", format!("scadenza non valida: {due}")))?;
    let local = chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .ok_or_else(|| err("todo.invalid", "ora inesistente nel fuso locale"))?;
    Ok(local
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub fn todo_add(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let id = short_id();
    let due = args["due"].as_str().map(due_utc).transpose()?;
    let priority = args["priority"].as_str().unwrap_or("normale");
    store
        .conn
        .execute(
            "INSERT INTO todos (id, text, due_at, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, args["text"].as_str().unwrap_or(""), due, priority, crate::now()],
        )
        .map_err(|e| err("todo.unavailable", e.to_string()))?;
    Ok(json!({"id": id, "text": args["text"], "due_at": due, "priority": priority}))
}

pub fn todo_list(store: &Store, args: &Value) -> Result<Value, ToolError> {
    let filter = match args["status"].as_str().unwrap_or("aperte") {
        "completate" => "done_at IS NOT NULL",
        "tutte" => "1 = 1",
        _ => "done_at IS NULL",
    };
    let mut stmt = store
        .conn
        .prepare(&format!(
            "SELECT id, text, due_at, priority, done_at FROM todos WHERE {filter} ORDER BY due_at IS NULL, due_at, created_at LIMIT 100"
        ))
        .map_err(|e| err("todo.unavailable", e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "text": r.get::<_, String>(1)?, "due_at": r.get::<_, Option<String>>(2)?,
                      "priority": r.get::<_, String>(3)?, "done_at": r.get::<_, Option<String>>(4)?}))
        })
        .map_err(|e| err("todo.unavailable", e.to_string()))?;
    Ok(
        json!({"todos": rows.collect::<Result<Vec<_>, _>>().map_err(|e| err("todo.unavailable", e.to_string()))?}),
    )
}

pub fn todo_done(store: &Store, id: &str) -> Result<Value, ToolError> {
    let n = store
        .conn
        .execute(
            "UPDATE todos SET done_at = ?2 WHERE id = ?1 AND done_at IS NULL",
            rusqlite::params![id, crate::now()],
        )
        .map_err(|e| err("todo.unavailable", e.to_string()))?;
    if n == 0 {
        return Err(err(
            "todo.not_found",
            format!("nessuna attività aperta con id {id}"),
        ));
    }
    Ok(json!({"id": id, "done": true}))
}

/// Attività scadute e non ancora notificate; le segna come notificate.
pub fn todos_due(store: &Store, now: &str) -> Vec<Value> {
    let Ok(mut stmt) = store.conn.prepare(
        "SELECT id, text, due_at, priority FROM todos WHERE done_at IS NULL AND notified_at IS NULL AND due_at IS NOT NULL AND due_at <= ?1",
    ) else {
        return vec![];
    };
    let due: Vec<Value> = stmt
        .query_map([now], |r| Ok(json!({"id": r.get::<_, String>(0)?, "text": r.get::<_, String>(1)?, "due_at": r.get::<_, String>(2)?, "priority": r.get::<_, String>(3)?})))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for t in &due {
        let _ = store.conn.execute(
            "UPDATE todos SET notified_at = ?2 WHERE id = ?1",
            rusqlite::params![t["id"].as_str(), now],
        );
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculator_handles_precedence_functions_and_errors() {
        let v = |e: &str| calc(e).unwrap()["result"].as_f64().unwrap();
        assert_eq!(v("2 + 3 * 4"), 14.0);
        assert_eq!(v("(2 + 3) * 4"), 20.0);
        assert_eq!(v("2 ^ 3 ^ 2"), 512.0);
        assert_eq!(v("-3^2"), -9.0);
        assert_eq!(v("2^-1"), 0.5);
        assert_eq!(v("sqrt(16) + abs(-2)"), 6.0);
        assert_eq!(v("1,5 * 2"), 3.0);
        assert_eq!(v("200 * 15%"), 30.0);
        assert_eq!(v("10 % 3"), 1.0);
        assert!((v("sin(pi/2)") - 1.0).abs() < 1e-9);
        assert_eq!(calc("1/0").unwrap_err().code, "math.invalid");
        assert!(calc("2 +").is_err() && calc("rm(1)").is_err() && calc("(1").is_err());
    }

    #[test]
    fn units_convert_within_the_same_quantity() {
        let r = |v: f64, a: &str, b: &str| convert(v, a, b).unwrap()["result"].as_f64().unwrap();
        assert_eq!(r(1.0, "km", "m"), 1000.0);
        assert!((r(100.0, "C", "F") - 212.0).abs() < 1e-9);
        assert!((r(0.0, "K", "C") + 273.15).abs() < 1e-9);
        assert!((r(1.0, "mi", "km") - 1.609344).abs() < 1e-9);
        assert_eq!(r(1.0, "GiB", "MiB"), 1024.0);
        assert_eq!(
            convert(1.0, "kg", "m").unwrap_err().code,
            "units.incompatible"
        );
        assert_eq!(
            convert(1.0, "parsec", "m").unwrap_err().code,
            "units.unknown"
        );
    }

    #[test]
    fn dates_add_and_subtract() {
        let r = date_calc(&json!({"date": "2026-11-19", "until": "2026-12-25"})).unwrap();
        assert_eq!(r["days_between"], 36);
        assert_eq!(r["weekday"], "giovedì");
        let r = date_calc(&json!({"date": "2026-01-31", "add_months": 1})).unwrap();
        assert_eq!(r["result"], "2026-02-28");
        let r = date_calc(&json!({"date": "2026-11-19", "add_days": -19})).unwrap();
        assert_eq!(r["result"], "2026-10-31");
        assert!(date_calc(&json!({"date": "19/11/2026"})).is_err());
    }

    #[test]
    fn files_are_searched_moved_and_deleted_inside_the_workspace() {
        let ws = std::env::temp_dir().join(format!("jarvis-ws-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(ws.join("note")).unwrap();
        std::fs::write(ws.join("note/spesa.txt"), "latte\nuova\npane").unwrap();
        std::fs::write(ws.join("diario.md"), "oggi bel tempo").unwrap();
        let r = fs_search(&ws, &json!({"text": "UOVA"})).unwrap();
        assert_eq!(r["results"][0]["path"], "note/spesa.txt");
        assert_eq!(r["results"][0]["line"], 2);
        let r = fs_search(&ws, &json!({"name": "diario"})).unwrap();
        assert_eq!(r["results"].as_array().unwrap().len(), 1);
        fs_mkdir(&ws, "archivio").unwrap();
        assert_eq!(fs_mkdir(&ws, "archivio").unwrap_err().code, "fs.exists");
        fs_move(&ws, "diario.md", "archivio/diario.md").unwrap();
        assert!(ws.join("archivio/diario.md").exists());
        assert_eq!(fs_delete(&ws, "archivio").unwrap_err().code, "fs.not_empty");
        fs_delete(&ws, "archivio/diario.md").unwrap();
        fs_delete(&ws, "archivio").unwrap();
        assert_eq!(
            fs_delete(&ws, "../fuori").unwrap_err().code,
            "fs.outside_workspace"
        );
        assert!(
            fs_delete(&ws, ".").is_err(),
            "la workspace stessa non si elimina"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn disks_are_reported() {
        let d = disks().unwrap();
        assert!(!d["disks"].as_array().unwrap().is_empty(), "{d}");
    }
}
