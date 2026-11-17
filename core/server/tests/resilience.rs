//! Fase 4 — robustezza e osservabilità (RNF-02, RNF-07): guasti iniettati nel worker reale,
//! recupero, correlazione nei log senza contenuti, concorrenza e degrado controllato.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

fn config(data: &Path, timeout_ms: u64, worker_env: Vec<(String, String)>) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18770,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: timeout_ms,
        voice_engine: Some("fake".into()),
        worker_env,
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    }
}

fn temp() -> PathBuf {
    std::env::temp_dir().join(format!("jarvis-res-{}", uuid::Uuid::new_v4().simple()))
}

/// Jarvis con un guasto che si verifica una sola volta (marker), poi il worker riavviato funziona.
fn with_fault(fault: &str, timeout_ms: u64) -> (Jarvis, PathBuf) {
    let data = temp();
    std::fs::create_dir_all(&data).unwrap();
    let env = vec![
        ("JARVIS_FAULT_INJECTION".into(), "1".into()),
        ("JARVIS_FAULT".into(), format!("{fault}-once")),
        (
            "JARVIS_FAULT_MARKER".into(),
            data.join("fault.marker").to_string_lossy().into_owned(),
        ),
    ];
    (Jarvis::new(config(&data, timeout_ms, env)).unwrap(), data)
}

fn counter(j: &Jarvis, name: &str) -> u64 {
    j.observability().snapshot()["counters"][name]
        .as_u64()
        .unwrap_or(0)
}

fn assert_fault_then_recovery(fault: &str, expected_code: &str) {
    let (mut j, _) = with_fault(fault, 1500);
    let failed = j.chat("che ore sono").unwrap();
    assert_eq!(
        failed.task["state"], "failed",
        "{fault}: il guasto deve far fallire il task"
    );
    assert_eq!(failed.task["error"]["code"], expected_code, "{fault}");
    assert_eq!(
        failed.task["error"]["effect"], "unknown",
        "{fault}: esito incerto, nessuna ripetizione cieca"
    );
    // Il guasto è rilevabile e correlato al task (RF-13).
    let notes = j.notifications(10, true, None).unwrap();
    assert_eq!(
        notes["items"][0]["task_id"], failed.task["task_id"],
        "{fault}"
    );
    // Recupero: il worker viene riavviato alla richiesta successiva.
    let ok = j.chat("che ore sono").unwrap();
    assert_eq!(
        ok.task["state"], "succeeded",
        "{fault}: recupero dopo il guasto"
    );
    assert_eq!(
        counter(&j, "worker.spawned"),
        2,
        "{fault}: un riavvio del worker"
    );
    assert_eq!(counter(&j, "delegation.failed"), 1);
}

#[test]
fn worker_hang_times_out_and_recovers() {
    assert_fault_then_recovery("hang", "bridge.timeout");
}

#[test]
fn worker_crash_is_isolated_and_recovers() {
    assert_fault_then_recovery("crash", "bridge.worker_unavailable");
}

#[test]
fn worker_garbage_output_is_rejected_and_recovers() {
    assert_fault_then_recovery("garbage", "bridge.invalid_response");
}

#[test]
fn slow_worker_within_timeout_is_measured_not_failed() {
    let (mut j, _) = with_fault("slow:300", 5_000);
    assert_eq!(j.chat("che ore sono").unwrap().task["state"], "succeeded");
    let m = j.observability().snapshot();
    assert!(
        m["series"]["delegation"]["general_assistant"]["max_ms"]
            .as_f64()
            .unwrap()
            >= 300.0,
        "{m}"
    );
    assert_eq!(counter(&j, "delegation.failed"), 0);
}

#[test]
fn logs_are_correlated_and_never_contain_user_content() {
    let data = temp();
    let mut j = Jarvis::new(config(&data, 30_000, vec![])).unwrap();
    let out = j
        .chat("ricorda che il mio codice segreto è 4815162342")
        .unwrap();
    let log = std::fs::read_to_string(data.join("logs/core.jsonl")).unwrap();
    assert!(
        !log.contains("4815162342") && !log.contains("segreto"),
        "nessun contenuto dell'utente nei log"
    );
    let finished: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .filter(|v: &Value| v["event"] == "task.finished")
        .collect();
    assert_eq!(finished[0]["task_id"], out.task["task_id"]);
    assert_eq!(finished[0]["correlation_id"], out.task["correlation_id"]);
    assert!(finished[0]["duration_ms"].as_f64().unwrap() > 0.0);
}

