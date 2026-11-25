use super::*;

#[test]
fn event_stream_responses_are_read_when_finished() {
    let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
    assert_eq!(sse_response(body).unwrap()["result"]["tools"], json!([]));
    assert!(sse_response("data: non json\n\n").is_none());
    assert!(sse_response("").is_none());
}

#[test]
fn session_ids_are_restricted_to_visible_ascii() {
    assert_eq!(
        clean_session("6dabf7af-d213-4e99").as_deref(),
        Some("6dabf7af-d213-4e99")
    );
    for bad in ["", "con spazio", "a\r\nX-Evil: 1", "è", &"x".repeat(201)] {
        assert!(clean_session(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn addresses_and_names_are_restricted() {
    assert!(valid_url("https://mcp.esempio.org/mcp").is_ok());
    assert!(valid_url("http://127.0.0.1:9000/mcp").is_ok());
    assert!(valid_url("http://localhost/mcp").is_ok());
    for bad in [
        "http://esempio.org/mcp",
        "ftp://esempio.org",
        "https://utente:pw@esempio.org",
        "https://esempio.org/x#frammento",
        "non un indirizzo",
    ] {
        assert!(valid_url(bad).is_err(), "{bad}");
    }
    assert!(valid_name("meteo-1") && !valid_name("Meteo") && !valid_name("") && !valid_name("a b"));
}

#[test]
fn servers_are_stored_without_tokens_and_capped() {
    let dir = std::env::temp_dir().join(format!("jarvis-mcpc-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    add(&dir, "meteo", "https://mcp.esempio.org/mcp").unwrap();
    assert_eq!(
        url_of(&dir, "meteo").unwrap(),
        "https://mcp.esempio.org/mcp"
    );
    let shown = list(&dir, &|name| name == "meteo");
    assert_eq!(shown["items"][0]["has_token"], true);
    assert!(!std::fs::read_to_string(dir.join(FILE))
        .unwrap()
        .contains("token"));
    assert!(add(&dir, "Brutto", "https://x.org").is_err());
    assert!(add(&dir, "ok", "http://esempio.org").is_err());
    for i in 0..MAX_SERVERS {
        let _ = add(&dir, &format!("s{i}"), "https://x.org/mcp");
    }
    assert!(add(&dir, "uno-di-troppo", "https://x.org/mcp").is_err());
    assert!(remove(&dir, "meteo").unwrap());
    assert!(!remove(&dir, "meteo").unwrap());
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------------------- server di prova

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

type Reply = (u16, Vec<(String, String)>, String);

/// Richiesta ricevuta dal server di prova: corpo JSON, intestazione di sessione e numero d'ordine.
struct Seen {
    body: Value,
    session: Option<String>,
    n: usize,
}

/// Server HTTP minimo su loopback: una richiesta per connessione, risposta decisa dal gestore.
fn serve(handler: impl Fn(Seen) -> Reply + Send + Sync + 'static) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (handler, counter) = (handler.clone(), counter.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                let mut session = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if let Some(v) = line.to_lowercase().strip_prefix("mcp-session-id:") {
                        session = Some(v.trim().to_string());
                    }
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).ok();
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                let (status, headers, text) = handler(Seen {
                    body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                    session,
                    n,
                });
                let mut out = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", text.len());
                for (k, v) in headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                let mut stream = stream;
                stream.write_all(out.as_bytes()).ok();
                stream.write_all(text.as_bytes()).ok();
            });
        }
    });
    (url, hits)
}

fn ok(result: Value) -> Reply {
    (
        200,
        vec![],
        json!({"jsonrpc": "2.0", "id": 1, "result": result}).to_string(),
    )
}

fn init_reply(session: &str) -> Reply {
    (
        200,
        vec![("Mcp-Session-Id".into(), session.into())],
        json!({"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": PROTOCOL, "serverInfo": {"name": "prova"}}}).to_string(),
    )
}

fn fast() {
    BACKOFF_MS.store(1, Ordering::Relaxed);
}

fn tool(name: &str, read_only: bool, schema: Value) -> Value {
    json!({"name": name, "description": "d", "inputSchema": schema, "annotations": {"readOnlyHint": read_only}})
}

