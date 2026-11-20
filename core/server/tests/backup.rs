//! RNF-08: backup cifrato e ripristino su ambiente pulito, con verifiche d'integrità.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use jarvis_core::{api, backup};
use serde_json::{json, Value};
use tower::ServiceExt;

const PASS: &str = "passphrase di prova lunga";

fn config(data: &Path) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 18771,
        data_dir: data.to_path_buf(),
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        ai_backend: Some("offline".into()),
        delegation_timeout_ms: 30_000,
        voice_engine: Some("fake".into()),
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: "t".into(),
        web: jarvis_core::web::WebConfig::default(),
    }
}

fn root() -> PathBuf {
    std::env::temp_dir().join(format!("jarvis-bak-{}", uuid::Uuid::new_v4().simple()))
}

/// Popola dati realistici: conversazione, memoria, file confermato, automazione.
fn populated(data: &Path) -> (Value, String) {
    let mut j = Jarvis::new(config(data)).unwrap();
    j.chat("ricorda che il backup funziona").unwrap();
    let out = j.chat("scrivi diario.txt: pagina importante").unwrap();
    let id = out.confirmation.unwrap()["confirmation_id"]
        .as_str()
        .unwrap()
        .to_string();
    j.confirm(&id, true).unwrap();
    j.create_workflow(&json!({"name": "Ora", "trigger": {"kind": "interval", "every_seconds": 3600}, "steps": [{"tool": "time.now", "arguments": {}}]}))
        .unwrap();
    std::fs::write(
        data.join("updates.json"),
        r#"{"enabled":true,"channel":"beta"}"#,
    )
    .unwrap();
    let summary = j.create_backup(PASS).unwrap();
    let file = config(data)
        .backups_dir()
        .join(summary["file"].as_str().unwrap())
        .to_string_lossy()
        .into_owned();
    (summary, file)
}

#[test]
fn backup_restores_everything_on_a_clean_environment() {
    let base = root();
    let (summary, file) = populated(&base.join("data"));
    assert_eq!(summary["audit"]["chain_valid"], true);
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..8], b"JARVISB1");
    assert!(
        !String::from_utf8_lossy(&bytes).contains("pagina importante"),
        "il contenuto è cifrato"
    );

    // Ambiente pulito: un'altra cartella mai usata.
    let clean = base.join("altro-computer/data");
    let report = backup::restore(Path::new(&file), PASS, &clean).unwrap();
    assert_eq!(
        (
            report["database"].as_str(),
            report["audit_chain_valid"].as_bool()
        ),
        (Some("ok"), Some(true))
    );
    assert!(report["previous_data"].is_null());
    assert!(std::fs::read_to_string(clean.join("updates.json"))
        .unwrap()
        .contains("beta"));

    let mut j = Jarvis::new(config(&clean)).unwrap();
    assert_eq!(
        std::fs::read_to_string(clean.join("workspace/diario.txt")).unwrap(),
        "pagina importante"
    );
    assert_eq!(j.memory().unwrap()[0]["text"], "il backup funziona");
    assert_eq!(
        j.workflows().unwrap().len(),
        1,
        "le automazioni tornano con la loro approvazione"
    );
    assert!(j.conversation(50).unwrap().len() >= 4);
    let audit = j.audit_log(100).unwrap();
    assert_eq!(audit["chain_valid"], true);
    assert_eq!(audit["total"], summary["audit"]["events"].as_u64().unwrap());
    // Il Core ripristinato continua a funzionare e a estendere la stessa catena.
    j.chat("che ore sono").unwrap();
    assert_eq!(j.audit_log(100).unwrap()["chain_valid"], true);
}

