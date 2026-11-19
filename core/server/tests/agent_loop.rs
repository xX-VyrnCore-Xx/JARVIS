//! Ciclo dell'agente (ADR-0037): worker Python reale contro un Ollama sintetico che propone più strumenti
//! in sequenza. Ogni passo passa dalla policy del Core; il ciclo si ferma alla risposta, a una proposta
//! ripetuta o dopo 4 strumenti.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
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

/// Ollama sintetico: con strumenti nella richiesta risponde con la prossima chiamata del copione (o ripete
/// l'ultima se `repeat`), senza strumenti risponde con un testo. Registra le richieste a `/api/chat`.
fn fake_ollama(
    script: Vec<(&'static str, Value)>,
    repeat: bool,
) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let mut queue: VecDeque<(&'static str, Value)> = script.into();
    std::thread::spawn(move || {
        let mut last: Option<(&'static str, Value)> = None;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let payload = if first.starts_with("GET /api/tags") {
                json!({"models": [{"name": "qwen2.5:3b"}]})
            } else {
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                log.lock().unwrap().push(req.clone());
                let next = if req.get("tools").is_some() {
                    queue.pop_front().or_else(|| if repeat { last.clone() } else { None })
                } else {
                    None
                };
                match next {
                    Some((name, args)) => {
                        last = Some((name, args.clone()));
                        json!({"message": {"role": "assistant", "content": "", "tool_calls": [{"function": {"name": name, "arguments": args}}]},
                               "done": true, "prompt_eval_count": 10, "eval_count": 2})
                    }
                    None => json!({"message": {"role": "assistant", "content": "Fatto: ecco il riepilogo."},
                                   "done": true, "prompt_eval_count": 10, "eval_count": 5}),
                }
            }
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
        }
    });
    (url, seen)
}

async fn chat(ollama: String, port: u16, message: &str) -> Value {
    let data = std::env::temp_dir().join(format!("jarvis-loop-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port,
        data_dir: data,
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        ai_backend: Some("ollama".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![
            ("JARVIS_OLLAMA_URL".into(), ollama),
            ("JARVIS_TOOL_SELECTION".into(), "all".into()),
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
    let (url, seen) = fake_ollama(
        vec![("time__now", json!({})), ("system__info", json!({}))],
        false,
    );
    let out = chat(url, 18771, "che ore sono e che sistema uso?").await;
    assert_eq!(tools_called(&out), ["time.now", "system.info"], "{out}");
    assert_eq!(out["reply"], "Fatto: ecco il riepilogo.");
    assert_eq!(out["task"]["state"], "succeeded");
    let reqs = seen.lock().unwrap();
    assert_eq!(reqs.len(), 3, "due strumenti e la risposta finale");
    // Il secondo passo riceve il primo risultato come chiamata e risposta dello strumento.
    let second = &reqs[1]["messages"];
    assert!(second.to_string().contains("time__now"), "{second}");
    assert!(second
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "tool"));
    // Il prompt di sistema porta data, regole e memoria.
    let system = reqs[0]["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.contains("Data e ora locali") && system.contains("uno per volta"),
        "{system}"
    );
}

#[tokio::test]
async fn repeated_proposal_closes_the_loop() {
    let (url, _) = fake_ollama(vec![("time__now", json!({}))], true);
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
        ("time__now", json!({})),
        ("system__info", json!({})),
        ("memory__list", json!({})),
        ("memory__search", json!({"query": "caffè"})),
        ("fs__list", json!({"path": "."})),
    ];
    let (url, seen) = fake_ollama(script, false);
    let out = chat(url, 18773, "fai un controllo completo").await;
    assert_eq!(
        tools_called(&out),
        ["time.now", "system.info", "memory.list", "memory.search"],
        "il quinto strumento non viene eseguito: {out}"
    );
    assert_eq!(out["reply"], "Fatto: ecco il riepilogo.");
    let last = seen.lock().unwrap().last().cloned().unwrap();
    assert!(
        last.get("tools").is_none(),
        "all'ultimo passo il modello non riceve strumenti"
    );
}

#[tokio::test]
async fn specialist_gets_only_its_tools_and_cannot_leave_its_scope() {
    // Richiesta di ricerca: l'agente `research` riceve solo Web, ora e ricerca nella memoria (ADR-0038).
    let (url, seen) = fake_ollama(vec![("shell__run", json!({"command": "ls"}))], false);
    let out = chat(url, 18774, "cerca sul web le novità di Rust").await;
    let first = seen.lock().unwrap()[0].clone();
    let mut offered: Vec<String> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect();
    offered.sort();
    assert_eq!(
        offered,
        [
            "memory__search",
            "time__now",
            "web__browse",
            "web__fetch",
            "web__form",
            "web__search"
        ],
        "{first}"
    );
    assert!(first["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("Specializzazione per questa richiesta — Ricerca"));
    // Una proposta fuori ambito viene scartata (dal worker e, in seconda difesa, dal Core): nessuna esecuzione.
    let steps = out["steps"].as_array().unwrap();
    assert_eq!(steps[0]["agent"], "research", "{out}");
    assert!(
        !steps
            .iter()
            .any(|s| s["kind"] == "tool_call" || s["kind"] == "policy"),
        "{out}"
    );
    assert!(out["confirmation"].is_null());
}

#[tokio::test]
async fn temporary_agents_only_get_tools_without_confirmation() {
    // Il principale chiede un agente temporaneo con fs.write: il Core rifiuta prima di crearlo (ADR-0040).
    let (url, seen) = fake_ollama(
        vec![(
            "agent__delegate",
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
        reqs[1]["messages"]
            .to_string()
            .contains("agent.invalid_tools"),
        "{}",
        reqs[1]
    );
}

#[tokio::test]
async fn invalid_arguments_are_explained_and_the_model_can_retry() {
    // Primo tentativo con un campo sbagliato; il modello riceve i difetti e riprova correttamente (ADR-0045).
    let (url, seen) = fake_ollama(
        vec![
            ("memory__search", json!({"domanda": "caffè"})),
            ("memory__search", json!({"query": "caffè"})),
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
    let text = second["messages"].to_string();
    assert!(
        text.contains("policy.invalid_arguments") && text.contains("expected"),
        "{text}"
    );
}