fn echo_schema() -> Value {
    json!({"type": "object", "required": ["query"], "properties": {
        "query": {"type": "string"}, "limit": {"type": "integer"}, "sort": {"type": "string", "enum": ["a", "b"]}}})
}

/// Server completo: sessione, elenco con uno strumento di sola lettura e uno no, chiamata che fa l'eco.
fn echo_server() -> (String, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let inits = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let (i, c) = (inits.clone(), calls.clone());
    let (url, _) = serve(move |seen| match seen.body["method"].as_str() {
        Some("initialize") => {
            i.fetch_add(1, Ordering::SeqCst);
            init_reply("S1")
        }
        Some("notifications/initialized") => (202, vec![], String::new()),
        Some("tools/list") => ok(
            json!({"tools": [tool("cerca", true, echo_schema()), tool("scrivi", false, json!({"type": "object"}))]}),
        ),
        Some("tools/call") => {
            c.fetch_add(1, Ordering::SeqCst);
            ok(
                json!({"content": [{"type": "text", "text": format!("eco {}", seen.body["params"]["arguments"]["query"])}]}),
            )
        }
        _ => (400, vec![], "{}".into()),
    });
    (url, inits, calls)
}

#[test]
fn the_session_is_opened_once_and_reused_across_calls() {
    fast();
    let (url, inits, calls) = echo_server();
    list_tools(&url, None).unwrap();
    for _ in 0..3 {
        let out = call_tool(&url, None, "cerca", &json!({"query": "rust"})).unwrap();
        assert_eq!(out["text"], "eco \"rust\"");
        assert_eq!(out["untrusted"], true);
        assert_eq!(out["read_only"], true);
    }
    assert_eq!(
        inits.load(Ordering::SeqCst),
        1,
        "una sola initialize per quattro richieste"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let stats = stats_of(&url);
    assert_eq!(stats["calls"], 4);
    assert_eq!(stats["failures"], 0);
    assert!(stats["last_ok"].is_string());
}

#[test]
fn a_forgotten_session_is_reopened_once_and_the_call_goes_through() {
    fast();
    let inits = Arc::new(AtomicUsize::new(0));
    let i = inits.clone();
    let alive = Arc::new(std::sync::Mutex::new("S1".to_string()));
    let a = alive.clone();
    let (url, _) = serve(move |seen| match seen.body["method"].as_str() {
        Some("initialize") => {
            let n = i.fetch_add(1, Ordering::SeqCst) + 1;
            let id = format!("S{n}");
            *a.lock().unwrap() = id.clone();
            init_reply(&id)
        }
        Some("notifications/initialized") => (202, vec![], String::new()),
        Some(m) => {
            if seen.session.as_deref().map(str::to_lowercase)
                != Some(a.lock().unwrap().to_lowercase())
            {
                return (404, vec![], "{}".into());
            }
            if m == "tools/list" {
                ok(json!({"tools": [tool("cerca", true, echo_schema())]}))
            } else {
                ok(json!({"content": [{"type": "text", "text": "fatto"}]}))
            }
        }
        None => (400, vec![], "{}".into()),
    });
    list_tools(&url, None).unwrap();
    // Il server dimentica la sessione (riavvio): la richiesta successiva riceve 404.
    *alive.lock().unwrap() = "altra".into();
    let out = call_tool(&url, None, "cerca", &json!({"query": "x"})).unwrap();
    assert_eq!(out["text"], "fatto");
    assert_eq!(
        inits.load(Ordering::SeqCst),
        2,
        "sessione riaperta una volta"
    );
}

#[test]
fn tool_lists_are_read_page_by_page_and_changes_are_noticed() {
    fast();
    let phase = Arc::new(AtomicUsize::new(0));
    let p = phase.clone();
    let (url, _) = serve(move |seen| match seen.body["method"].as_str() {
        Some("initialize") => init_reply("S"),
        Some("notifications/initialized") => (202, vec![], String::new()),
        Some("tools/list") => {
            let extra = p.load(Ordering::SeqCst);
            match seen.body["params"]["cursor"].as_str() {
                None => {
                    ok(json!({"tools": [tool("uno", true, json!({}))], "nextCursor": "pagina2"}))
                }
                Some("pagina2") => ok(
                    json!({"tools": [tool(if extra == 0 { "due" } else { "tre" }, true, json!({}))]}),
                ),
                _ => (400, vec![], "{}".into()),
            }
        }
        _ => (400, vec![], "{}".into()),
    });
    let first = list_tools(&url, None).unwrap();
    let names: Vec<&str> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["uno", "due"]);
    assert_eq!(first["more"], false);
    assert_eq!(first["changed"], false);
    assert_eq!(
        list_tools(&url, None).unwrap()["changed"],
        false,
        "stesso elenco, stessa impronta"
    );
    phase.store(1, Ordering::SeqCst);
    let second = list_tools(&url, None).unwrap();
    assert_eq!(second["changed"], true);
    assert_ne!(first["fingerprint"], second["fingerprint"]);
}

