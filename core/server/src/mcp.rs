//! Server MCP di JARVIS (ADR-0138): gli strumenti di sola lettura e calcolo del Core, usabili da altri agenti.
//!
//! Protocollo Model Context Protocol (JSON-RPC 2.0 su HTTP, risposte JSON): `initialize`, `ping`, `tools/list` e
//! `tools/call`. Passa dallo stesso token del proprietario delle altre API e ogni chiamata attraversa policy,
//! permessi del proprietario e audit come quelle dell'agente interno. Sono esposti solo strumenti senza effetti e
//! senza conferma: scritture, terminale, schermo, applicazioni, connettori e nodi restano dentro JARVIS.

use serde_json::{json, Value};

use crate::orchestrator::{CoreError, Jarvis};

pub const PROTOCOL: &str = "2025-03-26";

/// Nome nel protocollo: i punti diventano doppi trattini bassi (alcuni client accettano solo `[A-Za-z0-9_-]`).
pub fn wire_name(name: &str) -> String {
    name.replace('.', "__")
}

pub fn tool_name(wire: &str) -> String {
    wire.replace("__", ".")
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn error_text(error: CoreError) -> String {
    match error {
        CoreError::BadRequest(m)
        | CoreError::Unavailable(m)
        | CoreError::NotFound(m)
        | CoreError::Gone(m)
        | CoreError::Unauthorized(m)
        | CoreError::Internal(m) => m,
    }
}

/// Gestisce una richiesta JSON-RPC. Le notifiche (senza `id`) non hanno risposta.
pub fn handle(core: &mut Jarvis, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    if request["jsonrpc"] != "2.0" || !request["method"].is_string() {
        return Some(failure(&id, -32600, "Richiesta non valida"));
    }
    Some(match request["method"].as_str().unwrap_or_default() {
        "initialize" => reply(
            &id,
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "jarvis", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Strumenti di JARVIS in sola lettura e calcolo (calcoli aziendali, conversioni, data e ora, memoria, ricerca nei file della workspace). Le azioni con effetti restano dentro JARVIS e richiedono la conferma del proprietario.",
            }),
        ),
        "ping" => reply(&id, json!({})),
        "tools/list" => reply(&id, json!({"tools": core.mcp_tools()})),
        "tools/call" => {
            let params = &request["params"];
            let Some(name) = params["name"].as_str() else {
                return Some(failure(&id, -32602, "Nome dello strumento mancante"));
            };
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(value) if value.is_object() => value.clone(),
                Some(_) => {
                    return Some(failure(
                        &id,
                        -32602,
                        "Gli argomenti devono essere un oggetto",
                    ))
                }
            };
            match core.mcp_call(&tool_name(name), &arguments) {
                Ok((text, is_error)) => reply(
                    &id,
                    json!({"content": [{"type": "text", "text": text}], "isError": is_error}),
                ),
                Err(error) => failure(&id, -32603, &error_text(error)),
            }
        }
        _ => failure(&id, -32601, "Metodo non supportato"),
    })
}

/// Gestisce una richiesta singola o un lotto; `None` se erano solo notifiche.
pub fn handle_message(core: &mut Jarvis, message: &Value) -> Option<Value> {
    match message {
        Value::Array(items) if !items.is_empty() && items.len() <= 32 => {
            let replies: Vec<Value> = items.iter().filter_map(|item| handle(core, item)).collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        Value::Array(_) => Some(failure(&Value::Null, -32600, "Lotto vuoto o troppo grande")),
        other => handle(core, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_between_the_catalog_and_the_protocol() {
        assert_eq!(wire_name("biz.unit_economics"), "biz__unit_economics");
        assert_eq!(tool_name("biz__unit_economics"), "biz.unit_economics");
        assert_eq!(tool_name(&wire_name("time.now")), "time.now");
    }
}