#[test]
fn wrong_passphrase_and_tampering_are_rejected_without_touching_data() {
    let base = root();
    let (_, file) = populated(&base.join("data"));
    let target = base.join("ripristino/data");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("segnaposto.txt"), "dati attuali").unwrap();

    let err = backup::restore(Path::new(&file), "passphrase sbagliata!", &target).unwrap_err();
    assert!(err.contains("passphrase errata"), "{err}");
    let mut bytes = std::fs::read(&file).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    let tampered = base.join("alterato.jbak");
    std::fs::write(&tampered, &bytes).unwrap();
    assert!(backup::restore(&tampered, PASS, &target)
        .unwrap_err()
        .contains("alterato"));
    std::fs::write(base.join("finto.jbak"), b"non un backup").unwrap();
    assert!(backup::restore(&base.join("finto.jbak"), PASS, &target)
        .unwrap_err()
        .contains("non è un backup"));

    assert_eq!(
        std::fs::read_to_string(target.join("segnaposto.txt")).unwrap(),
        "dati attuali",
        "dati intatti dopo i rifiuti"
    );
    let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".jarvis-restore")
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "nessuna cartella temporanea abbandonata"
    );
}

#[test]
fn restore_over_existing_data_keeps_a_copy() {
    let base = root();
    let data = base.join("data");
    let (_, file) = populated(&data);
    std::fs::write(data.join("workspace/dopo.txt"), "scritto dopo il backup").unwrap();
    let report = backup::restore(Path::new(&file), PASS, &data).unwrap();
    let previous = PathBuf::from(report["previous_data"].as_str().unwrap());
    assert!(
        previous.join("workspace/dopo.txt").exists(),
        "i dati sostituiti sono conservati"
    );
    assert!(
        !data.join("workspace/dopo.txt").exists(),
        "i dati tornano allo stato del backup"
    );
}

#[test]
fn short_passphrase_is_refused_and_data_dir_is_locked() {
    let base = root();
    let data = base.join("data");
    let mut j = Jarvis::new(config(&data)).unwrap();
    assert!(matches!(
        j.create_backup("corta"),
        Err(jarvis_core::orchestrator::CoreError::BadRequest(_))
    ));
    let second = Jarvis::new(config(&data));
    assert!(
        second.is_err_and(|e| e.contains("già usando")),
        "un solo Core per cartella dati"
    );
    drop(j);
    assert!(
        Jarvis::new(config(&data)).is_ok(),
        "il blocco si libera alla chiusura"
    );
}

#[tokio::test]
async fn backup_api_is_protected_audited_and_listed() {
    let base = root();
    let app = api::router(api::state(
        Jarvis::new(config(&base.join("data"))).unwrap(),
        "t".into(),
        18771,
    ));
    let call = |method: &str, uri: &str, body: Option<Value>, token: bool| {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "127.0.0.1:18771")
            .header("content-type", "application/json");
        if token {
            req = req.header("x-jarvis-token", "t");
        }
        req.body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap()
    };
    let res = app
        .clone()
        .oneshot(call(
            "POST",
            "/api/backup",
            Some(json!({"passphrase": PASS})),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = app
        .clone()
        .oneshot(call(
            "POST",
            "/api/backup",
            Some(json!({"passphrase": "corta"})),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = app
        .clone()
        .oneshot(call(
            "POST",
            "/api/backup",
            Some(json!({"passphrase": PASS})),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let summary: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        summary.get("path").is_none(),
        "l'API non espone percorsi assoluti"
    );
    let res = app
        .clone()
        .oneshot(call("GET", "/api/backups", None, true))
        .await
        .unwrap();
    let list: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list["items"][0]["file"], summary["file"]);
    for (file, passphrase, expected) in [
        (summary["file"].as_str().unwrap(), PASS, StatusCode::OK),
        (
            summary["file"].as_str().unwrap(),
            "passphrase errata",
            StatusCode::BAD_REQUEST,
        ),
        ("../altro.jbak", PASS, StatusCode::BAD_REQUEST),
    ] {
        let res = app
            .clone()
            .oneshot(call(
                "POST",
                "/api/backups/verify",
                Some(json!({"file": file, "passphrase": passphrase})),
                true,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), expected, "{file}");
    }

    let res = app
        .clone()
        .oneshot(call("GET", "/api/audit", None, true))
        .await
        .unwrap();
    let audit: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "backup.create"));
}
