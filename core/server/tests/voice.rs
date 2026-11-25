//! Test di RF-09 sull'API vocale con i motori finti del worker (deterministici, senza modelli).

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jarvis_core::api;
use jarvis_core::config::Config;
use jarvis_core::orchestrator::Jarvis;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "t";
const PORT: u16 = 18769;

fn app(voice: &str) -> axum::Router {
    app_with(Some(voice.into()))
}

fn app_with(voice: Option<String>) -> axum::Router {
    let data = std::env::temp_dir().join(format!("jarvis-voice-{}", uuid::Uuid::new_v4().simple()));
    let cfg = Config {
        host: "127.0.0.1".parse().unwrap(),
        port: PORT,
        data_dir: data,
        python: std::env::var("JARVIS_PYTHON").unwrap_or_else(|_| "python3".into()),
        ai_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai"),
        delegation_timeout_ms: 30_000,
        voice_engine: voice,
        worker_env: vec![],
        scheduler_tick_s: 20,
        token: TOKEN.into(),
        web: jarvis_core::web::WebConfig::default(),
    };
    api::router(api::state(Jarvis::new(cfg).unwrap(), TOKEN.into(), PORT))
}

/// WAV PCM 16 bit mono con un tono e, se indicato, il chunk `jtxt` letto dal motore finto.
fn wav(seconds: f32, amplitude: f32, text: Option<&str>) -> Vec<u8> {
    let rate = 16_000u32;
    let samples: Vec<i16> = (0..(rate as f32 * seconds) as usize)
        .map(|i| {
            (amplitude * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin()) as i16
        })
        .collect();
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut extra = Vec::new();
    if let Some(t) = text {
        let mut payload = t.as_bytes().to_vec();
        let len = payload.len() as u32;
        if payload.len() % 2 == 1 {
            payload.push(0);
        }
        extra.extend_from_slice(b"jtxt");
        extra.extend_from_slice(&len.to_le_bytes());
        extra.extend_from_slice(&payload);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32 + 8 + extra.len() as u32 - 8).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out.extend_from_slice(&extra);
    out
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    ctype: &str,
    token: bool,
) -> (StatusCode, Vec<u8>, Option<String>) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", format!("127.0.0.1:{PORT}"))
        .header("content-type", ctype);
    if token {
        req = req.header("x-jarvis-token", TOKEN);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let voice = res
        .headers()
        .get("x-jarvis-voice")
        .map(|v| v.to_str().unwrap().to_string());
    (
        status,
        res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        voice,
    )
}

fn json_of(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn push_to_talk_transcription_is_local_and_conformant() {
    let app = app("fake");
    let (s, body, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        wav(1.2, 4000.0, Some("che ore sono")),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let t = json_of(&body);
    assert!(
        jarvis_contracts::validate(jarvis_contracts::Contract::VoiceTranscription, &t).is_valid()
    );
    assert_eq!(t["text"], "che ore sono");
    assert_eq!(t["engine"]["location"], "local");
    assert_eq!(t["duration_ms"], 1200);
    assert_eq!(
        t["wake"],
        json!({"mode": "push_to_talk", "detected": false})
    );
}

#[tokio::test]
async fn always_on_listening_returns_only_a_verdict() {
    let app = app("fake");
    for (text, expected) in [
        ("Ehi Jarvis, info sistema", true),
        ("parlavo d'altro", false),
    ] {
        let (status, body, _) = send(
            &app,
            "POST",
            "/api/voice/wake",
            wav(1.0, 4000.0, Some(text)),
            "audio/wav",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let out = json_of(&body);
        assert_eq!(out["detected"], expected, "{text}");
        // Nessun testo né comando: solo l'esito, il motore e il punteggio.
        let keys: Vec<_> = out.as_object().unwrap().keys().cloned().collect();
        assert!(
            keys.iter()
                .all(|k| ["detected", "engine", "score"].contains(&k.as_str())),
            "{keys:?}"
        );
        assert!(!String::from_utf8_lossy(&body).contains("sistema"));
    }
    let (status, _, _) = send(
        &app,
        "POST",
        "/api/voice/wake",
        Vec::new(),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wake_word_extracts_the_command() {
    let app = app("fake");
    let (_, body, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe?mode=wake_word",
        wav(1.0, 4000.0, Some("Ehi Jarvis, info sistema")),
        "audio/wav",
        true,
    )
    .await;
    let t = json_of(&body);
    assert_eq!(t["wake"]["detected"], true);
    assert_eq!(t["wake"]["command"], "info sistema");
    let (_, body, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe?mode=wake_word",
        wav(1.0, 4000.0, Some("parlavo d'altro")),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(
        json_of(&body)["wake"]["detected"],
        false,
        "senza parola di attivazione nessun comando"
    );
}

#[tokio::test]
async fn silence_invalid_and_oversized_audio() {
    let app = app("fake");
    let (s, body, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        wav(0.5, 0.0, None),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(
        (s, json_of(&body)["text"].as_str()),
        (StatusCode::OK, Some("")),
        "il silenzio non produce testo"
    );
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        b"non un wav".to_vec(),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        vec![0u8; 6 * 1024 * 1024],
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        wav(61.0, 1000.0, None),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "oltre 60 secondi");
}

#[tokio::test]
async fn speech_synthesis_returns_wav() {
    let app = app("fake");
    let (s, body, voice) = send(
        &app,
        "POST",
        "/api/voice/speak",
        json!({"text": "Sono le dieci."}).to_string().into_bytes(),
        "application/json",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(&body[..4], b"RIFF");
    assert_eq!(&body[8..12], b"WAVE");
    assert_eq!(voice.as_deref(), Some("fake/fake-voice"));
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/speak",
        json!({"text": "  "}).to_string().into_bytes(),
        "application/json",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn voice_off_reports_unavailable_and_health_explains_why() {
    let app = app("off");
    let (s, body, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        wav(1.0, 4000.0, Some("x")),
        "audio/wav",
        true,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(json_of(&body)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("disattivata"));
    let (_, body, _) = send(
        &app,
        "GET",
        "/api/health",
        vec![],
        "application/json",
        false,
    )
    .await;
    let health = json_of(&body);
    assert_eq!(health["ai"]["gateway"]["voice"]["available"], false);
}

#[tokio::test]
async fn voice_endpoints_require_the_session_token() {
    let app = app("fake");
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/transcribe",
        wav(1.0, 4000.0, Some("x")),
        "audio/wav",
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = send(
        &app,
        "POST",
        "/api/voice/speak",
        b"{\"text\":\"x\"}".to_vec(),
        "application/json",
        false,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn without_a_chosen_engine_the_voice_is_never_forced_off() {
    // ADR-0183: il Core non deve spegnere la voce da solo; senza motori installati dice cosa manca.
    let res = app_with(None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/wake")
                .header("host", format!("127.0.0.1:{PORT}"))
                .header("x-jarvis-token", TOKEN)
                .header("content-type", "audio/wav")
                .body(Body::from(wav(1.0, 8000.0, None)))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        !message.contains("JARVIS_VOICE_ENGINE=off") && !message.contains("disattivata"),
        "{status} {body}"
    );
}
