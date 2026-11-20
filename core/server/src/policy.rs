//! Security Engine del prototipo: decisione per operazione (`jarvis.policy-decision/1`).
//!
//! Regole, in ordine: strumento e versione nel catalogo; argomenti conformi
//! all'`input_schema`; percorsi lessicalmente dentro la workspace; conferma
//! obbligatoria per strumenti a rischio alto o critico; altrimenti consenso.
//! Policy assente o argomenti non validi negano l'azione (SECURITY_AND_PRIVACY.md).

use chrono::{Duration, Utc};
use jarvis_contracts::{validate, Contract};
use serde_json::{json, Value};

use crate::catalog::Catalog;
use crate::tools::lexically_inside;
use crate::web::{check_url, WebConfig};

pub const POLICY_VERSION: &str = "0.1.0";
pub const CONFIRMATION_TTL_SECS: i64 = 300;

/// Spiegazione leggibile di un motivo di policy.
/// Terminale disattivato dal proprietario (`JARVIS_SHELL=off`); attivo per impostazione, sempre con conferma.
fn shell_disabled() -> bool {
    std::env::var("JARVIS_SHELL").is_ok_and(|v| matches!(v.as_str(), "off" | "0" | "false"))
}

pub fn explain(reason: &str) -> &'static str {
    match reason {
        "policy.unknown_tool" => "strumento non presente nel catalogo autorizzato",
        "policy.invalid_arguments" => "argomenti non conformi al descrittore dello strumento",
        "policy.path_outside_workspace" => "il percorso è fuori dalla workspace di JARVIS",
        "policy.web_disabled" => "l'accesso al Web è disattivato (JARVIS_WEB=off)",
        "policy.private_address" => {
            "l'indirizzo è locale o privato: JARVIS legge solo pagine pubbliche"
        }
        "policy.url_not_allowed" => "il dominio non è consentito dalla configurazione Web",
        "policy.url_scheme_not_allowed" => "sono ammessi solo indirizzi http e https",
        "policy.url_with_credentials" => "l'indirizzo contiene credenziali",
        "policy.invalid_url" => "l'indirizzo non è valido",
        "policy.not_delegated" => {
            "lo strumento non rientra nell'ambito concesso all'agente per questa richiesta"
        }
        "policy.shell_disabled" => "il terminale è disattivato (JARVIS_SHELL=off)",
        "policy.command_blocked" => {
            "comando distruttivo o con privilegi di amministratore: JARVIS non lo esegue"
        }
        _ => "azione non autorizzata dalla policy",
    }
}

pub struct Request<'a> {
    /// Chi propone l'operazione (agente o modulo di automazione).
    pub subject: Value,
    pub task_id: &'a str,
    pub correlation_id: &'a str,
    pub tool: &'a str,
    pub version: &'a str,
    pub arguments: &'a Value,
    pub digest: &'a str,
}

fn resource_uri(tool: &str, args: &Value) -> String {
    let clean = |s: &str| -> String {
        let c: String = s
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || "._/-".contains(ch) {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let c = c.trim_matches('/').to_string();
        if c.is_empty() {
            "root".into()
        } else {
            c
        }
    };
    if let Some((slug, base)) = tool.strip_prefix("node.").and_then(|r| r.split_once('.')) {
        return match args["path"].as_str() {
            Some(p) if base.starts_with("fs.") => {
                format!("jarvis://node/{}/workspace/{}", clean(slug), clean(p))
            }
            _ => format!("jarvis://node/{}/tool/{}", clean(slug), clean(base)),
        };
    }
    match args["path"].as_str() {
        Some(p) if tool.starts_with("fs.") => format!("jarvis://file/workspace/{}", clean(p)),
        _ if tool == "shell.run" => "jarvis://terminal/workspace".to_string(),
        _ if matches!(tool, "web.fetch" | "web.browse" | "web.form") => {
            let host = args["url"]
                .as_str()
                .and_then(|u| url::Url::parse(u).ok())
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default();
            format!("jarvis://web/{}", clean(&host))
        }
        _ if tool.starts_with("connector.") => {
            format!(
                "jarvis://connector/{}",
                clean(&tool["connector.".len()..].replacen('.', "/", 1))
            )
        }
        _ => format!("jarvis://tool/{}", clean(tool)),
    }
}

fn connector_parameters(path: &str, value: &Value, rows: &mut Vec<String>) {
    match value {
        Value::Object(properties) if !properties.is_empty() => {
            for (key, value) in properties {
                let label = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path} › {key}")
                };
                connector_parameters(&label, value, rows);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (i, item) in items.iter().enumerate() {
                connector_parameters(&format!("{path} ({})", i + 1), item, rows);
            }
        }
        Value::Object(_) | Value::Array(_) if path.is_empty() => (),
        _ => {
            let text = match value {
                Value::Object(_) => "gruppo vuoto".to_owned(),
                Value::Array(_) => "lista vuota".to_owned(),
                _ => value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            };
            rows.push(format!("{path} = {text}"));
        }
    }
}

