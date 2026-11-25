//! Navigazione Web interattiva (ADR-0043) con il browser del sistema (Chrome, Chromium o Edge) contro un sito
//! sintetico locale: JavaScript eseguito, collegamenti seguiti, modulo compilato e inviato, richieste verso
//! host vietati bloccate dal Core anche quando le chiede la pagina.
//! In CI (`CI` impostata) il browser deve esserci; in locale, senza browser, il test lo dichiara e termina.

use std::sync::{Arc, Mutex};

use axum::routing::{get, post};
use jarvis_core::browser::{browse, find_browser, submit_form};
use jarvis_core::web::WebConfig;
use serde_json::json;

/// I test avviano un browser vero: uno alla volta, altrimenti su una macchina carica gli avvii a freddo si
/// rallentano a vicenda (guasto intermittente visto in CI).
static ONE_BROWSER_AT_A_TIME: Mutex<()> = Mutex::new(());

fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    ONE_BROWSER_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn browser_or_skip() -> bool {
    if find_browser().is_some() {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "in CI serve un browser compatibile (google-chrome)"
    );
    eprintln!("nessun browser compatibile installato: test della navigazione non eseguito");
    false
}

/// Sito sintetico; restituisce la porta e il registro delle richieste ricevute.
fn site() -> (u16, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    let home = format!(
        r#"<!doctype html><html><head><title>Sito di prova</title></head><body>
        <h1>Benvenuto</h1><p id="dyn">vuoto</p>
        <a href="/seconda">Seconda pagina</a>
        <img src="http://vietato.localhost:{port}/spia">
        <form action="/invia" method="post"><label>Nome <input name="nome"></label>
          <input name="password" type="password"><button>Invia</button></form>
        <script>document.getElementById('dyn').textContent = 'Contenuto generato dallo script';</script>
        </body></html>"#
    );
    let app = axum::Router::new()
        .route("/", get(move || async move { axum::response::Html(home) }))
        .route(
            "/seconda",
            get(move || async move {
                l1.lock().unwrap().push("seconda".into());
                axum::response::Html("<title>Seconda</title><p>Sei nella seconda pagina</p>")
            }),
        )
        .route(
            "/invia",
            post(move |body: String| async move {
                l2.lock().unwrap().push(format!("invia:{body}"));
                let nome = body
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("nome="))
                    .unwrap_or("")
                    .to_string();
                axum::response::Html(format!("<title>Risposta</title><p>Grazie {nome}</p>"))
            }),
        )
        .route(
            "/spia",
            get(move || async move {
                l3.lock().unwrap().push("spia".into());
                ""
            }),
        );
    std::thread::spawn(move || rt.block_on(async { axum::serve(listener, app).await.unwrap() }));
    (port, log)
}

fn web() -> WebConfig {
    WebConfig {
        allow_private: true,
        deny: vec!["vietato.localhost".into()],
        ..WebConfig::default()
    }
}

#[test]
fn browser_renders_follows_links_and_submits_forms_within_the_web_policy() {
    let _one = exclusive();
    if !browser_or_skip() {
        return;
    }
    let (port, log) = site();
    let url = format!("http://127.0.0.1:{port}/");

    let page = browse(&web(), &json!({"url": url})).unwrap();
    assert_eq!(page["title"], "Sito di prova");
    assert!(
        page["text"]
            .as_str()
            .unwrap()
            .contains("Contenuto generato dallo script"),
        "JavaScript eseguito: {page}"
    );
    assert!(page["links"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["text"] == "Seconda pagina"));
    let fields: Vec<&str> = page["forms"][0]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["nome", "password"]);
    assert_eq!(page["untrusted"], true);
    assert_eq!(page["content_sha256"].as_str().unwrap().len(), 64);
    // La pagina chiede un'immagine a un host vietato: il Core blocca la richiesta.
    assert!(
        page["blocked_requests"]
            .to_string()
            .contains("vietato.localhost"),
        "{page}"
    );
    assert!(!log.lock().unwrap().contains(&"spia".to_string()));

    let second = browse(&web(), &json!({"url": url, "follow": ["seconda"]})).unwrap();
    assert!(
        second["text"]
            .as_str()
            .unwrap()
            .contains("Sei nella seconda pagina"),
        "{second}"
    );
    assert!(second["url"].as_str().unwrap().ends_with("/seconda"));

    let sent = submit_form(
        &web(),
        &json!({"url": url, "fields": {"nome": "Ada", "password": "segreta"}}),
    )
    .unwrap();
    assert!(
        sent["text"].as_str().unwrap().contains("Grazie Ada"),
        "{sent}"
    );
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|e| e == "invia:nome=Ada&password=segreta"));

    let missing =
        submit_form(&web(), &json!({"url": url, "fields": {"inesistente": "x"}})).unwrap_err();
    assert_eq!(missing.code, "web.element_not_found");
}

#[test]
fn private_addresses_are_refused_before_launching_the_page() {
    let _one = exclusive();
    if !browser_or_skip() {
        return;
    }
    let strict = WebConfig::default();
    let e = browse(&strict, &json!({"url": "http://127.0.0.1:9/"})).unwrap_err();
    assert_eq!(e.code, "policy.private_address");
}

#[test]
fn chat_browses_freely_but_submits_forms_only_after_confirmation_with_masked_secrets() {
    let _one = exclusive();
    if !browser_or_skip() {
        return;
    }
    let (port, log) = site();
    let url = format!("http://127.0.0.1:{port}/");
    let data =
        std::env::temp_dir().join(format!("jarvis-browser-{}", uuid::Uuid::new_v4().simple()));
    let cfg = jarvis_core::config::Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18790,
        data_dir: data.clone(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 60_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: web(),
    };
    let mut j = jarvis_core::orchestrator::Jarvis::new(cfg).unwrap();

    let out = j
        .chat(&format!("naviga {url} e clicca Seconda pagina"))
        .unwrap()
        .to_json();
    let result = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "tool_result")
        .cloned()
        .unwrap();
    assert_eq!(result["status"], "succeeded", "{out}");
    assert!(
        out["confirmation"].is_null(),
        "la lettura non chiede conferma"
    );
    assert!(
        out["reply"]
            .as_str()
            .unwrap()
            .contains("Sei nella seconda pagina"),
        "{out}"
    );
    assert!(
        out["sources"].to_string().contains("/seconda"),
        "fonte con l'indirizzo finale: {out}"
    );

    let out = j
        .chat(&format!("compila modulo {url}: nome=Ada, password=segreta"))
        .unwrap()
        .to_json();
    let effect = out["confirmation"]["expected_effect"].as_str().unwrap();
    assert!(
        effect.contains("nome=Ada") && effect.contains("password=•••"),
        "{effect}"
    );
    assert!(
        !effect.contains("segreta"),
        "la password non compare nella conferma"
    );
    assert!(
        !log.lock().unwrap().iter().any(|e| e.starts_with("invia:")),
        "nessun invio prima della conferma"
    );
    let id = out["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = j.confirm(&id, true).unwrap().to_json();
    assert!(
        done["reply"].as_str().unwrap().contains("Grazie Ada"),
        "{done}"
    );
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|e| e == "invia:nome=Ada&password=segreta"));
    let _ = std::fs::remove_dir_all(&data);
}