#[test]
fn deleted_audit_log_is_detected_and_core_keeps_working() {
    let data = temp();
    let mut j = Jarvis::new(config(&data, 30_000, vec![])).unwrap();
    j.chat("che ore sono").unwrap();
    std::fs::remove_file(data.join("audit/core-security.jsonl")).unwrap();
    j.chat("che ore sono").unwrap();
    assert_eq!(
        j.audit_log(10).unwrap()["chain_valid"],
        false,
        "la cancellazione rompe la catena"
    );
    j.monitor().unwrap();
    let notes = j.notifications(10, true, None).unwrap();
    assert!(notes["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["category"] == "security"));
}

#[test]
fn removed_workspace_degrades_to_a_tool_error() {
    let data = temp();
    let mut j = Jarvis::new(config(&data, 30_000, vec![])).unwrap();
    std::fs::remove_dir_all(data.join("workspace")).unwrap();
    let out = j.chat("elenca file").unwrap().to_json();
    let result = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "tool_result")
        .unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(
        j.chat("che ore sono").unwrap().task["state"],
        "succeeded",
        "il resto continua a funzionare"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_are_serialized_without_errors() {
    let data = temp();
    let app = api::router(api::state(
        Jarvis::new(config(&data, 30_000, vec![])).unwrap(),
        "t".into(),
        18770,
    ));
    let calls = (0..20).map(|i| {
        let app = app.clone();
        tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("host", "127.0.0.1:18770")
                .header("x-jarvis-token", "t")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"message": if i % 2 == 0 { "che ore sono" } else { "ciao" }})
                        .to_string(),
                ))
                .unwrap();
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            (
                status,
                body["task"]["state"].as_str().unwrap_or("").to_string(),
            )
        })
    });
    for call in calls {
        let (status, state) = call.await.unwrap();
        assert_eq!((status, state.as_str()), (StatusCode::OK, "succeeded"));
    }
    let req = Request::builder()
        .uri("/api/metrics")
        .header("host", "127.0.0.1:18770")
        .header("x-jarvis-token", "t")
        .body(Body::empty())
        .unwrap();
    let metrics: Value = serde_json::from_slice(
        &app.oneshot(req)
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(metrics["series"]["http"]["POST /api/chat"]["count"], 20);
    assert_eq!(metrics["series"]["task"]["chat"]["errors"], 0);
    assert_eq!(metrics["series"]["tool"]["time.now"]["count"], 10);
}

#[tokio::test]
async fn metrics_require_the_session_token() {
    let app = api::router(api::state(
        Jarvis::new(config(&temp(), 30_000, vec![])).unwrap(),
        "t".into(),
        18770,
    ));
    let req = Request::builder()
        .uri("/api/metrics")
        .header("host", "127.0.0.1:18770")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn metrics_survive_restart_and_export_in_prometheus_format() {
    let data = temp();
    {
        let mut j = Jarvis::new(config(&data, 30_000, vec![])).unwrap();
        j.chat("che ore sono").unwrap();
        let saved = j
            .persist_metrics(true)
            .unwrap()
            .expect("istantanea forzata");
        assert!(saved["series"]["tool"]["time.now"]["count"] == 1, "{saved}");
        assert!(
            j.persist_metrics(false).unwrap().is_none(),
            "al massimo un'istantanea ogni 5 minuti"
        );
        let text = saved.to_string();
        assert!(
            !text.contains("che ore sono"),
            "nessun contenuto dell'utente"
        );
    }
    // Dopo il riavvio lo storico resta e le nuove metriche ripartono da zero.
    let j = Jarvis::new(config(&data, 30_000, vec![])).unwrap();
    let history = j.metrics_history(24).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["series"]["tool"]["time.now"]["count"], 1);

    let app = api::router(api::state(j, "t".into(), 18770));
    let get = |uri: &str, token: bool| {
        let mut b = Request::builder()
            .uri(uri)
            .header("host", "127.0.0.1:18770");
        if token {
            b = b.header("x-jarvis-token", "t");
        }
        b.body(Body::empty()).unwrap()
    };
    let res = app
        .clone()
        .oneshot(get("/api/metrics/export?format=prometheus", false))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = app
        .clone()
        .oneshot(get("/api/metrics/export?format=prometheus", true))
        .await
        .unwrap();
    assert!(res.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/plain; version=0.0.4"));
    let body =
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(
        body.contains("# TYPE jarvis_requests_total counter"),
        "{body}"
    );
    assert!(body.contains("jarvis_uptime_seconds"));
    let res = app
        .clone()
        .oneshot(get("/api/metrics/export", true))
        .await
        .unwrap();
    let doc: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(doc["format"], "jarvis-metrics/1");
    assert_eq!(doc["history"].as_array().unwrap().len(), 1);
    let res = app
        .oneshot(get("/api/metrics/history?hours=48", true))
        .await
        .unwrap();
    let doc: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(doc["items"].as_array().unwrap().len(), 1);
}