#[test]
fn only_busy_answers_are_retried_and_other_errors_are_reported_at_once() {
    fast();
    let (url, hits) = serve({
        move |seen| match seen.body["method"].as_str() {
            Some("initialize") if seen.n == 1 => {
                (503, vec![("Retry-After".into(), "0".into())], "{}".into())
            }
            Some("initialize") => init_reply("S"),
            Some("notifications/initialized") => (202, vec![], String::new()),
            Some("tools/list") => ok(json!({"tools": []})),
            _ => (400, vec![], "{}".into()),
        }
    });
    assert!(
        list_tools(&url, None).is_ok(),
        "un 503 passeggero si ripete"
    );
    assert!(hits.load(Ordering::SeqCst) >= 3);
    let (broken, broken_hits) = serve(|_| (500, vec![], "{}".into()));
    let err = list_tools(&broken, None).unwrap_err();
    assert!(err.contains("HTTP 500"), "{err}");
    assert_eq!(
        broken_hits.load(Ordering::SeqCst),
        1,
        "un 500 non si ripete"
    );
    // Un server spento: errore chiaro dopo un numero limitato di tentativi.
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/mcp", l.local_addr().unwrap())
    };
    let err = list_tools(&closed, None).unwrap_err();
    assert!(err.contains("non è raggiungibile"), "{err}");
}

