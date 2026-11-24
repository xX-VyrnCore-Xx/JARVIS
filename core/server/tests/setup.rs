//! Test della configurazione guidata dei modelli (ADR-0019) con passi simulati (`JARVIS_SETUP_FAKE=1`).

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const PORT: u16 = 18773;

fn app(data: &std::path::Path) -> axum::Router {
    std::env::set_var("JARVIS_SETUP_FAKE", "1");
    std::env::set_var("JARVIS_SETUP_FAKE_FAIL", "voice.packages-in-un-altro-test");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    api::router(api::state(Jarvis::new(cfg).unwrap(), "t".into(), PORT))
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: bool,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", format!("127.0.0.1:{PORT}"));
    if token {
        b = b.header("x-jarvis-token", "t");
    }
    let req = match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn guided_setup_runs_steps_records_choices_and_restarts_the_worker() {
    let data = std::env::temp_dir().join(format!("jarvis-setup-{}", uuid::Uuid::new_v4().simple()));
    let app = app(&data);
    assert_eq!(
        call(&app, "GET", "/api/setup", None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, status) = call(&app, "GET", "/api/setup", None, true).await;
    assert_eq!(status["first_run"], true);
    assert_eq!(status["choices"]["stt_models"][2], "small");

    // Valori fuori elenco rifiutati: niente argomenti arbitrari a pip.
    let (code, _) = call(
        &app,
        "POST",
        "/api/setup/start",
        Some(json!({"stt_model": "--index-url=http://x"})),
        true,
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let plan = json!({"voice": true, "stt_model": "base", "tts_voice": "it_IT-riccardo-x_low"});
    let (code, _) = call(&app, "POST", "/api/setup/start", Some(plan.clone()), true).await;
    assert_eq!(code, StatusCode::ACCEPTED);
    let (code, _) = call(&app, "POST", "/api/setup/start", Some(plan), true).await;
    assert_eq!(
        code,
        StatusCode::CONFLICT,
        "una sola configurazione alla volta"
    );

    let mut last = Value::Null;
    for _ in 0..200 {
        let (_, s) = call(&app, "GET", "/api/setup", None, true).await;
        if s["running"] == false && s["finished_at"].is_string() {
            last = s;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let steps = last["steps"].as_array().expect("configurazione terminata");
    let ids: Vec<&str> = steps.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(
        ids,
        ["voice.packages", "voice.stt", "voice.tts", "voice.wake"]
    );
    assert!(steps.iter().all(|s| s["status"] == "done"), "{last}");
    assert_eq!(last["first_run"], false);
    let saved: Value =
        serde_json::from_slice(&std::fs::read(data.join("setup.json")).unwrap()).unwrap();
    assert_eq!(saved["plan"]["stt_model"], "base");

    // Il worker riparte e funziona dopo la configurazione.
    let (code, out) = call(
        &app,
        "POST",
        "/api/chat",
        Some(json!({"message": "che ore sono"})),
        true,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{out}");
}

#[test]
fn worker_env_points_to_installed_packages_and_models() {
    let data = std::env::temp_dir().join(format!(
        "jarvis-setup-env-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(data.join("python-packages")).unwrap();
    std::fs::create_dir_all(data.join("models/piper")).unwrap();
    std::fs::write(data.join("models/piper/it_IT-riccardo-x_low.onnx"), b"x").unwrap();
    std::fs::write(
        data.join("setup.json"),
        json!({"plan": {"tts_voice": "it_IT-paola-medium", "stt_model": "small", "ollama_model": "qwen2.5:3b"}}).to_string(),
    )
    .unwrap();
    let setup = jarvis_core::setup::Setup::new(&data, "python3", &data);
    let env: std::collections::HashMap<String, String> = setup.worker_env().into_iter().collect();
    assert!(
        env["PYTHONPATH"].starts_with(&data.join("python-packages").to_string_lossy().into_owned())
    );
    assert!(env["JARVIS_TTS_VOICE"].ends_with("it_IT-riccardo-x_low.onnx"));
    assert!(!env.contains_key("JARVIS_OLLAMA_MODEL"));
}
