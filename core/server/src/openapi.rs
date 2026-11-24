//! Descrizione OpenAPI 3.1 dell'API locale (ADR-0146): serve a strumenti e agenti esterni per conoscere le rotte,
//! i permessi e il formato degli errori senza leggere il codice. Il documento è prodotto da una tabella unica, così
//! non può divergere dall'elenco dichiarato; un test verifica che le rotte in sola lettura esistano davvero.

use serde_json::{json, Map, Value};

/// Metodo, percorso, riassunto, gruppo, accesso (`o` = solo proprietario, `d` = anche dispositivi associati).
pub const ROUTES: &[(&str, &str, &str, &str, char)] = &[
    ("GET", "/api/health", "Stato del Core", "Sistema", 'd'),
    (
        "GET",
        "/api/system/stats",
        "CPU, memoria, disco e uptime",
        "Sistema",
        'o',
    ),
    ("GET", "/api/metrics", "Metriche operative", "Sistema", 'o'),
    ("GET", "/api/audit", "Registro di audit", "Sistema", 'o'),
    (
        "GET",
        "/api/openapi.json",
        "Questo documento",
        "Sistema",
        'o',
    ),
    (
        "POST",
        "/api/chat",
        "Chat con il motore locale",
        "Chat",
        'd',
    ),
    (
        "POST",
        "/api/chat/stream",
        "Chat locale in streaming (NDJSON)",
        "Chat",
        'd',
    ),
    (
        "POST",
        "/api/cloud/chat",
        "Chat con un modello cloud",
        "Chat",
        'd',
    ),
    (
        "POST",
        "/api/cloud/chat/stream",
        "Chat cloud in streaming (NDJSON)",
        "Chat",
        'd',
    ),
    (
        "POST",
        "/api/cloud/background",
        "Avvia un lavoro cloud in background (202)",
        "Chat",
        'o',
    ),
    (
        "GET",
        "/api/cloud/jobs",
        "Lavori cloud in background",
        "Chat",
        'o',
    ),
    (
        "GET",
        "/api/conversation",
        "Messaggi della chat attiva",
        "Chat",
        'd',
    ),
    (
        "GET",
        "/api/conversations",
        "Chat recenti e chat attiva",
        "Chat",
        'o',
    ),
    (
        "POST",
        "/api/conversations",
        "Apre una chat nuova",
        "Chat",
        'o',
    ),
    (
        "POST",
        "/api/conversations/select",
        "Rende attiva una chat",
        "Chat",
        'o',
    ),
    (
        "POST",
        "/api/conversations/delete",
        "Elimina una chat",
        "Chat",
        'o',
    ),
    (
        "POST",
        "/api/prompt/check",
        "Corregge un messaggio scritto",
        "Chat",
        'o',
    ),
    (
        "GET",
        "/api/confirmations",
        "Azioni in attesa di conferma",
        "Conferme",
        'd',
    ),
    ("GET", "/api/tasks", "Attività recenti", "Attività", 'd'),
    (
        "GET",
        "/api/tools",
        "Catalogo degli strumenti",
        "Strumenti",
        'o',
    ),
    ("GET", "/api/skills", "Competenze", "Strumenti", 'o'),
    (
        "POST",
        "/api/skills",
        "Crea una competenza",
        "Strumenti",
        'o',
    ),
    (
        "GET",
        "/api/memory",
        "Ricordi (filtro kind, ricerca q)",
        "Memoria",
        'o',
    ),
    (
        "GET",
        "/api/memory/stats",
        "Statistiche della memoria",
        "Memoria",
        'o',
    ),
    (
        "GET",
        "/api/memory/graph",
        "Grafo della conoscenza",
        "Memoria",
        'o',
    ),
    (
        "POST",
        "/api/memory/{id}/pin",
        "Fissa o libera un ricordo",
        "Memoria",
        'o',
    ),
    (
        "GET",
        "/api/profile",
        "Profilo del proprietario",
        "Profilo",
        'o',
    ),
    (
        "PATCH",
        "/api/profile",
        "Aggiorna il profilo",
        "Profilo",
        'o',
    ),
    ("GET", "/api/persona", "Personalità", "Profilo", 'o'),
    (
        "GET",
        "/api/missions",
        "Missioni autonome",
        "Autonomia",
        'o',
    ),
    (
        "POST",
        "/api/missions",
        "Crea una missione",
        "Autonomia",
        'o',
    ),
    ("GET", "/api/workflows", "Automazioni", "Autonomia", 'o'),
    (
        "GET",
        "/api/proactive",
        "Iniziative proattive",
        "Autonomia",
        'o',
    ),
    (
        "PATCH",
        "/api/proactive",
        "Aggiorna le iniziative",
        "Autonomia",
        'o',
    ),
    (
        "POST",
        "/api/proactive/briefing",
        "Briefing quotidiano adesso",
        "Autonomia",
        'o',
    ),
    (
        "POST",
        "/api/proactive/weekly",
        "Briefing settimanale adesso",
        "Autonomia",
        'o',
    ),
    ("GET", "/api/notifications", "Notifiche", "Notifiche", 'd'),
    (
        "GET",
        "/api/notifications/stream",
        "Notifiche in tempo reale (SSE)",
        "Notifiche",
        'd',
    ),
    (
        "GET",
        "/api/devices",
        "Dispositivi associati",
        "Dispositivi",
        'o',
    ),
    (
        "POST",
        "/api/devices/pairing",
        "Crea un invito monouso con collegamento",
        "Dispositivi",
        'o',
    ),
    (
        "POST",
        "/api/devices/pair",
        "Associa un dispositivo con il codice",
        "Dispositivi",
        'd',
    ),
    (
        "GET",
        "/api/devices/access",
        "Indirizzi pubblici del Core",
        "Dispositivi",
        'o',
    ),
    (
        "PUT",
        "/api/devices/access",
        "Aggiunge o rimuove un indirizzo",
        "Dispositivi",
        'o',
    ),
    (
        "GET",
        "/api/tailscale",
        "Stato del collegamento Tailscale",
        "Dispositivi",
        'o',
    ),
    (
        "POST",
        "/api/tailscale/connect",
        "Pubblica il Core nella rete Tailscale",
        "Dispositivi",
        'o',
    ),
    ("GET", "/api/connectors", "Connettori", "Integrazioni", 'o'),
    (
        "POST",
        "/api/mcp",
        "Server MCP (JSON-RPC 2.0)",
        "Integrazioni",
        'o',
    ),
    (
        "GET",
        "/api/cloud",
        "Provider cloud e impostazioni",
        "Integrazioni",
        'o',
    ),
    (
        "GET",
        "/api/voice/system",
        "Voce di sistema e ascolto",
        "Voce",
        'o',
    ),
    (
        "PATCH",
        "/api/voice/system",
        "Imposta lettura risposte e ascolto",
        "Voce",
        'o',
    ),
    (
        "POST",
        "/api/voice/wake",
        "Verdetto sulla parola di attivazione (WAV)",
        "Voce",
        'o',
    ),
    (
        "POST",
        "/api/voice/transcribe",
        "Trascrive un audio (WAV)",
        "Voce",
        'o',
    ),
    (
        "GET",
        "/api/updates",
        "Stato degli aggiornamenti",
        "Sistema",
        'o',
    ),
];