fn expected_effect(tool: &str, args: &Value) -> String {
    match tool {
        "web.form" => {
            let fields: Vec<String> = args["fields"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            let secret = ["pass", "pwd", "token", "secret", "card", "cvv", "iban"]
                                .iter()
                                .any(|s| k.to_lowercase().contains(s));
                            if secret { format!("{k}=•••") } else { format!("{k}={}", v.as_str().map_or_else(|| v.to_string(), str::to_string).chars().take(80).collect::<String>()) }
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "Compilare{} il modulo {} della pagina {} con: {}. Il sito riceverà questi dati.",
                if args["submit"] == false { "" } else { " e inviare" },
                args["form"].as_u64().unwrap_or(0),
                args["url"].as_str().unwrap_or("?"),
                fields.join(", ")
            )
        }
        "fs.move" => format!(
            "Spostare «{}» in «{}» nella workspace di JARVIS.",
            args["from"].as_str().unwrap_or("?"),
            args["to"].as_str().unwrap_or("?")
        ),
        "fs.delete" => format!(
            "Eliminare definitivamente «{}» dalla workspace di JARVIS (le cartelle solo se vuote).",
            args["path"].as_str().unwrap_or("?")
        ),
        "shell.run" => format!(
            "Eseguire nel terminale, nella workspace di JARVIS, il comando: «{}». Può modificare file e avviare programmi con i tuoi permessi; si interrompe dopo 60 secondi.",
            args["command"].as_str().unwrap_or("?")
        ),
        "fs.write" => {
            let content = args["content"].as_str().unwrap_or_default();
            let preview: String = content.chars().take(200).collect();
            let more = if content.chars().count() > 200 {
                "…"
            } else {
                ""
            };
            format!("Scrivere {} byte nel file '{}' della workspace (sovrascrive se esiste). Anteprima: «{preview}{more}»", content.len(), args["path"].as_str().unwrap_or("?"))
        }
        "workflow.create" => {
            let steps: Vec<String> = args["steps"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            format!("{} {}", x["tool"].as_str().unwrap_or("?"), x["arguments"])
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "Creare l'automazione «{}» ({}) che eseguirà in background, senza altre conferme: {}",
                args["name"].as_str().unwrap_or("?"),
                crate::automation::describe(&args["trigger"]),
                steps.join(" → ")
            )
        }
        "workflow.update" => {
            let mut parts = Vec::new();
            if let Some(n) = args["name"].as_str() {
                parts.push(format!("nome «{n}»"));
            }
            if args["trigger"].is_object() {
                parts.push(format!(
                    "frequenza {}",
                    crate::automation::describe(&args["trigger"])
                ));
            }
            if let Some(steps) = args["steps"].as_array() {
                let list: Vec<String> = steps
                    .iter()
                    .map(|x| format!("{} {}", x["tool"].as_str().unwrap_or("?"), x["arguments"]))
                    .collect();
                parts.push(format!("passi {}", list.join(" → ")));
            }
            if let Some(f) = args["on_failure"].as_str() {
                parts.push(format!("in caso di errore {f}"));
            }
            format!(
                "Modificare l'automazione {} (nuova versione, approvata ora e poi eseguita in background senza altre conferme): {}",
                args["id"].as_str().unwrap_or("?"),
                parts.join("; ")
            )
        }
        "workflow.delete" => format!(
            "Eliminare definitivamente l'automazione {}",
            args["id"].as_str().unwrap_or("?")
        ),
        "memory.forget" => format!(
            "Cancellare definitivamente l'elemento di memoria {}",
            args["id"].as_str().unwrap_or("?")
        ),
        t if t.starts_with("connector.") => {
            let mut rows = Vec::new();
            connector_parameters("", args, &mut rows);
            let values = rows.join("\n");
            let details = values.chars().take(810).collect::<String>();
            let suffix = if values.chars().count() > 810 { "… (anteprima abbreviata)" } else { "" };
            format!("Eseguire {t} sul servizio esterno.\n{details}{suffix}")
        },
        t if t.starts_with("node.") => {
            let (slug, base) = t["node.".len()..].split_once('.').unwrap_or(("?", t));
            let target = if slug == "auto" {
                "Sul nodo scelto automaticamente (il più libero fra quelli in linea)".to_string()
            } else {
                format!("Sul nodo «{slug}»")
            };
            format!(
                "{target}: {}",
                expected_effect(base, args).replace("della workspace", "della workspace del nodo")
            )
        }
        _ => format!("Eseguire {tool} con argomenti {args}"),
    }
}

