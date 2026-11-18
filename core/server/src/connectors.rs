//! Integration Engine (RF-14): connettori dichiarativi installabili, autorizzabili e revocabili.
//!
//! Un manifest `jarvis.connector/1` descrive host, autenticazione e operazioni. Ogni operazione diventa uno
//! strumento `connector.<id>.<operazione>` del catalogo, presente solo finché il connettore è autorizzato,
//! quindi un connettore revocato viene negato dalla policy come strumento sconosciuto. Le scritture verso
//! l'esterno sono sempre a rischio alto, quindi con conferma. Le credenziali restano nell'archivio cifrato
//! dei segreti e vengono aggiunte alla richiesta solo dal Core.

use hmac::{Hmac, Mac};
use jarvis_contracts::{validate, Contract};
use serde_json::{json, Value};
use sha2::Sha256;
use url::Url;

use crate::tools::ToolError;
use crate::web::{check_url, WebConfig};

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn rank(risk: &str) -> u8 {
    match risk {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        _ => 3,
    }
}

/// Valida un manifest e ne restituisce la forma normalizzata (rischi effettivi delle operazioni).
pub fn validate_manifest(manifest: &Value, web: &WebConfig) -> Result<Value, String> {
    let report = validate(Contract::Connector, manifest);
    if !report.is_valid() {
        return Err(format!(
            "manifest non conforme: {}",
            report.schema.join("; ")
        ));
    }
    let base = Url::parse(manifest["base_url"].as_str().unwrap_or_default())
        .map_err(|_| "base_url non valido".to_string())?;
    let host = base.host_str().unwrap_or_default().to_ascii_lowercase();
    let hosts: Vec<String> = manifest["allowed_hosts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str().map(str::to_string))
        .collect();
    if !hosts.contains(&host) {
        return Err(format!(
            "l'host di base_url ({host}) deve comparire in allowed_hosts"
        ));
    }
    check_url(web, base.as_str())
        .map_err(|r| format!("base_url non ammesso dalla policy Web ({r})"))?;
    if manifest["auth"]["type"] == "oauth2" {
        // OAuth (ADR-0027): endpoint in HTTPS (HTTP solo su loopback con la policy di prova) e token
        // richiesti solo a un host dichiarato.
        for key in ["authorize_url", "token_url"] {
            let u = Url::parse(manifest["auth"][key].as_str().unwrap_or_default())
                .map_err(|_| format!("auth.{key} non valido"))?;
            let loopback = matches!(u.host_str(), Some("127.0.0.1" | "localhost"));
            if u.scheme() != "https" && !(web.allow_private && loopback) {
                return Err(format!("auth.{key} deve usare HTTPS"));
            }
        }
        let token = Url::parse(manifest["auth"]["token_url"].as_str().unwrap_or_default())
            .map_err(|_| "auth.token_url non valido".to_string())?;
        let token_host = token.host_str().unwrap_or_default().to_ascii_lowercase();
        if !hosts.contains(&token_host) {
            return Err(format!(
                "l'host di auth.token_url ({token_host}) deve comparire in allowed_hosts"
            ));
        }
    }
    let mut normalized = manifest.clone();
    let mut seen = std::collections::BTreeSet::new();
    for op in normalized["operations"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        let name = op["name"].as_str().unwrap_or_default().to_string();
        if !seen.insert(name.clone()) {
            return Err(format!("operazione duplicata: {name}"));
        }
        let schema = &op["input_schema"];
        if schema["type"] != "object" || schema["additionalProperties"] != false {
            return Err(format!(
                "{name}: input_schema deve essere un oggetto chiuso (additionalProperties: false)"
            ));
        }
        jsonschema::validator_for(schema)
            .map_err(|e| format!("{name}: input_schema non valido: {e}"))?;
        let path = op["path"].as_str().unwrap_or_default();
        for segment in path.split('{').skip(1) {
            let var = segment.split('}').next().unwrap_or_default();
            if !schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|x| x == var))
            {
                return Err(format!("{name}: il parametro di percorso {{{var}}} deve essere obbligatorio nell'input_schema"));
            }
        }
        let declared = op["risk"].as_str().unwrap_or("medium");
        let floor = if op["effects"] == "write_external" {
            "high"
        } else {
            "low"
        };
        op["risk"] = if rank(declared) >= rank(floor) {
            declared
        } else {
            floor
        }
        .into();
    }
    Ok(normalized)
}

