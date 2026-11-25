//! Ciclo dell'agente (ADR-0037): worker Python sintetico che propone più strumenti
//! in sequenza. Ogni passo passa dalla policy del Core; il ciclo si ferma alla risposta, a una proposta
//! ripetuta o dopo 4 strumenti.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-ciclo";

struct Script {
    calls: Vec<(&'static str, Value)>,
    repeat: bool,
    seen: Arc<Mutex<Vec<Value>>>,
}

fn fake_worker(
    calls: Vec<(&'static str, Value)>,
    repeat: bool,
) -> (Script, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    (
        Script {
            calls,
            repeat,
            seen: seen.clone(),
        },
        seen,
    )
}

async fn chat(script: Script, port: u16, message: &str) -> Value {
    let data = std::env::temp_dir().join(format!("jarvis-loop-{}", uuid::Uuid::new_v4().simple()));
    let log = data.join("delegations.jsonl");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port,
        data_dir: data,
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![
            ("JARVIS_TEST_CALLS".into(), json!(script.calls).to_string()),
            (
                "JARVIS_TEST_REPEAT".into(),
                if script.repeat { "1" } else { "0" }.into(),
            ),
            (
                "JARVIS_TEST_DELEGATIONS".into(),
                log.to_string_lossy().into_owned(),
            ),
        ],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let app = api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), port));
    let req = Request::builder()
        .method("POST")
        .uri("/api/chat")
        .header("host", format!("127.0.0.1:{port}"))
        .header("content-type", "application/json")
        .header("x-jarvis-token", TOKEN)
        .body(Body::from(json!({"message": message}).to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    *script.seen.lock().unwrap() = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

fn tools_called(out: &Value) -> Vec<String> {
    out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "tool_call")
        .map(|s| s["tool"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn agent_chains_tools_then_answers() {
    let (url, seen) = fake_worker(
        vec![("time.now", json!({})), ("system.info", json!({}))],
        false,
    );
    let out = chat(url, 18771, "che ore sono e che sistema uso?").await;
    assert_eq!(tools_called(&out), ["time.now", "system.info"], "{out}");
    assert_eq!(out["reply"], "Fatto: ecco il riepilogo.");
    assert_eq!(out["task"]["state"], "succeeded");
    let reqs = seen.lock().unwrap();
    assert_eq!(reqs.len(), 3, "due strumenti e la risposta finale");
    assert_eq!(reqs[1]["inputs"]["tool_result"]["tool"], "time.now");
    assert_eq!(reqs[1]["inputs"]["tool_result"]["status"], "succeeded");
}

#[tokio::test]
async fn repeated_proposal_closes_the_loop() {
    let (url, _) = fake_worker(vec![("time.now", json!({}))], true);
    let out = chat(url, 18772, "che ore sono").await;
    assert_eq!(
        tools_called(&out),
        ["time.now"],
        "la ripetizione non viene eseguita: {out}"
    );
    assert!(out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "loop_guard"));
    assert_eq!(out["task"]["state"], "succeeded");
}

#[tokio::test]
async fn loop_stops_after_four_tools() {
    let script = vec![
        ("time.now", json!({})),
        ("system.info", json!({})),
        ("memory.list", json!({})),
        ("memory.search", json!({"query": "caffè"})),
        ("fs.list", json!({"path": "."})),
    ];
    let (url, seen) = fake_worker(script, false);
    let out = chat(url, 18773, "fai un controllo completo").await;
    assert_eq!(
        tools_called(&out),
        ["time.now", "system.info", "memory.list", "memory.search"],
        "il quinto strumento non viene eseguito: {out}"
    );
    assert_eq!(out["reply"], "Fatto: ecco il riepilogo.");
    let last = seen.lock().unwrap().last().cloned().unwrap();
    assert!(
        last["inputs"]["steps_left"] == 0,
        "all'ultimo passo il modello non riceve strumenti"
    );
}

#[tokio::test]
async fn specialist_gets_only_its_tools_and_cannot_leave_its_scope() {
    // Richiesta di ricerca: l'agente `research` riceve solo Web, estrazione di testo, note, ora e ricerca nella memoria (ADR-0038, ADR-0171).
    let (url, seen) = fake_worker(vec![("shell.run", json!({"command": "ls"}))], false);
    let out = chat(url, 18774, "cerca sul web le novità di Rust").await;
    let first = seen.lock().unwrap()[0].clone();
    let mut offered: Vec<String> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    offered.sort();
    assert_eq!(
        offered,
        [
            "memory.search",
            "notes.add",
            "notes.search",
            "text.extract",
            "time.now",
            "web.browse",
            "web.fetch",
            "web.form",
            "web.search"
        ],
        "{first}"
    );
    assert_eq!(first["delegate"]["agent"], "research");
    let steps = out["steps"].as_array().unwrap();
    assert!(
        steps
            .iter()
            .any(|s| s["kind"] == "scope" && s["allowed"] == false),
        "{out}"
    );
    assert!(!steps.iter().any(|s| s["kind"] == "tool_result"), "{out}");
    assert!(out["confirmation"].is_null());
}

#[tokio::test]
async fn temporary_agents_only_get_tools_without_confirmation() {
    // Il principale chiede un agente temporaneo con fs.write: il Core rifiuta prima di crearlo (ADR-0040).
    let (url, seen) = fake_worker(
        vec![(
            "agent.delegate",
            json!({"role": "scrittore", "objective": "scrivi un file", "tools": ["fs.write"]}),
        )],
        false,
    );
    let out = chat(url, 18778, "usa un agente temporaneo per scrivere il file").await;
    assert!(
        !out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["kind"] == "subagent"),
        "{out}"
    );
    let reqs = seen.lock().unwrap();
    assert!(
        reqs[1]["inputs"]["tool_history"]
            .to_string()
            .contains("agent.invalid_tools"),
        "{}",
        reqs[1]
    );
}

#[tokio::test]
async fn invalid_arguments_are_explained_and_the_model_can_retry() {
    // Primo tentativo con un campo sbagliato; il modello riceve i difetti e riprova correttamente (ADR-0045).
    let (url, seen) = fake_worker(
        vec![
            ("memory.search", json!({"query": ""})),
            ("memory.search", json!({"query": "caffè"})),
        ],
        false,
    );
    let out = chat(url, 18779, "cerca nella memoria il caffè").await;
    let calls = tools_called(&out);
    assert_eq!(calls, ["memory.search", "memory.search"], "{out}");
    let results: Vec<&Value> = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "tool_result")
        .collect();
    assert_eq!(
        results.len(),
        1,
        "solo il secondo tentativo viene eseguito: {out}"
    );
    let second = seen.lock().unwrap()[1].clone();
    let text = second["inputs"]["tool_history"].to_string();
    assert!(
        text.contains("policy.invalid_arguments") && text.contains("expected"),
        "{text}"
    );
}

#[tokio::test]
async fn missing_required_tool_arguments_are_requested_without_a_retry() {
    let (url, seen) = fake_worker(vec![("memory.search", json!({}))], false);
    let out = chat(url, 18779, "cerca nella memoria").await;
    assert!(out["reply"].as_str().unwrap().contains("query"), "{out}");
    assert!(out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "clarification"));
    assert!(!out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "tool_result"));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn active_skills_enter_the_prompt_and_widen_only_the_offered_tools() {
    // «buongiorno» attiva la competenza inclusa «Briefing del mattino» anche per una richiesta di ricerca:
    // le sue istruzioni entrano nel prompt e i suoi strumenti vengono offerti per primi (ADR-0048).
    let (url, seen) = fake_worker(vec![], false);
    let out = chat(url, 18780, "buongiorno, cerca sul web le notizie di oggi").await;
    let first = seen.lock().unwrap()[0].clone();
    assert_eq!(first["inputs"]["skills"][0]["name"], "Briefing del mattino");
    let offered: Vec<String> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        offered.iter().any(|tool| tool == "weather.forecast"),
        "{offered:?}"
    );
    assert!(
        offered.iter().any(|tool| tool == "todo.list"),
        "{offered:?}"
    );
    assert!(!offered.iter().any(|t| t == "shell.run"), "{offered:?}");
    assert!(out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "skill" && s["skill"] == "briefing"));
}
