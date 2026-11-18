//! Memoria completa (ADR-0022): tipi, ricerca ibrida, knowledge graph, episodi, correzione e cancellazione
//! propagate agli indici, ricostruzione. Core reale con il worker Python offline.

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
const PORT: u16 = 18766;

struct Env {
    app: axum::Router,
    data: PathBuf,
}

fn env() -> Env {
    let data =
        std::env::temp_dir().join(format!("jarvis-memory-{}", uuid::Uuid::new_v4().simple()));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: root.join("ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    let core = Jarvis::new(cfg).expect("core");
    Env {
        app: api::router(api::state(core, TOKEN.into(), PORT)),
        data,
    }
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
        out
    }

    async fn items(&self, query: &str) -> Vec<Value> {
        let (status, out) = self.api("GET", &format!("/api/memory{query}"), None).await;
        assert_eq!(status, StatusCode::OK, "{out}");
        out["items"].as_array().unwrap().clone()
    }
}

fn texts(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|i| i["text"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn memory_kinds_search_graph_and_episodes() {
    let env = env();
    env.chat("ricorda che il mio cane si chiama Fido").await;
    env.chat("ricorda che la riunione di progetto è il martedì")
        .await;
    env.chat("preferisco risposte brevi").await;
    env.chat("che ore sono").await;

    let semantic = env.items("?kind=semantic").await;
    assert_eq!(semantic.len(), 2, "{semantic:?}");
    assert_eq!(semantic[0]["source"]["kind"], "agent");
    let procedural = env.items("?kind=procedural").await;
    assert_eq!(texts(&procedural), vec!["preferisco risposte brevi"]);

    // Episodio per l'attività conclusa, legato al task e con scadenza; nessun episodio per la memoria.
    let episodic = env.items("?kind=episodic").await;
    assert_eq!(episodic.len(), 1, "{episodic:?}");
    assert!(episodic[0]["text"].as_str().unwrap().contains("time.now"));
    assert!(episodic[0]["source"]["task_id"].is_string());
    assert!(episodic[0]["expires_at"].is_string());

    // Ricerca ibrida tollerante alle flessioni: «cani» trova «cane».
    let found = env.items("?q=come%20si%20chiamano%20i%20cani").await;
    assert_eq!(found[0]["text"], "il mio cane si chiama Fido", "{found:?}");
    assert!(found[0]["score"].as_f64().unwrap() > 0.0);
    let (status, _) = env.api("GET", "/api/memory?kind=altro", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Knowledge graph derivato dai fatti.
    let (_, graph) = env.api("GET", "/api/memory/graph", None).await;
    let edge = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["to"] == "fido")
        .cloned()
        .expect("arco verso Fido");
    assert_eq!(
        (edge["from"].as_str(), edge["relation"].as_str()),
        (Some("io"), Some("cane"))
    );

    // Dalla chat: ricerca in memoria e preferenze sempre nel contesto dell'agente.
    let out = env.chat("cosa sai di Fido?").await;
    assert!(out["reply"].as_str().unwrap().contains("Fido"), "{out}");

    // Correzione: testo, indice e grafo aggiornati, con audit.
    let id = edge["memory_id"].as_str().unwrap().to_string();
    let (status, _) = env
        .api(
            "PATCH",
            &format!("/api/memory/{id}"),
            Some(json!({"text": "il mio cane si chiama Rex"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let found = env.items("?q=Rex").await;
    assert_eq!(found[0]["id"], id.as_str());
    assert!(env
        .items("?q=Fido")
        .await
        .iter()
        .all(|i| i["id"] != id.as_str()));
    let (_, graph) = env.api("GET", "/api/memory/graph", None).await;
    assert!(graph.to_string().contains("\"rex\"") && !graph.to_string().contains("\"fido\""));
    let (status, _) = env
        .api(
            "PATCH",
            "/api/memory/inesistente",
            Some(json!({"text": "x"})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Cancellazione propagata a indice e grafo.
    let (status, _) = env.api("DELETE", &format!("/api/memory/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(env.items("?q=Rex").await.is_empty());
    let (_, graph) = env.api("GET", "/api/memory/graph", None).await;
    assert!(!graph.to_string().contains("rex"));

    // Indici ricostruibili dalla sola tabella sorgente.
    let (_, r) = env.api("POST", "/api/memory/reindex", None).await;
    assert_eq!(r["reindexed"], 3);
    assert_eq!(
        env.items("?q=martedi").await[0]["text"],
        "la riunione di progetto è il martedì"
    );
    let (_, audit) = env.api("GET", "/api/audit?limit=50", None).await;
    let audit = audit.to_string();
    assert!(audit.contains("memory.correct") && audit.contains("memory.reindex"));
    let _ = std::fs::remove_dir_all(&env.data);
}

#[tokio::test]
async fn same_fact_is_not_stored_twice_and_database_reports_health() {
    let env = env();
    env.chat("ricorda che il mio cane si chiama Fido").await;
    let again = env.chat("ricorda che il mio cane si chiama fido!").await;
    assert!(
        again["reply"]
            .as_str()
            .unwrap_or_default()
            .contains("Lo sapevo già"),
        "{again}"
    );
    assert_eq!(
        env.items("?kind=semantic").await.len(),
        1,
        "nessun duplicato (ADR-0037)"
    );

    let (status, db) = env.api("GET", "/api/database", None).await;
    assert_eq!(status, StatusCode::OK, "{db}");
    assert_eq!(db["ok"], true, "{db}");
    assert_eq!(db["schema_version"], jarvis_core::store::SCHEMA_VERSION);
    assert_eq!(db["rows"]["memory"], 1);
    assert!(db["size_bytes"].as_i64().unwrap() > 0);
    let _ = std::fs::remove_dir_all(&env.data);
}
