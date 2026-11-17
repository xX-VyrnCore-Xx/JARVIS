//! Automation Engine: calcolo delle esecuzioni, costruzione e verifica dei workflow (RF-07).
//!
//! L'esecuzione dei passi avviene nell'Orchestrator con la policy vigente; qui
//! restano le regole pure, testabili senza processo né database.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use jarvis_contracts::canonical_digest;
use serde_json::Value;

/// Prossima esecuzione strettamente successiva a `after`, nel fuso `tz` per i trigger giornalieri.
pub fn next_run<Tz: TimeZone>(
    trigger: &Value,
    after: DateTime<Utc>,
    tz: &Tz,
) -> Option<DateTime<Utc>> {
    match trigger["kind"].as_str()? {
        "interval" => Some(after + Duration::seconds(trigger["every_seconds"].as_i64()?)),
        "daily" => {
            let at = NaiveTime::parse_from_str(trigger["at"].as_str()?, "%H:%M").ok()?;
            let mut day = after.with_timezone(tz).date_naive();
            for _ in 0..4 {
                // `earliest` gestisce le ore ripetute; nel salto dell'ora legale l'orario non esiste e si passa avanti di un'ora.
                let local = tz
                    .from_local_datetime(&day.and_time(at))
                    .earliest()
                    .or_else(|| {
                        tz.from_local_datetime(&(day.and_time(at) + Duration::hours(1)))
                            .earliest()
                    });
                if let Some(candidate) = local.map(|t| t.with_timezone(&Utc)) {
                    if candidate > after {
                        return Some(candidate);
                    }
                }
                day = day.succ_opt()?;
            }
            None
        }
        "once" => {
            let at = DateTime::parse_from_rfc3339(trigger["at"].as_str()?)
                .ok()?
                .with_timezone(&Utc);
            (at > after).then_some(at)
        }
        _ => None,
    }
}

/// Prima esecuzione alla creazione o alla riattivazione.
pub fn first_run<Tz: TimeZone>(
    trigger: &Value,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Option<DateTime<Utc>> {
    next_run(trigger, now, tz)
}

pub fn steps_digest(steps: &Value) -> String {
    canonical_digest(steps)
}

/// Descrizione leggibile del trigger.
pub fn describe(trigger: &Value) -> String {
    match trigger["kind"].as_str() {
        Some("interval") => {
            let s = trigger["every_seconds"].as_i64().unwrap_or(0);
            match s {
                s if s % 86_400 == 0 => plural(s / 86_400, "giorno", "giorni"),
                s if s % 3600 == 0 => plural(s / 3600, "ora", "ore"),
                s if s % 60 == 0 => plural(s / 60, "minuto", "minuti"),
                s => format!("ogni {s} secondi"),
            }
        }
        Some("daily") => format!("ogni giorno alle {}", trigger["at"].as_str().unwrap_or("?")),
        Some("once") => format!(
            "una volta, il {}",
            fmt_local(trigger["at"].as_str().unwrap_or(""))
        ),
        _ => "trigger sconosciuto".into(),
    }
}

fn plural(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("ogni {one}")
    } else {
        format!("ogni {n} {many}")
    }
}

pub fn fmt_local(rfc3339: &str) -> String {
    DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}

pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Riassunto breve dell'output di un passo per la cronologia (minimizzato).
pub fn summarize(output: &Value) -> String {
    let raw = output.to_string();
    let mut s: String = raw.chars().take(300).collect();
    if raw.chars().count() > 300 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use serde_json::json;

    fn t(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    #[test]
    fn interval_adds_period() {
        let tr = json!({"kind": "interval", "every_seconds": 300});
        assert_eq!(
            next_run(&tr, t("2026-11-17T12:00:00Z"), &Utc),
            Some(t("2026-11-17T12:05:00Z"))
        );
    }

    #[test]
    fn daily_uses_local_time_and_rolls_to_tomorrow() {
        let rome = FixedOffset::east_opt(3600).unwrap();
        let tr = json!({"kind": "daily", "at": "07:30"});
        // 06:00 UTC = 07:00 a UTC+1: oggi alle 07:30 locali = 06:30 UTC.
        assert_eq!(
            next_run(&tr, t("2026-11-17T06:00:00Z"), &rome),
            Some(t("2026-11-17T06:30:00Z"))
        );
        // Dopo l'orario: domani.
        assert_eq!(
            next_run(&tr, t("2026-11-17T06:30:00Z"), &rome),
            Some(t("2026-11-18T06:30:00Z"))
        );
    }

    #[test]
    fn once_only_in_future() {
        let tr = json!({"kind": "once", "at": "2026-11-18T08:00:00Z"});
        assert_eq!(
            next_run(&tr, t("2026-11-17T08:00:00Z"), &Utc),
            Some(t("2026-11-18T08:00:00Z"))
        );
        assert_eq!(next_run(&tr, t("2026-11-18T08:00:00Z"), &Utc), None);
    }

    #[test]
    fn descriptions() {
        assert_eq!(
            describe(&json!({"kind": "interval", "every_seconds": 60})),
            "ogni minuto"
        );
        assert_eq!(
            describe(&json!({"kind": "interval", "every_seconds": 7200})),
            "ogni 2 ore"
        );
        assert_eq!(
            describe(&json!({"kind": "daily", "at": "08:15"})),
            "ogni giorno alle 08:15"
        );
    }

    #[test]
    fn digest_changes_with_steps() {
        let a = json!([{"tool": "fs.write", "version": "1.0.0", "arguments": {"path": "a", "content": "x"}}]);
        let b = json!([{"tool": "fs.write", "version": "1.0.0", "arguments": {"path": "a", "content": "y"}}]);
        assert_ne!(steps_digest(&a), steps_digest(&b));
    }
}
