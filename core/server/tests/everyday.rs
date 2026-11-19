//! Strumenti quotidiani (ADR-0046) dalla chat con il worker Python reale in modalità offline: calcolo,
//! conversioni, note, cose da fare con notifica alla scadenza, file con conferma, meteo contro un Open-Meteo
//! sintetico, correzione di un ricordo.

use std::path::PathBuf;

use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};

fn jarvis(tag: &str, web: jarvis_core::web::WebConfig) -> (Jarvis, PathBuf) {
    let data = std::env::temp_dir().join(format!(
        "jarvis-everyday-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18783,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web,
    };
    (Jarvis::new(cfg).unwrap(), data)
}

fn say(j: &mut Jarvis, msg: &str) -> Value {
    j.chat(msg).unwrap().to_json()
}

fn reply(out: &Value) -> String {
    out["reply"].as_str().unwrap_or_default().to_string()
}

#[test]
fn everyday_tools_work_from_the_chat() {
    let (mut j, data) = jarvis("base", jarvis_core::web::WebConfig::default());
    assert_eq!(
        reply(&say(&mut j, "calcola (2+3)*4 - 2^3")),
        "(2+3)*4 - 2^3 = 12"
    );
    assert!(reply(&say(&mut j, "converti 100 c in f")).contains("212"));
    assert!(reply(&say(&mut j, "quanti giorni mancano al 2099-01-01")).contains("mancano"));

    let out = say(&mut j, "prendi nota: comprare il latte e il pane");
    assert!(reply(&out).starts_with("Nota salvata"), "{out}");
    assert!(reply(&say(&mut j, "cerca nelle note pane")).contains("comprare il latte"));

    // Cosa da fare già scaduta: il pianificatore la notifica una sola volta.
    let out = say(
        &mut j,
        "ricordami di pagare la bolletta entro 2020-01-01 09:00",
    );
    let id = reply(&out).split("id ").nth(1).unwrap()[..8].to_string();
    j.run_due(chrono::Utc::now()).unwrap();
    j.run_due(chrono::Utc::now()).unwrap();
    let notes = j.notifications(50, false, None).unwrap();
    let todo: Vec<&Value> = notes["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| {
            n["title"]
                .as_str()
                .unwrap_or_default()
                .contains("pagare la bolletta")
        })
        .collect();
    assert_eq!(todo.len(), 1, "una notifica per la scadenza: {notes}");
    assert!(reply(&say(&mut j, "cose da fare")).contains("pagare la bolletta"));
    assert!(reply(&say(&mut j, &format!("fatto {id}"))).contains("completata"));
    assert!(reply(&say(&mut j, "cose da fare")).contains("Nessuna cosa da fare"));

    // File: scrittura confermata, ricerca libera, eliminazione solo dopo la conferma.
    let out = say(&mut j, "scrivi bozza.txt: testo da cercare");
    j.confirm(
        out["confirmation"]["confirmation_id"].as_str().unwrap(),
        true,
    )
    .unwrap();
    assert!(reply(&say(&mut j, "trova file con «da cercare»")).contains("bozza.txt"));
    let out = say(&mut j, "elimina bozza.txt");
    assert_eq!(out["task"]["state"], "waiting_confirmation", "{out}");
    assert!(out["confirmation"]["expected_effect"]
        .as_str()
        .unwrap()
        .contains("bozza.txt"));
    assert!(data.join("workspace/bozza.txt").exists());
    j.confirm(
        out["confirmation"]["confirmation_id"].as_str().unwrap(),
        true,
    )
    .unwrap();
    assert!(!data.join("workspace/bozza.txt").exists());
    let out = say(&mut j, "sposta ../fuori.txt in dentro.txt");
    let policy = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "policy")
        .unwrap();
    assert_eq!(policy["reasons"][0], "policy.path_outside_workspace");

    assert!(reply(&say(&mut j, "spazio su disco")).contains("GB liberi"));
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn weather_uses_open_meteo_through_the_web_policy() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new()
        .route("/v1/search", axum::routing::get(|| async {
            axum::Json(json!({"results": [{"name": "Roma", "admin1": "Lazio", "country": "Italia", "latitude": 41.9, "longitude": 12.5}]}))
        }))
        .route("/v1/forecast", axum::routing::get(|| async {
            axum::Json(json!({
                "current": {"temperature_2m": 18.5, "apparent_temperature": 17.9, "relative_humidity_2m": 60, "weather_code": 1, "wind_speed_10m": 9.0},
                "daily": {"time": ["2026-11-19", "2026-11-20"], "weather_code": [61, 0], "temperature_2m_max": [19.0, 21.0],
                          "temperature_2m_min": [11.0, 12.0], "precipitation_probability_max": [80, 5]}
            }))
        }));
    std::thread::spawn(move || rt.block_on(async { axum::serve(listener, app).await.unwrap() }));
    std::env::set_var("JARVIS_WEATHER_URL", format!("http://127.0.0.1:{port}"));
    let web = jarvis_core::web::WebConfig {
        allow_private: true,
        ..Default::default()
    };
    let (mut j, data) = jarvis("meteo", web);
    let out = say(&mut j, "meteo a Roma");
    let text = reply(&out);
    assert!(
        text.contains("Meteo a Roma") && text.contains("18.5") && text.contains("pioggia"),
        "{out}"
    );
    assert!(out["confirmation"].is_null());
    let (mut off, data2) = jarvis(
        "meteo-off",
        jarvis_core::web::WebConfig {
            enabled: false,
            ..Default::default()
        },
    );
    let out = say(&mut off, "meteo a Roma");
    let policy = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "policy")
        .unwrap();
    assert_eq!(policy["decision"], "deny", "Web spento: niente meteo");
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&data2);
}
