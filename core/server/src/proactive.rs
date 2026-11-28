//! Iniziative proattive (ADR-0097): JARVIS si fa avanti da solo, senza essere interpellato, con un
//! briefing quotidiano, avvisi sullo spazio disco, conferme dimenticate, automazioni che falliscono e cose da
//! fare lasciate indietro. Tutto è deterministico e locale, nessun dato lascia il computer. Questo modulo ha le
//! impostazioni e la logica pura; la valutazione periodica sta in `orchestrator/initiatives.rs`.

use std::path::Path;

use chrono::{Datelike, NaiveTime, Timelike};
use serde_json::{json, Value};

/// Minuti dopo i quali una conferma ferma viene ricordata.
pub const CONFIRMATION_NUDGE_MINUTES: i64 = 2;
/// Occupazione del disco oltre la quale JARVIS avvisa.
pub const DISK_ALERT_PERCENT: f64 = 90.0;
/// Il briefing si manda solo entro queste ore dall'orario scelto: più tardi non è più un briefing.
pub const BRIEFING_WINDOW_HOURS: u32 = 6;
/// Cose da fare aperte da più di questi giorni, e quante ne servono per avvisare.
pub const STALE_DAYS: i64 = 7;
pub const STALE_MIN: u64 = 3;
/// Automazioni: esecuzioni consecutive fallite oltre le quali si segnala.
pub const WORKFLOW_FAILURES: usize = 3;

const FILE: &str = "proactive.json";

pub fn defaults() -> Value {
    json!({
        "enabled": true,
        "quiet_from": "22:00", "quiet_to": "07:00",
        "briefing": true, "briefing_time": "08:00",
        "weekly_briefing": true, "weekly_day": 0, "weekly_time": "08:30",
        "disk": true, "confirmations": true, "workflows": true, "stale_todos": true, "sandbox": true,
    })
}

/// Impostazioni correnti: i valori salvati sovrascrivono i predefiniti; un file illeggibile non blocca.
pub fn settings(dir: &Path) -> Value {
    let mut out = defaults();
    if let Some(saved) = std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    {
        for (k, v) in saved.as_object().into_iter().flatten() {
            if out.get(k).is_some_and(|d| same_kind(d, v)) {
                out[k] = v.clone();
            }
        }
    }
    out
}

fn same_kind(a: &Value, b: &Value) -> bool {
    (a.is_boolean() && b.is_boolean())
        || (a.is_string() && b.is_string())
        || (a.is_u64() && b.is_u64())
}

pub fn parse_time(text: &str) -> Option<NaiveTime> {
    let (h, m) = text.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)
}

/// Applica una modifica parziale dopo averla validata e la salva.
pub fn update(dir: &Path, patch: &Value) -> Result<Value, String> {
    let Some(map) = patch.as_object() else {
        return Err("formato non valido".into());
    };
    let known = defaults();
    let mut current = settings(dir);
    for (k, v) in map {
        let Some(default) = known.get(k) else {
            return Err(format!("impostazione sconosciuta: {k}"));
        };
        if !same_kind(default, v) {
            return Err(format!("valore non valido per {k}"));
        }
        if k.ends_with("_from") || k.ends_with("_to") || k.ends_with("_time") {
            let text = v.as_str().unwrap_or_default();
            if parse_time(text).is_none() {
                return Err(format!("{k}: usa il formato HH:MM"));
            }
        }
        if k == "weekly_day" && v.as_u64().is_none_or(|day| day > 6) {
            return Err("weekly_day: da 0 (lunedì) a 6 (domenica)".into());
        }
        current[k] = v.clone();
    }
    crate::backup::atomic_write(&dir.join(FILE), current.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(current)
}

/// Ore di silenzio: vero se `t` cade fra `from` e `to`, anche a cavallo della mezzanotte. Con
/// `from == to` non c'è silenzio.
pub fn in_quiet(t: NaiveTime, from: NaiveTime, to: NaiveTime) -> bool {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => t >= from && t < to,
        std::cmp::Ordering::Greater => t >= from || t < to,
    }
}

/// Vero se è il momento del briefing: dall'orario scelto per `BRIEFING_WINDOW_HOURS` ore.
pub fn briefing_window(t: NaiveTime, at: NaiveTime) -> bool {
    let start = at.num_seconds_from_midnight();
    let now = t.num_seconds_from_midnight();
    now >= start && now < start + BRIEFING_WINDOW_HOURS * 3600
}

