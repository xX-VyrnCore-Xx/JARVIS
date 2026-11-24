//! Iniziative proattive (ADR-0097): briefing quotidiano una sola volta al giorno, ore di silenzio,
//! conferme dimenticate, impostazioni persistenti e briefing su richiesta.

use std::path::PathBuf;

use chrono::{Local, TimeZone};
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

fn jarvis() -> Jarvis {
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18786,
        data_dir: std::env::temp_dir().join(format!(
            "jarvis-proactive-it-{}",
            uuid::Uuid::new_v4().simple()
        )),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    Jarvis::new(cfg).unwrap()
}

fn at(day: u32, h: u32, m: u32) -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 11, day, h, m, 0)
        .earliest()
        .unwrap()
}

fn notes(j: &Jarvis, category: &str) -> Vec<Value> {
    j.notifications(100, false, None).unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["category"] == category)
        .cloned()
        .collect()
}

#[test]
fn the_morning_briefing_is_sent_once_a_day_with_the_real_state() {
    let mut j = jarvis();
    j.proactive_update(&json!({"disk": false, "weekly_briefing": false}))
        .unwrap();
    j.chat("ricordami di pagare la bolletta entro 2020-01-01 09:00")
        .unwrap();
    j.chat("da fare: leggere il contratto").unwrap();
    j.chat("prendi nota: comprare il pane").unwrap();
    assert!(
        j.proactive_tick(at(23, 7, 30)).unwrap().is_empty(),
        "prima dell'orario scelto"
    );
    assert_eq!(j.proactive_tick(at(23, 8, 5)).unwrap(), ["briefing"]);
    assert!(
        j.proactive_tick(at(23, 9, 0)).unwrap().is_empty(),
        "una sola volta al giorno"
    );
    let briefing = notes(&j, "briefing");
    assert_eq!(briefing.len(), 1);
    assert_eq!(briefing[0]["title"], "Buongiorno: lunedì 23 novembre");
    let body = briefing[0]["body"].as_str().unwrap();
    assert!(
        body.contains("2 aperte") && body.contains("1 in ritardo"),
        "{body}"
    );
    assert!(
        body.contains("Priorità consigliata: pagare la bolletta"),
        "{body}"
    );
    // Il giorno dopo ne arriva uno nuovo; molto più tardi non ha più senso.
    assert!(
        j.proactive_tick(at(24, 15, 0)).unwrap().is_empty(),
        "fuori finestra"
    );
    assert_eq!(j.proactive_tick(at(24, 8, 0)).unwrap(), ["briefing"]);
}

#[test]
fn the_weekly_briefing_arrives_once_a_week_on_the_chosen_day() {
    let mut j = jarvis();
    j.proactive_update(&json!({"disk": false, "briefing": false}))
        .unwrap();
    j.chat("da fare: preparare il budget").unwrap();
    assert!(
        j.proactive_tick(at(23, 8, 0)).unwrap().is_empty(),
        "prima dell'orario scelto"
    );
    assert_eq!(j.proactive_tick(at(23, 8, 30)).unwrap(), ["weekly"]);
    assert!(
        j.proactive_tick(at(23, 9, 30)).unwrap().is_empty(),
        "una sola volta a settimana"
    );
    assert!(
        j.proactive_tick(at(24, 8, 30)).unwrap().is_empty(),
        "non è il giorno scelto"
    );
    let weekly = notes(&j, "briefing");
    assert_eq!(weekly.len(), 1);
    assert!(weekly[0]["title"]
        .as_str()
        .unwrap()
        .starts_with("Briefing settimanale della direzione"));
    let body = weekly[0]["body"].as_str().unwrap();
    assert!(
        body.contains("Settimana trascorsa") && body.contains("1 ancora aperte"),
        "{body}"
    );
    assert_eq!(j.proactive_tick(at(30, 8, 30)).unwrap(), ["weekly"]);
    // Il giorno si sceglie: giovedì.
    j.proactive_update(&json!({"weekly_day": 3})).unwrap();
    assert!(j.proactive_tick(at(30, 9, 0)).unwrap().is_empty());
    let report = j.weekly_briefing_now().unwrap();
    assert!(report["body"]
        .as_str()
        .unwrap()
        .contains("Settimana in arrivo"));
}

#[test]
fn nothing_is_sent_during_quiet_hours_and_settings_are_respected() {
    let mut j = jarvis();
    j.proactive_update(&json!({"disk": false})).unwrap();
    j.proactive_update(
        &json!({"briefing_time": "23:00", "quiet_from": "22:00", "quiet_to": "07:00"}),
    )
    .unwrap();
    assert!(
        j.proactive_tick(at(23, 23, 30)).unwrap().is_empty(),
        "silenzio"
    );
    j.proactive_update(&json!({"quiet_from": "01:00", "quiet_to": "02:00"}))
        .unwrap();
    assert_eq!(j.proactive_tick(at(23, 23, 30)).unwrap(), ["briefing"]);
    j.proactive_update(&json!({"briefing": false})).unwrap();
    assert!(
        j.proactive_tick(at(24, 23, 30)).unwrap().is_empty(),
        "briefing disattivato"
    );
    j.proactive_update(&json!({"briefing": true, "enabled": false}))
        .unwrap();
    assert!(
        j.proactive_tick(at(25, 23, 30)).unwrap().is_empty(),
        "tutto disattivato"
    );
    assert!(j
        .proactive_update(&json!({"briefing_time": "mezzogiorno"}))
        .is_err());
    assert_eq!(j.proactive_settings()["briefing_time"], "23:00");
}

#[test]
fn a_forgotten_confirmation_is_recalled_once() {
    let mut j = jarvis();
    // Nessun silenzio e nessun altro tipo di iniziativa: il test non dipende dall'ora né dai dischi.
    j.proactive_update(
        &json!({"briefing": false, "disk": false, "quiet_from": "00:00", "quiet_to": "00:00"}),
    )
    .unwrap();
    let out = j.chat("scrivi bozza.txt: testo").unwrap().to_json();
    assert_eq!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert!(j
        .proactive_tick(Local::now() + chrono::Duration::minutes(5))
        .unwrap()
        .is_empty());
    let later = Local::now() + chrono::Duration::minutes(20);
    assert_eq!(j.proactive_tick(later).unwrap(), ["confirmations"]);
    assert!(
        j.proactive_tick(later + chrono::Duration::minutes(5))
            .unwrap()
            .is_empty(),
        "una sola volta"
    );
    let recalls: Vec<Value> = notes(&j, "confirmation")
        .into_iter()
        .filter(|n| n["title"] == "Un'azione attende la tua conferma")
        .collect();
    assert_eq!(recalls.len(), 1);
    assert!(recalls[0]["body"].as_str().unwrap().contains("bozza.txt"));
}

#[test]
fn an_on_demand_briefing_can_be_repeated() {
    let mut j = jarvis();
    let first = j.briefing_now().unwrap();
    assert!(first["title"].as_str().unwrap().contains(char::is_numeric));
    j.briefing_now().unwrap();
    assert_eq!(notes(&j, "briefing").len(), 2);
}