pub fn decide(catalog: &Catalog, web: &WebConfig, req: &Request) -> Value {
    let now = Utc::now();
    let ts = |t: chrono::DateTime<Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let tool = catalog.get(req.tool).filter(|t| t.version() == req.version);
    let (decision, reasons, risk): (&str, Vec<&str>, String) = match tool {
        None => ("deny", vec!["policy.unknown_tool"], "high".into()),
        Some(t) if !t.input.is_valid(req.arguments) => {
            ("deny", vec!["policy.invalid_arguments"], t.risk().into())
        }
        Some(t)
            if req.arguments["path"]
                .as_str()
                .is_some_and(|p| !lexically_inside(p)) =>
        {
            (
                "deny",
                vec!["policy.path_outside_workspace"],
                t.risk().into(),
            )
        }
        Some(t)
            if t.name() == "fs.move"
                && [&req.arguments["from"], &req.arguments["to"]]
                    .iter()
                    .any(|p| p.as_str().is_some_and(|p| !lexically_inside(p))) =>
        {
            (
                "deny",
                vec!["policy.path_outside_workspace"],
                t.risk().into(),
            )
        }
        Some(t) if t.name() == "shell.run" && shell_disabled() => {
            ("deny", vec!["policy.shell_disabled"], t.risk().into())
        }
        Some(t)
            if t.name() == "shell.run"
                && crate::tools::command_blocked(
                    req.arguments["command"].as_str().unwrap_or_default(),
                ) =>
        {
            ("deny", vec!["policy.command_blocked"], t.risk().into())
        }
        // Web spento: anche gli strumenti con ambito Web fuori dal prefisso (es. il meteo, ADR-0046).
        Some(t)
            if (t.name().starts_with("web.") || t.descriptor["scope"] == "web") && !web.enabled =>
        {
            ("deny", vec!["policy.web_disabled"], t.risk().into())
        }
        Some(t)
            if matches!(t.name(), "web.fetch" | "web.browse" | "web.form")
                && check_url(web, req.arguments["url"].as_str().unwrap_or_default()).is_err() =>
        {
            (
                "deny",
                vec![
                    check_url(web, req.arguments["url"].as_str().unwrap_or_default()).unwrap_err(),
                ],
                t.risk().into(),
            )
        }
        Some(t) if t.needs_confirmation() || matches!(t.risk(), "high" | "critical") => (
            "require_confirmation",
            vec!["policy.owner_confirmation_required"],
            t.risk().into(),
        ),
        Some(t) => ("allow", vec!["policy.scope_match"], t.risk().into()),
    };
    // Lo schema vieta 'allow' con rischio alto: uno strumento sconosciuto resta 'deny'.
    let mut doc = json!({
        "contract": "jarvis.policy-decision/1",
        "decision_id": uuid::Uuid::new_v4().to_string(),
        "request": {
            "task_id": req.task_id,
            "correlation_id": req.correlation_id,
            "subject": req.subject,
            "on_behalf_of": {"kind": "owner", "id": "owner"},
            "action": {"tool": if tool.is_some() { req.tool } else { "unknown" }, "version": if tool.is_some() { req.version } else { "0.0.0" }},
            "resource": resource_uri(req.tool, req.arguments),
            "node": "core-local",
            "destination": if req.tool.starts_with("web.") || req.tool.starts_with("connector.") { "external" } else { "local" },
            "parameters_digest": req.digest,
        },
        "decision": decision,
        "reasons": reasons,
        "risk": risk,
        "policy_version": POLICY_VERSION,
        "decided_at": ts(now),
        "expires_at": ts(now + Duration::seconds(CONFIRMATION_TTL_SECS)),
    });
    if decision == "require_confirmation" {
        doc["confirmation"] = json!({
            "confirmation_id": uuid::Uuid::new_v4().to_string(),
            "parameters_digest": req.digest,
            "target": resource_uri(req.tool, req.arguments),
            "expected_effect": expected_effect(req.tool, req.arguments),
            "single_use": true,
            "expires_at": ts(now + Duration::seconds(CONFIRMATION_TTL_SECS)),
        });
    }
    debug_assert!(
        validate(Contract::PolicyDecision, &doc).is_valid(),
        "{:?}",
        validate(Contract::PolicyDecision, &doc)
    );
    doc
}