/// Dati del briefing.
pub struct Facts {
    pub weekday: u32,
    pub day: u32,
    pub month: u32,
    pub hour: u32,
    pub open_todos: u64,
    pub due_today: u64,
    pub overdue: u64,
    pub top: Option<String>,
    pub confirmations: usize,
    pub unread: u64,
    pub missions: u64,
    pub failing: Vec<String>,
}

impl Facts {
    pub fn from_date(date: impl Datelike, hour: u32) -> Self {
        Self {
            weekday: date.weekday().num_days_from_monday(),
            day: date.day(),
            month: date.month(),
            hour,
            open_todos: 0,
            due_today: 0,
            overdue: 0,
            top: None,
            confirmations: 0,
            unread: 0,
            missions: 0,
            failing: Vec::new(),
        }
    }
}

const DAYS: [&str; 7] = [
    "lunedì",
    "martedì",
    "mercoledì",
    "giovedì",
    "venerdì",
    "sabato",
    "domenica",
];
const MONTHS: [&str; 12] = [
    "gennaio",
    "febbraio",
    "marzo",
    "aprile",
    "maggio",
    "giugno",
    "luglio",
    "agosto",
    "settembre",
    "ottobre",
    "novembre",
    "dicembre",
];

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Titolo e testo del briefing.
pub fn compose_briefing(f: &Facts) -> (String, String) {
    let greeting = match f.hour {
        0..=11 => "Buongiorno",
        12..=17 => "Buon pomeriggio",
        _ => "Buonasera",
    };
    let title = format!(
        "{greeting}: {} {} {}",
        DAYS[f.weekday as usize % 7],
        f.day,
        MONTHS[(f.month as usize).saturating_sub(1) % 12]
    );
    let mut lines = Vec::new();
    if f.open_todos == 0 {
        lines.push("• Nessuna cosa da fare aperta.".to_string());
    } else {
        let mut todo = format!("• Da fare: {} aperte", f.open_todos);
        if f.due_today > 0 {
            todo.push_str(&format!(", {} in scadenza oggi", f.due_today));
        }
        if f.overdue > 0 {
            todo.push_str(&format!(", {} in ritardo", f.overdue));
        }
        lines.push(todo + ".");
    }
    if f.confirmations > 0 {
        lines.push(format!(
            "• {} in attesa della tua conferma.",
            plural(f.confirmations as u64, "azione", "azioni")
        ));
    }
    if f.unread > 0 {
        lines.push(format!(
            "• {} da leggere.",
            plural(f.unread, "notifica", "notifiche")
        ));
    }
    if f.missions > 0 {
        lines.push(format!(
            "• {} in corso.",
            plural(f.missions, "missione", "missioni")
        ));
    }
    if !f.failing.is_empty() {
        lines.push(format!(
            "• Automazioni con errori recenti: {}.",
            f.failing
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(top) = &f.top {
        lines.push(format!("Priorità consigliata: {top}"));
    }
    (title, lines.join("\n"))
}

/// Dati del briefing settimanale della direzione (ADR-0142): la settimana trascorsa e quella che inizia.
#[derive(Default)]
pub struct WeekFacts {
    pub week: u32,
    pub done: u64,
    pub opened: u64,
    pub open_todos: u64,
    pub overdue: u64,
    pub upcoming: Vec<String>,
    pub missions_active: u64,
    pub missions_completed: u64,
    pub memories: u64,
    pub runs: u64,
    pub runs_failed: u64,
    pub confirmations: usize,
    pub unread: u64,
}

/// Titolo e testo del briefing settimanale: cosa è stato fatto, cosa pesa e cosa viene dopo.
pub fn compose_weekly(f: &WeekFacts) -> (String, String) {
    let title = format!(
        "Briefing settimanale della direzione · settimana {}",
        f.week
    );
    let mut past = Vec::new();
    past.push(format!(
        "• Cose da fare: {} completate, {} nuove, {} ancora aperte.",
        f.done, f.opened, f.open_todos
    ));
    if f.missions_completed > 0 || f.missions_active > 0 {
        past.push(format!(
            "• Missioni: {} concluse, {} in corso.",
            f.missions_completed, f.missions_active
        ));
    }
    if f.runs > 0 {
        let mut line = format!(
            "• Automazioni: {}",
            plural(f.runs, "esecuzione", "esecuzioni")
        );
        if f.runs_failed > 0 {
            line.push_str(&format!(", di cui {} non riuscite", f.runs_failed));
        }
        past.push(line + ".");
    }
    if f.memories > 0 {
        past.push(format!(
            "• Memoria: {} aggiunti.",
            plural(f.memories, "ricordo", "ricordi")
        ));
    }
    let mut attention = Vec::new();
    if f.overdue > 0 {
        attention.push(format!(
            "• {} in ritardo: vanno chiuse o ripianificate.",
            plural(f.overdue, "cosa da fare", "cose da fare")
        ));
    }
    if f.confirmations > 0 {
        attention.push(format!(
            "• {} in attesa della tua conferma.",
            plural(f.confirmations as u64, "azione", "azioni")
        ));
    }
    if f.runs_failed > 0 {
        attention.push("• Controlla le automazioni non riuscite: un errore ripetuto di solito nasconde un problema di processo.".into());
    }
    if f.unread > 0 {
        attention.push(format!(
            "• {} da leggere.",
            plural(f.unread, "notifica", "notifiche")
        ));
    }
    let mut lines = vec!["Settimana trascorsa".to_string()];
    lines.extend(past);
    if !attention.is_empty() {
        lines.push(String::new());
        lines.push("Richiede la tua attenzione".into());
        lines.extend(attention);
    }
    lines.push(String::new());
    lines.push("Settimana in arrivo".into());
    if f.upcoming.is_empty() {
        lines.push(
            "• Nessuna scadenza nei prossimi sette giorni: è il momento di fissare le priorità."
                .into(),
        );
    } else {
        for item in f.upcoming.iter().take(5) {
            lines.push(format!("• {item}"));
        }
    }
    lines.push(String::new());
    lines.push("Chiedimi «priorità RICE», «runway» o «margine» per trasformare questi numeri in decisioni.".into());
    (title, lines.join("\n"))
}

/// Una sessione del terminale sandbox oltre questa dimensione sta per essere rifiutata (limite 256 MiB).
pub const SANDBOX_ALERT_BYTES: u64 = 200 * 1024 * 1024;

/// Sessioni sandbox quasi piene: (sessione, MiB usati).
pub fn sandbox_alerts(sizes: &[(String, u64)]) -> Vec<(String, u64)> {
    sizes
        .iter()
        .filter(|(_, bytes)| *bytes >= SANDBOX_ALERT_BYTES)
        .map(|(name, bytes)| (name.clone(), bytes / 1024 / 1024))
        .collect()
}

/// Mount con spazio quasi esaurito: (mount, GB liberi, percentuale usata).
pub fn disk_alerts(disks: &Value) -> Vec<(String, f64, f64)> {
    disks["disks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["total_gb"].as_f64().unwrap_or(0.0) >= 1.0)
        .filter(|d| d["used_percent"].as_f64().unwrap_or(0.0) >= DISK_ALERT_PERCENT)
        .map(|d| {
            (
                d["mount"].as_str().unwrap_or("?").to_string(),
                d["free_gb"].as_f64().unwrap_or(0.0),
                d["used_percent"].as_f64().unwrap_or(0.0),
            )
        })
        .collect()
}

/// Settimana ISO come chiave di deduplica («2026-W47»).
pub fn week_key(date: impl Datelike) -> String {
    let w = date.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn quiet_hours_wrap_around_midnight() {
        let (from, to) = (t(22, 0), t(7, 0));
        assert!(in_quiet(t(23, 30), from, to) && in_quiet(t(3, 0), from, to));
        assert!(
            !in_quiet(t(7, 0), from, to)
                && !in_quiet(t(12, 0), from, to)
                && !in_quiet(t(21, 59), from, to)
        );
        assert!(in_quiet(t(13, 0), t(12, 0), t(14, 0)));
        assert!(
            !in_quiet(t(3, 0), t(8, 0), t(8, 0)),
            "da == a: nessun silenzio"
        );
    }

    #[test]
    fn the_briefing_is_sent_only_in_its_window() {
        assert!(!briefing_window(t(7, 59), t(8, 0)));
        assert!(briefing_window(t(8, 0), t(8, 0)) && briefing_window(t(13, 59), t(8, 0)));
        assert!(!briefing_window(t(14, 0), t(8, 0)));
    }

    #[test]
    fn settings_merge_validate_and_persist() {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-proactive-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(settings(&dir), defaults());
        let saved = update(&dir, &json!({"briefing_time": "07:30", "disk": false})).unwrap();
        assert_eq!(
            (saved["briefing_time"].as_str(), saved["disk"].as_bool()),
            (Some("07:30"), Some(false))
        );
        assert_eq!(settings(&dir)["briefing_time"], "07:30");
        assert!(update(&dir, &json!({"briefing_time": "8:00"})).is_err());
        assert!(update(&dir, &json!({"briefing_time": "25:00"})).is_err());
        assert!(update(&dir, &json!({"disk": "sì"})).is_err());
        assert!(update(&dir, &json!({"inventata": true})).is_err());
        assert_eq!(
            settings(&dir)["briefing_time"],
            "07:30",
            "un rifiuto non cambia nulla"
        );
        // Un file corrotto o con tipi sbagliati ricade sui predefiniti.
        std::fs::write(dir.join(FILE), br#"{"enabled": "no", "disk": false"#).unwrap();
        assert_eq!(settings(&dir), defaults());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_briefing_summarises_the_day() {
        let mut facts = Facts::from_date(chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(), 8);
        let (title, body) = compose_briefing(&facts);
        assert_eq!(title, "Buongiorno: lunedì 23 novembre");
        assert_eq!(body, "• Nessuna cosa da fare aperta.");
        facts.open_todos = 5;
        facts.due_today = 1;
        facts.overdue = 2;
        facts.confirmations = 1;
        facts.unread = 3;
        facts.missions = 2;
        facts.failing = vec!["Backup".into()];
        facts.top = Some("pagare la bolletta".into());
        let (_, body) = compose_briefing(&facts);
        for expected in [
            "5 aperte, 1 in scadenza oggi, 2 in ritardo",
            "1 azione in attesa",
            "3 notifiche da leggere",
            "2 missioni in corso",
            "errori recenti: Backup",
            "Priorità consigliata: pagare la bolletta",
        ] {
            assert!(body.contains(expected), "{expected} in {body}");
        }
        facts.hour = 19;
        assert!(compose_briefing(&facts).0.starts_with("Buonasera"));
    }

    #[test]
    fn the_weekly_briefing_reports_the_past_week_and_the_next() {
        let (title, body) = compose_weekly(&WeekFacts {
            week: 48,
            ..Default::default()
        });
        assert!(title.ends_with("settimana 48"));
        assert!(body.contains("0 completate, 0 nuove, 0 ancora aperte"));
        assert!(body.contains("Nessuna scadenza nei prossimi sette giorni"));
        assert!(!body.contains("Richiede la tua attenzione"));
        let (_, body) = compose_weekly(&WeekFacts {
            week: 48,
            done: 6,
            opened: 4,
            open_todos: 9,
            overdue: 2,
            upcoming: vec!["firmare il contratto".into()],
            missions_active: 1,
            missions_completed: 2,
            memories: 3,
            runs: 7,
            runs_failed: 1,
            confirmations: 1,
            unread: 5,
        });
        for expected in [
            "6 completate, 4 nuove, 9 ancora aperte",
            "2 concluse, 1 in corso",
            "7 esecuzioni, di cui 1 non riuscite",
            "3 ricordi aggiunti",
            "Richiede la tua attenzione",
            "2 cose da fare in ritardo",
            "1 azione in attesa",
            "5 notifiche da leggere",
            "firmare il contratto",
            "priorità RICE",
        ] {
            assert!(body.contains(expected), "{expected} in {body}");
        }
    }

    #[test]
    fn the_weekly_day_is_validated() {
        let dir = std::env::temp_dir().join(format!("jarvis-weekly-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(settings(&dir)["weekly_day"], 0);
        assert!(update(&dir, &json!({"weekly_day": 7})).is_err());
        assert!(update(&dir, &json!({"weekly_day": "lunedì"})).is_err());
        assert_eq!(
            update(&dir, &json!({"weekly_day": 4})).unwrap()["weekly_day"],
            4
        );
        assert!(update(&dir, &json!({"weekly_time": "9:00"})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn disk_alerts_ignore_tiny_and_healthy_volumes() {
        let disks = json!({"disks": [
            {"mount": "/", "total_gb": 500.0, "free_gb": 20.0, "used_percent": 96.0},
            {"mount": "/data", "total_gb": 500.0, "free_gb": 250.0, "used_percent": 50.0},
            {"mount": "/boot", "total_gb": 0.4, "free_gb": 0.0, "used_percent": 100.0},
        ]});
        assert_eq!(disk_alerts(&disks), vec![("/".to_string(), 20.0, 96.0)]);
        assert_eq!(
            week_key(chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()),
            "2026-W48"
        );
    }

    #[test]
    fn sandbox_alerts_flag_only_nearly_full_sessions() {
        let sizes = vec![
            ("main".to_string(), 10 * 1024 * 1024),
            ("big".to_string(), 230 * 1024 * 1024),
        ];
        assert_eq!(sandbox_alerts(&sizes), vec![("big".to_string(), 230)]);
        assert!(defaults()["sandbox"] == true);
    }
}
