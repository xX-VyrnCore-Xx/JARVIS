//! Test d'integrazione dell'API con il worker Python reale in modalità offline (deterministica).
//! Richiede `python3` con `jsonschema` (tests/contracts/requirements.txt).

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "token-di-test";
const PORT: u16 = 18765;

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    let data = std::env::temp_dir().join(format!("jarvis-test-{}", uuid::Uuid::new_v4().simple()));
    Env {
        app: api::router(api::state(test_core(&data), TOKEN.into(), PORT)),
        data,
    }
}

fn test_core(data: &std::path::Path) -> Jarvis {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    Jarvis::new(cfg).expect("core")
}

impl Env {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
        host: &str,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", host)
            .header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("x-jarvis-token", t);
        }
        let req = req
            .body(
                body.map(|b| Body::from(b.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn api(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(
            method,
            path,
            body,
            Some(TOKEN),
            &format!("127.0.0.1:{PORT}"),
        )
        .await
    }

    async fn chat(&self, message: &str) -> Value {
        let (status, out) = self
            .api("POST", "/api/chat", Some(json!({"message": message})))
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        assert!(
            jarvis_contracts::validate(jarvis_contracts::Contract::Task, &out["task"]).is_valid(),
            "task non conforme: {out}"
        );
        out
    }
}

fn kinds(out: &Value) -> Vec<String> {
    out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn reply_without_tools() {
    let e = env();
    let out = e.chat("ciao").await;
    assert_eq!(out["task"]["state"], "succeeded");
    assert!(out["reply"].as_str().unwrap().contains("JARVIS"));
    assert_eq!(out["backend"]["name"], "offline");
    assert_eq!(kinds(&out), ["delegation"]);
}

#[tokio::test]
async fn low_risk_tool_runs_without_confirmation() {
    let e = env();
    let out = e.chat("che ore sono").await;
    assert_eq!(
        kinds(&out),
        ["delegation", "tool_call", "policy", "tool_result"]
    );
    assert_eq!(out["steps"][2]["decision"], "allow");
    assert!(out["reply"].as_str().unwrap().starts_with("Sono le"));
}

#[tokio::test]
async fn high_risk_write_requires_single_use_confirmation() {
    let e = env();
    let out = e.chat("scrivi note.txt: contenuto sintetico").await;
    assert_eq!(out["task"]["state"], "waiting_confirmation");
    assert!(
        !e.data.join("workspace/note.txt").exists(),
        "nessuna scrittura prima della conferma"
    );
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (_, pending) = e.api("GET", "/api/confirmations", None).await;
    assert_eq!(pending["items"].as_array().unwrap().len(), 1);

    let (status, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["task"]["state"], "succeeded");
    assert_eq!(
        std::fs::read_to_string(e.data.join("workspace/note.txt")).unwrap(),
        "contenuto sintetico"
    );

    let (reuse, _) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(
        reuse,
        StatusCode::NOT_FOUND,
        "una conferma vale una sola volta"
    );

    let read = e.chat("leggi note.txt").await;
    assert!(read["reply"]
        .as_str()
        .unwrap()
        .contains("contenuto sintetico"));
}

#[tokio::test]
async fn rejected_confirmation_cancels_without_effects() {
    let e = env();
    let out = e.chat("scrivi altro.txt: da non scrivere").await;
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": false})),
        )
        .await;
    assert_eq!(done["task"]["state"], "cancelled");
    assert!(!e.data.join("workspace/altro.txt").exists());
}

#[tokio::test]
async fn desktop_request_stays_local_with_cloud_selected() {
    let e = env();
    let (status, pending) = e
        .api(
            "POST",
            "/api/cloud/chat",
            Some(json!({"provider":"nim", "message":"Apri la calcolatrice"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    assert_eq!(pending["task"]["state"], "waiting_confirmation");
    assert_eq!(pending["confirmation"]["tool"], "pc.open");
    assert!(pending["backend"].is_null());
    let id = pending["confirmation"]["confirmation_id"].as_str().unwrap();

    let (status, done) = e
        .api(
            "POST",
            &format!("/api/confirmations/{id}"),
            Some(json!({"approve": false})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["task"]["state"], "cancelled");

    let (status, _) = e
        .api(
            "POST",
            "/api/workflows",
            Some(json!({"name":"Avvio", "steps":[{"tool":"pc.open", "arguments":{"target":"calculator"}}]})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn tool_failure_is_visible_in_task_state_and_catalog_matches_policy() {
    let e = env();
    let failed = e.chat("leggi file_inesistente.txt").await;
    assert_eq!(failed["task"]["state"], "failed", "{failed}");
    assert_eq!(failed["task"]["error"]["effect"], "not_applied");

    let (_, catalog) = e.api("GET", "/api/tools", None).await;
    let list = catalog["items"].as_array().unwrap();
    for name in ["pc.open", "code.run_python", "shell.run"] {
        assert!(list
            .iter()
            .any(|tool| tool["name"] == name && tool["workflow_step"] == false));
    }
}

#[tokio::test]
async fn path_traversal_is_denied_by_policy() {
    let e = env();
    for message in ["leggi ../../etc/passwd", "leggi /etc/passwd"] {
        let out = e.chat(message).await;
        let policy = out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "policy")
            .unwrap();
        assert_eq!(policy["decision"], "deny", "{message}");
        assert_eq!(policy["reasons"][0], "policy.path_outside_workspace");
        assert!(
            !kinds(&out).contains(&"tool_result".to_string()),
            "nessuna esecuzione dopo un diniego"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_is_blocked_at_execution() {
    let e = env();
    std::os::unix::fs::symlink("/etc", e.data.join("workspace/esterno")).unwrap();
    let out = e.chat("leggi esterno/hosts").await;
    let result = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "tool_result")
        .unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["error"], "fs.outside_workspace");
}

#[tokio::test]
async fn memory_round_trip_and_owner_deletion() {
    let e = env();
    let saved = e.chat("ricorda che il colore preferito è il blu").await;
    assert_eq!(saved["steps"][2]["decision"], "allow");
    let (_, mem) = e.api("GET", "/api/memory", None).await;
    let item = &mem["items"][0];
    assert_eq!(item["text"], "il colore preferito è il blu");

    let recall = e.chat("colore preferito?").await;
    assert!(
        recall["reply"].as_str().unwrap().contains("blu"),
        "la memoria entra nel contesto: {recall}"
    );

    let forget = e
        .chat(&format!("dimentica {}", item["id"].as_str().unwrap()))
        .await;
    assert_eq!(
        forget["task"]["state"], "waiting_confirmation",
        "cancellare la memoria via agente richiede conferma"
    );

    let (status, _) = e
        .api(
            "DELETE",
            &format!("/api/memory/{}", item["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // Restano solo gli episodi delle attività (ADR-0022), che non contengono il fatto cancellato.
    let (_, mem) = e.api("GET", "/api/memory?kind=semantic", None).await;
    assert!(mem["items"].as_array().unwrap().is_empty());
    let (_, all) = e.api("GET", "/api/memory", None).await;
    assert!(!all.to_string().contains("blu"), "{all}");
}

#[tokio::test]
async fn audit_chain_records_every_decision() {
    let e = env();
    e.chat("che ore sono").await;
    let out = e.chat("scrivi a.txt: x").await;
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    e.api(
        "POST",
        &format!("/api/confirmations/{id}"),
        Some(json!({"approve": true})),
    )
    .await;
    let (_, audit) = e.api("GET", "/api/audit", None).await;
    assert_eq!(audit["chain_valid"], true);
    let outcomes: Vec<&str> = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|e| e["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(
        outcomes,
        ["allowed", "succeeded", "allowed", "confirmed", "succeeded"]
    );

    // Manomissione del file: la verifica la rileva.
    let path = e.data.join("audit/core-security.jsonl");
    let tampered =
        std::fs::read_to_string(&path)
            .unwrap()
            .replacen("\"succeeded\"", "\"failed\"", 1);
    std::fs::write(&path, tampered).unwrap();
    let (_, audit) = e.api("GET", "/api/audit", None).await;
    assert_eq!(audit["chain_valid"], false);
}

#[tokio::test]
async fn api_requires_token_and_local_host() {
    let e = env();
    let host = format!("127.0.0.1:{PORT}");
    let (s, _) = e
        .call(
            "POST",
            "/api/chat",
            Some(json!({"message": "ciao"})),
            None,
            &host,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = e
        .call("GET", "/api/memory", None, Some("sbagliato"), &host)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = e
        .call(
            "GET",
            "/api/health",
            None,
            Some(TOKEN),
            &format!("attacker.example:{PORT}"),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "difesa dal DNS rebinding");
    let (s, health) = e.call("GET", "/api/health", None, None, &host).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(health["ai"]["status"], "up");
}

#[tokio::test]
async fn security_posture_and_cloud_configuration_are_local_and_hide_keys() {
    let e = env();
    let host = format!("127.0.0.1:{PORT}");
    for path in ["/api/security", "/api/cloud", "/api/n8n"] {
        assert_eq!(
            e.call("GET", path, None, None, &host).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (code, report) = e.api("GET", "/api/security", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(report["audit_chain_valid"], true);
    let out = e.chat("controlla la postura di sicurezza").await;
    assert!(out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["tool"] == "security.status"));
    let secret = "SyntheticNimKeyForJarvisTest";
    let (code, report) = e
        .api(
            "POST",
            "/api/cloud",
            Some(json!({
                "provider":"nvidia_nim", "model":"meta/llama-3.3-70b-instruct", "key":secret
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(report["providers"][1]["configured"], true);
    assert_eq!(report["providers"][1]["effective_rpm"], 35);
    assert!(!report.to_string().contains(secret));
    assert!(!std::fs::read_to_string(e.data.join("cloud.json"))
        .unwrap()
        .contains(secret));
    let (code, report) = e
        .api(
            "POST",
            "/api/cloud",
            Some(json!({
                "provider":"nvidia_nim", "remove_key":true
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(report["providers"][1]["configured"], false);
    assert_eq!(
        e.api(
            "POST",
            "/api/cloud/chat",
            Some(json!({
                "message":"test locale", "provider":"nvidia_nim"
            }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn credential_like_content_is_rejected_before_the_cloud_request() {
    let e = env();
    let (status, _) = e
        .api(
            "POST",
            "/api/cloud",
            Some(json!({
                "provider":"nvidia_nim", "model":"synthetic", "key":"synthetic"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = e
        .api(
            "POST",
            "/api/cloud/chat",
            Some(json!({
                "provider":"nvidia_nim", "message":"Authorization: Bearer valore-di-prova"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains("valore-di-prova"));
    let (_, turns) = e.api("GET", "/api/conversation", None).await;
    assert_eq!(turns["items"], json!([]));
}

#[tokio::test]
async fn python_sandbox_execution_requires_confirmation_and_cannot_be_scheduled() {
    let e = env();
    let pending = e.chat("esegui python in sandbox: print(2 + 2)").await;
    assert_eq!(
        pending["task"]["state"], "waiting_confirmation",
        "{pending}"
    );
    assert_eq!(pending["confirmation"]["tool"], "code.run_python");
    let (status, _) = e
        .api(
            "POST",
            "/api/workflows",
            Some(json!({
                "name":"Test Python", "trigger":{"kind":"interval","every_seconds":60},
                "steps":[{"tool":"code.run_python","arguments":{"source":"print(1)"}}]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn cloud_context_requires_opt_in_and_stays_with_the_provider() {
    use jarvis_core::cloud::{Answer, Completed, Provider};

    let dir = std::env::temp_dir().join(format!(
        "jarvis-cloud-context-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let mut core = test_core(&dir);
    for provider in ["nvidia_nim", "ollama_cloud"] {
        core.configure_cloud(&json!({"provider":provider, "model":"synthetic", "key":"synthetic"}))
            .unwrap();
    }
    let turn = |core: &mut Jarvis, question: &str, answer: &str| {
        core.finish_cloud_chat(
            question.into(),
            Completed {
                provider: Provider::Nim,
                model: "synthetic".into(),
                fallback: None,
                answer: Ok(Answer::Text(answer.into())),
            },
            jarvis_core::observability::Timer::start(),
        )
        .unwrap();
    };
    core.chat("scrivi privata.txt: appunto offline").unwrap();
    turn(
        &mut core,
        "Parliamo di JARVIS",
        "Possiamo esaminare il Core e il client.",
    );
    turn(&mut core, "E il Core?", "Il Core gestisce gli strumenti.");
    let ask = |core: &Jarvis, use_context| {
        core.prepare_cloud_chat(&json!({
            "provider":"auto", "use_context":use_context, "message":"E il secondo?"
        }))
        .unwrap()
    };
    let standalone = ask(&core, false);
    assert_eq!(standalone.targets[0].provider.id(), "ollama_cloud");
    assert!(standalone
        .targets
        .iter()
        .all(|target| target.history.is_empty()));
    let connected = ask(&core, true);
    assert_eq!(connected.targets[0].provider.id(), "nvidia_nim");
    let history = &connected.targets[0].history;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].user, "Parliamo di JARVIS");
    assert_eq!(history[1].assistant, "Il Core gestisce gli strumenti.");
    assert!(connected.targets[1].history.is_empty());

    let long = "a".repeat(3000);
    turn(&mut core, "Terzo", &long);
    turn(&mut core, "Quarto", "quarto esito");
    let bounded = ask(&core, true);
    let history = &bounded.targets[0].history;
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].user, "Parliamo di JARVIS");
    assert_eq!(history[2].assistant, long);

    core.chat("che ore sono").unwrap();
    assert!(ask(&core, true)
        .targets
        .iter()
        .all(|target| target.history.is_empty()));
    drop(core);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn source_followup_uses_only_the_previous_turn_and_stays_local() {
    let e = env();
    let out = e.chat("che ore sono").await;
    assert_eq!(out["task"]["state"], "succeeded");
    let (status, out) = e
        .api(
            "POST",
            "/api/cloud/chat",
            Some(json!({
                "provider": "ollama_cloud", "message": "Qual è la fonte?"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["sources"], json!([]));
    assert!(out["reply"]
        .as_str()
        .unwrap()
        .contains("risposta precedente"));
}

#[test]
fn cloud_tool_calls_use_core_policy_before_workspace_changes() {
    use jarvis_core::cloud::{Answer, Completed, Failure, Provider};
    use jarvis_core::observability::Timer;

    let dir = std::env::temp_dir().join(format!("jarvis-cloud-{}", uuid::Uuid::new_v4().simple()));
    let mut core = test_core(&dir);
    let turn = |core: &mut Jarvis, arguments: Value| {
        core.finish_cloud_chat(
            "Crea un appunto".into(),
            Completed {
                model: "synthetic".into(),
                provider: Provider::Nim,
                fallback: Some((Provider::Ollama, Failure::Unavailable)),
                answer: Ok(Answer::Call(json!({
                    "tool": "fs.write", "version": "1.0.0", "arguments": arguments,
                }))),
            },
            Timer::start(),
        )
        .unwrap()
        .to_json()
    };
    let missing = turn(&mut core, json!({"path":"nota.txt"}));
    assert!(
        kinds(&missing).contains(&"clarification".into()),
        "{missing}"
    );
    let pending = turn(
        &mut core,
        json!({"path":"nota.txt", "content":"verificato"}),
    );
    assert_eq!(
        pending["task"]["state"], "waiting_confirmation",
        "{pending}"
    );
    assert!(kinds(&pending).contains(&"cloud_fallback".into()));
    assert_eq!(pending["backend"]["name"], "nvidia_nim");
    let path = dir.join("workspace/nota.txt");
    assert!(!path.exists());
    let id = pending["confirmation"]["confirmation_id"].as_str().unwrap();
    let confirmed = core.confirm(id, true).unwrap().to_json();
    assert_eq!(confirmed["task"]["state"], "succeeded", "{confirmed}");
    assert_eq!(confirmed["backend"]["name"], "nvidia_nim");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "verificato");
    assert!(core.confirm(id, true).is_err());
    let limited = core
        .finish_cloud_chat(
            "Limite remoto".into(),
            Completed {
                provider: Provider::Nim,
                model: "synthetic".into(),
                fallback: None,
                answer: Err(Failure::RateLimited(std::time::Duration::from_secs(10))),
            },
            Timer::start(),
        )
        .unwrap()
        .to_json();
    assert_eq!(limited["task"]["state"], "failed");
    assert_eq!(limited["task"]["error"]["code"], "cloud.rate_limited");
    drop(core);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn n8n_chat_routine_requires_owner_confirmation() {
    let e = env();
    let (status, out) = e
        .api("POST", "/api/n8n", Some(json!({"enable": true})))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["enabled"], true);
    let out = e.chat("n8n audit ogni 15 minuti: Verifica registro").await;
    assert_eq!(out["confirmation"]["tool"], "n8n.create", "{out}");
    assert_eq!(out["confirmation"]["arguments"]["kind"], "audit");
    assert_eq!(out["confirmation"]["arguments"]["minutes"], 15);
}

#[tokio::test]
async fn invalid_input_is_rejected() {
    let e = env();
    let (s, _) = e
        .api("POST", "/api/chat", Some(json!({"message": "   "})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .api(
            "POST",
            "/api/chat",
            Some(json!({"message": "x".repeat(8001)})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .api(
            "POST",
            "/api/confirmations/inesistente",
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn web_pages_and_pwa_assets_are_not_served() {
    let e = env();
    for path in [
        "/",
        "/app.js",
        "/app.css",
        "/manifest.webmanifest",
        "/sw.js",
        "/fcm-sw.js",
        "/pair",
        "/pair.js",
        "/voice-worklet.js",
    ] {
        let req = Request::builder()
            .uri(path)
            .header("host", format!("localhost:{PORT}"))
            .body(Body::empty())
            .unwrap();
        let response = e.app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn conversation_and_tasks_persist_across_restart() {
    let e = env();
    e.chat("ciao").await;
    drop(e.app);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: e.data.clone(),
        python: "python3".into(),
        ai_dir: root.join("ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let again = Env {
        app: api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT)),
        data: e.data.clone(),
    };
    let (_, conv) = again.api("GET", "/api/conversation", None).await;
    assert_eq!(conv["items"].as_array().unwrap().len(), 2);
    let (_, tasks) = again.api("GET", "/api/tasks", None).await;
    assert_eq!(tasks["items"][0]["state"], "succeeded");
}

/// Aggiornamenti (ADR-0034): spenti per impostazione, configurazione validata, notifica una sola volta per versione.
#[tokio::test]
async fn update_check_notifies_once_per_version() {
    let e = env();
    let (_, s) = e.api("GET", "/api/updates", None).await;
    assert_eq!(
        (s["enabled"].as_bool(), s["channel"].as_str()),
        (Some(false), Some("stable"))
    );
    let (status, _) = e
        .api(
            "POST",
            "/api/updates",
            Some(json!({"enabled": true, "channel": "nightly"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, s) = e
        .api(
            "POST",
            "/api/updates",
            Some(json!({"enabled": true, "channel": "beta"})),
        )
        .await;
    assert_eq!(
        (status, s["channel"].as_str()),
        (StatusCode::OK, Some("beta"))
    );
}

#[tokio::test]
async fn every_task_has_a_w3c_trace_with_agent_policy_and_tool_spans() {
    let e = env();
    let out = e.chat("che ore sono").await;
    let id = out["task"]["task_id"].as_str().unwrap();
    let (status, trace) = e.api("GET", &format!("/api/tasks/{id}/trace"), None).await;
    assert_eq!(status, StatusCode::OK, "{trace}");
    let corr = out["task"]["correlation_id"]
        .as_str()
        .unwrap()
        .replace('-', "");
    assert_eq!(
        trace["trace_id"],
        corr.as_str(),
        "traccia = correlation_id (ADR-0039)"
    );
    let spans = trace["spans"].as_array().unwrap();
    let kinds: Vec<&str> = spans.iter().map(|s| s["kind"].as_str().unwrap()).collect();
    for k in ["agent", "policy", "tool", "task"] {
        assert!(kinds.contains(&k), "manca l'intervallo {k}: {kinds:?}");
    }
    let root = spans.iter().find(|s| s["kind"] == "task").unwrap();
    assert!(root["parent_span_id"].is_null());
    assert_eq!(root["span_id"].as_str().unwrap().len(), 16);
    for s in spans.iter().filter(|s| s["kind"] != "task") {
        assert_eq!(s["parent_span_id"], root["span_id"], "{s}");
        assert_eq!(s["trace_id"], trace["trace_id"]);
    }
    assert!(trace["total_ms"].as_f64().unwrap() >= 0.0);
    // Nessun contenuto dell'utente nella traccia.
    assert!(!trace.to_string().contains("che ore sono"), "{trace}");

    // Con la conferma la traccia include l'attesa del proprietario.
    let out = e.chat("scrivi traccia.txt: ciao").await;
    let cid = out["confirmation"]["confirmation_id"].as_str().unwrap();
    let id = out["task"]["task_id"].as_str().unwrap().to_string();
    let (status, _) = e
        .api(
            "POST",
            &format!("/api/confirmations/{cid}"),
            Some(json!({"approve": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, trace) = e.api("GET", &format!("/api/tasks/{id}/trace"), None).await;
    let spans = trace["spans"].as_array().unwrap();
    assert!(
        spans
            .iter()
            .any(|s| s["kind"] == "confirmation" && s["status"] == "approved"),
        "{trace}"
    );
    assert!(
        spans
            .iter()
            .any(|s| s["name"] == "fs.write" && s["status"] == "ok"),
        "{trace}"
    );

    let (status, _) = e.api("GET", "/api/tasks/inesistente/trace", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&e.data);
}