fn operation_id(method: &str, path: &str) -> String {
    let words: Vec<String> = path
        .trim_start_matches("/api/")
        .split(['/', '{', '}', '.'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect();
    format!("{}{}", method.to_lowercase(), words.join(""))
}

/// Documento OpenAPI 3.1.
pub fn document(version: &str) -> Value {
    let mut paths = Map::new();
    for (method, path, summary, tag, access) in ROUTES {
        let mut operation = json!({
            "summary": summary,
            "tags": [tag],
            "operationId": operation_id(method, path),
            "x-access": if *access == 'o' { "solo proprietario" } else { "proprietario e dispositivi associati" },
            "responses": {
                "200": {"description": "Esito riuscito"},
                "401": {"$ref": "#/components/responses/Errore"},
                "403": {"$ref": "#/components/responses/Errore"},
                "429": {"$ref": "#/components/responses/Errore"},
            },
        });
        if path.contains("{id}") {
            operation["parameters"] = json!([{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]);
        }
        if matches!(*method, "POST" | "PUT" | "PATCH") {
            operation["requestBody"] =
                json!({"content": {"application/json": {"schema": {"type": "object"}}}});
        }
        if *path == "/api/cloud/background" {
            operation["responses"]["202"] = json!({"description": "Lavoro avviato: segui l'esito con /api/cloud/jobs o con la notifica"});
        }
        paths
            .entry(path.to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("oggetto")
            .insert(method.to_lowercase(), operation);
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "JARVIS Core API",
            "version": version,
            "description": "API locale di JARVIS. Il Core ascolta su 127.0.0.1; i dispositivi remoti passano da un proxy fidato (Tailscale Serve). Ogni rotta applica identità, policy, conferme e audit. Gli errori hanno la forma {error: {code, message}}."
        },
        "servers": [{"url": "http://127.0.0.1:8765"}],
        "security": [{"owner": []}, {"bearer": []}, {"device": []}],
        "paths": Value::Object(paths),
        "components": {
            "securitySchemes": {
                "owner": {"type": "apiKey", "in": "header", "name": "X-Jarvis-Token"},
                "device": {"type": "apiKey", "in": "header", "name": "X-Jarvis-Device"},
                "bearer": {"type": "http", "scheme": "bearer"},
            },
            "responses": {
                "Errore": {
                    "description": "Errore in italiano semplice",
                    "content": {"application/json": {"schema": {
                        "type": "object",
                        "properties": {"error": {"type": "object", "properties": {
                            "code": {"type": "string"}, "message": {"type": "string"}}}},
                    }}},
                },
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_is_well_formed_and_ids_are_unique() {
        let doc = document("1.0.0");
        assert_eq!(doc["openapi"], "3.1.0");
        let mut ids = std::collections::HashSet::new();
        for (path, item) in doc["paths"].as_object().unwrap() {
            assert!(path.starts_with("/api/"));
            for operation in item.as_object().unwrap().values() {
                assert!(ids.insert(operation["operationId"].as_str().unwrap().to_string()));
            }
        }
        assert_eq!(ids.len(), ROUTES.len());
        assert_eq!(
            operation_id("POST", "/api/memory/{id}/pin"),
            "postMemoryIdPin"
        );
    }
}