pub fn tool_name(id: &str, op: &str) -> String {
    format!("connector.{id}.{op}")
}

/// Descrittori `jarvis.tool/1` delle operazioni (registrati nel catalogo solo se il connettore è autorizzato).
pub fn tool_descriptors(manifest: &Value) -> Vec<Value> {
    let id = manifest["id"].as_str().unwrap_or_default();
    manifest["operations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|op| {
            let write = op["effects"] == "write_external";
            let risk = op["risk"].as_str().unwrap_or("medium");
            json!({
                "contract": "jarvis.tool/1",
                "name": tool_name(id, op["name"].as_str().unwrap_or_default()),
                "version": manifest["version"],
                "summary": format!("[{}] {}", manifest["name"].as_str().unwrap_or(id), op["summary"].as_str().unwrap_or("")),
                "input_schema": op["input_schema"],
                "output_schema": {"type": "object"},
                "scope": "integration",
                "prerequisites": [format!("Connettore {id} autorizzato dal proprietario")],
                "effects": if write { "write_external" } else { "read" },
                "risk": risk,
                "permissions": [if write { "integration.connectors:write" } else { "integration.connectors:read" }],
                "errors": ["connector.http_error", "connector.server_error", "connector.not_authorized", "web.timeout", "web.unreachable"],
                "timeout_ms": 20000,
                "retry": if write { json!({"mode": "never"}) } else { json!({"mode": "safe", "max_attempts": 2}) },
                "confirmation": if matches!(risk, "high" | "critical") { "required" } else { "none" },
            })
        })
        .collect()
}