#[test]
fn arguments_are_checked_against_the_server_schema_before_anything_is_sent() {
    fast();
    let (url, _inits, calls) = echo_server();
    list_tools(&url, None).unwrap();
    for (args, expected) in [
        (json!({}), "obbligatorio"),
        (json!({"query": 5}), "tipo sbagliato"),
        (json!({"query": "a", "sort": "z"}), "non ammesso"),
        (json!({"query": "a", "limit": "dieci"}), "tipo sbagliato"),
        (json!("non un oggetto"), "oggetto"),
    ] {
        let err = call_tool(&url, None, "cerca", &args).unwrap_err();
        assert!(err.contains(expected), "{args}: {err}");
    }
    assert!(call_tool(&url, None, "inesistente", &json!({}))
        .unwrap_err()
        .contains("non offre"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "nessuna chiamata è partita"
    );
    assert!(call_tool(
        &url,
        None,
        "cerca",
        &json!({"query": "ok", "limit": 3, "sort": "a"})
    )
    .is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn rich_results_are_rendered_without_images_and_with_structured_content() {
    let result = json!({
        "content": [
            {"type": "text", "text": "ciao"},
            {"type": "image", "mimeType": "image/png", "data": "QUJDRA=="},
            {"type": "resource_link", "uri": "https://esempio.org/doc"},
            {"type": "resource", "resource": {"uri": "file:///x", "text": "dentro"}},
            {"type": "strano"}
        ],
        "structuredContent": {"n": 2}
    });
    let text = render(&result);
    assert!(text.contains("ciao") && text.contains("dentro"));
    assert!(
        text.contains("[image image/png non incluso, 6 byte]"),
        "{text}"
    );
    assert!(text.contains("[collegamento: https://esempio.org/doc]"));
    assert!(text.contains("non supportato"));
    assert!(text.ends_with("{\"n\":2}"));
    assert!(
        !text.contains("QUJDRA"),
        "i dati dell'immagine non entrano nel testo"
    );
}

#[test]
fn the_owner_policy_limits_tools_and_only_read_only_ones_can_skip_confirmation() {
    fast();
    let dir = std::env::temp_dir().join(format!("jarvis-mcpp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let (url, _, _) = echo_server();
    add(&dir, "prova", &url).unwrap();
    // Senza l'elenco degli strumenti non si può dire cosa sia di sola lettura.
    let early = set_policy(&dir, "prova", &json!({"auto": ["cerca"]})).unwrap_err();
    assert!(early.contains("non ancora letto"), "{early}");
    list_tools(&url, None).unwrap();
    // «scrivi» non è dichiarato di sola lettura: non può saltare la conferma.
    let err = set_policy(&dir, "prova", &json!({"auto": ["scrivi"]})).unwrap_err();
    assert!(err.contains("sola lettura"), "{err}");
    assert!(set_policy(&dir, "prova", &json!({"auto": ["inesistente"]})).is_err());
    assert!(set_policy(&dir, "prova", &json!({"auto": "cerca"})).is_err());
    assert!(set_policy(&dir, "assente", &json!({})).is_err());
    set_policy(
        &dir,
        "prova",
        &json!({"auto": ["cerca"], "allow": ["cerca"]}),
    )
    .unwrap();
    assert_eq!(auto_allowed(&dir), ["prova/cerca"]);
    assert!(is_allowed(&dir, "prova", "cerca"));
    assert!(!is_allowed(&dir, "prova", "scrivi"));
    assert!(
        is_allowed(&dir, "altro", "qualsiasi"),
        "senza elenco, tutti"
    );
    // La politica sopravvive a un nuovo salvataggio dello stesso server.
    add(&dir, "prova", &url).unwrap();
    assert_eq!(list(&dir, &|_| false)["items"][0]["auto"], json!(["cerca"]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn health_reports_latency_tools_and_failures() {
    fast();
    let (url, _, _) = echo_server();
    let h = health(&url, None);
    assert_eq!(h["reachable"], true);
    assert_eq!(h["tools"], 2);
    assert_eq!(h["read_only_tools"], 1);
    assert!(h["latency_ms"].is_number());
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/mcp", l.local_addr().unwrap())
    };
    let bad = health(&closed, None);
    assert_eq!(bad["reachable"], false);
    assert!(bad["error"].as_str().unwrap().contains("raggiungibile"));
}

#[test]
fn known_servers_install_with_their_catalog_address_and_no_automatic_tools() {
    let dir = std::env::temp_dir().join(format!("jarvis-mcpk-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let catalog = presets();
    let hf = catalog["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "huggingface")
        .unwrap();
    assert_eq!(hf["url"], "https://huggingface.co/mcp");
    assert!(hf["suggested_auto"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "hub_repo_search"));
    assert!(hf["privacy"].as_str().unwrap().contains("partono"));
    assert!(add_preset(&dir, "sconosciuto", None).is_err());
    assert_eq!(
        add_preset(&dir, "huggingface", None).unwrap(),
        "huggingface"
    );
    let shown = list(&dir, &|_| false);
    assert_eq!(shown["items"][0]["url"], "https://huggingface.co/mcp");
    assert_eq!(shown["items"][0]["preset"], "huggingface");
    assert_eq!(
        shown["items"][0]["auto"],
        json!([]),
        "nessuno strumento senza conferma di fabbrica"
    );
    // Per un server noto gli strumenti consigliati valgono anche prima di leggere l'elenco; gli altri no.
    set_policy(
        &dir,
        "huggingface",
        &json!({"auto": ["hub_repo_search", "hub_repo_details"]}),
    )
    .unwrap();
    assert!(set_policy(
        &dir,
        "huggingface",
        &json!({"auto": ["strumento_che_scrive"]})
    )
    .is_err());
    assert_eq!(add_preset(&dir, "huggingface", Some("hf")).unwrap(), "hf");
    let _ = std::fs::remove_dir_all(dir);
}