fn encode_segment(value: &Value) -> String {
    let raw = match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    url::form_urlencoded::byte_serialize(raw.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// Esegue un'operazione: percorso con parametri codificati, resto degli argomenti in query (GET) o JSON (POST).
pub fn call(
    manifest: &Value,
    op_name: &str,
    args: &Value,
    secret: Option<&str>,
    web: &WebConfig,
) -> Result<Value, ToolError> {
    let op = manifest["operations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["name"] == op_name)
        .ok_or_else(|| {
            err(
                "connector.unknown_operation",
                format!("operazione sconosciuta: {op_name}"),
            )
        })?;
    let mut rest = args.as_object().cloned().unwrap_or_default();
    let mut path = op["path"].as_str().unwrap_or("/").to_string();
    while let Some(start) = path.find('{') {
        let end = path[start..]
            .find('}')
            .map(|e| start + e)
            .ok_or_else(|| err("connector.invalid", "percorso malformato"))?;
        let var = path[start + 1..end].to_string();
        let value = rest
            .remove(&var)
            .ok_or_else(|| err("connector.invalid", format!("manca il parametro {var}")))?;
        path.replace_range(start..=end, &encode_segment(&value));
    }
    let base = manifest["base_url"]
        .as_str()
        .unwrap_or_default()
        .trim_end_matches('/');
    let mut url = Url::parse(&format!("{base}{path}"))
        .map_err(|_| err("connector.invalid", "URL non valido"))?;
    let method = op["method"].as_str().unwrap_or("GET");
    let body = if method == "GET" {
        for (k, v) in &rest {
            url.query_pairs_mut().append_pair(
                k,
                &match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
            );
        }
        None
    } else {
        Some(serde_json::to_vec(&Value::Object(rest)).unwrap_or_default())
    };
    let mut headers = vec![(
        "Accept".to_string(),
        "application/json, text/plain;q=0.8".to_string(),
    )];
    match (manifest["auth"]["type"].as_str(), secret) {
        (Some("bearer" | "oauth2"), Some(s)) => {
            headers.push(("Authorization".into(), format!("Bearer {s}")))
        }
        (Some("header"), Some(s)) => headers.push((
            manifest["auth"]["header_name"]
                .as_str()
                .unwrap_or("X-Api-Key")
                .to_string(),
            s.to_string(),
        )),
        (Some("none"), _) => {}
        _ => {
            return Err(err(
                "connector.not_authorized",
                "credenziale assente: autorizza il connettore",
            ))
        }
    }
    let hosts: Vec<String> = manifest["allowed_hosts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str().map(str::to_string))
        .collect();
    let (status, ctype, data) =
        crate::web::request(web, method, &url, &headers, body.as_deref(), &hosts)?;
    if status >= 500 {
        return Err(err(
            "connector.server_error",
            format!("{} ha risposto {status}", url.host_str().unwrap_or("?")),
        ));
    }
    if status >= 400 {
        return Err(err(
            "connector.http_error",
            format!("{} ha risposto {status}", url.host_str().unwrap_or("?")),
        ));
    }
    let content = if ctype.contains("json") {
        serde_json::from_slice::<Value>(&data).unwrap_or_else(|_| {
            Value::String(
                String::from_utf8_lossy(&data)
                    .chars()
                    .take(20_000)
                    .collect(),
            )
        })
    } else {
        Value::String(
            String::from_utf8_lossy(&data)
                .chars()
                .take(20_000)
                .collect(),
        )
    };
    Ok(json!({
        "connector": manifest["id"], "operation": op_name, "status": status, "data": content,
        "retrieved_at": crate::now(), "untrusted": true,
    }))
}

/// Errori transitori: una lettura può essere ripetuta, una scrittura no (potrebbe essere già avvenuta).
pub fn transient(code: &str) -> bool {
    matches!(
        code,
        "web.timeout" | "web.unreachable" | "connector.server_error"
    )
}

/// Verifica la firma `sha256=<hex>` di un webhook (confronto a tempo costante).
pub fn verify_signature(secret: &str, body: &[u8], header: Option<&str>) -> bool {
    let Some(hex_sig) = header.and_then(|h| h.strip_prefix("sha256=")) else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).expect("chiave HMAC");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).expect("chiave HMAC");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        json!({"contract": "jarvis.connector/1", "id": "meteo", "name": "Meteo", "version": "1.0.0", "description": "x",
               "base_url": "https://api.meteo.example/v1", "allowed_hosts": ["api.meteo.example"], "auth": {"type": "bearer"},
               "operations": [
                 {"name": "previsioni", "summary": "s", "method": "GET", "path": "/f/{citta}", "effects": "read",
                  "input_schema": {"type": "object", "additionalProperties": false, "required": ["citta"], "properties": {"citta": {"type": "string"}}}},
                 {"name": "segnala", "summary": "s", "method": "POST", "path": "/r", "effects": "write_external", "risk": "low",
                  "input_schema": {"type": "object", "additionalProperties": false, "properties": {"testo": {"type": "string"}}}}]})
    }

    #[test]
    fn writes_are_always_high_risk_with_confirmation() {
        let m = validate_manifest(&manifest(), &WebConfig::default()).unwrap();
        let tools = tool_descriptors(&m);
        assert_eq!(
            tools[1]["risk"], "high",
            "rischio dichiarato 'low' alzato per una scrittura esterna"
        );
        assert_eq!(tools[1]["confirmation"], "required");
        assert_eq!(tools[0]["confirmation"], "none");
        for t in &tools {
            assert!(validate(Contract::ToolDescriptor, t).is_valid(), "{t}");
        }
    }

    #[test]
    fn manifest_rules() {
        let web = WebConfig::default();
        let mut m = manifest();
        m["allowed_hosts"] = json!(["altro.example"]);
        assert!(validate_manifest(&m, &web)
            .unwrap_err()
            .contains("allowed_hosts"));
        let mut m = manifest();
        m["base_url"] = json!("http://127.0.0.1:9000");
        m["allowed_hosts"] = json!(["127.0.0.1"]);
        assert!(
            validate_manifest(&m, &web)
                .unwrap_err()
                .contains("policy Web"),
            "niente indirizzi privati"
        );
        let mut m = manifest();
        m["operations"][0]["input_schema"]["additionalProperties"] = json!(true);
        assert!(validate_manifest(&m, &web).is_err());
        let mut m = manifest();
        m["operations"][0]["input_schema"]["required"] = json!([]);
        assert!(validate_manifest(&m, &web)
            .unwrap_err()
            .contains("percorso"));
    }

    #[test]
    fn hmac_signatures() {
        let sig = sign("segreto", b"{\"a\":1}");
        assert!(verify_signature("segreto", b"{\"a\":1}", Some(&sig)));
        assert!(!verify_signature("segreto", b"{\"a\":2}", Some(&sig)));
        assert!(!verify_signature("altro", b"{\"a\":1}", Some(&sig)));
        assert!(!verify_signature("segreto", b"{\"a\":1}", None));
    }
}
