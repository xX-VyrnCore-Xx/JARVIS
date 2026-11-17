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

pub const POLICY_VERSION: &str = "0.1.0";
pub const CONFIRMATION_TTL_SECS: i64 = 300;

/// Spiegazione leggibile di un motivo di policy.
pub fn explain(reason: &str) -> &'static str {
    match reason {
        "policy.unknown_tool" => "strumento non presente nel catalogo autorizzato",
        "policy.invalid_arguments" => "argomenti non conformi al descrittore dello strumento",
        "policy.path_outside_workspace" => "il percorso è fuori dalla workspace di JARVIS",
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
    match args["path"].as_str() {
        Some(p) if tool.starts_with("fs.") => format!("jarvis://file/workspace/{}", clean(p)),
        _ => format!("jarvis://tool/{}", clean(tool)),
    }
}

fn expected_effect(tool: &str, args: &Value) -> String {
    match tool {
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
        "memory.forget" => format!(
            "Cancellare definitivamente l'elemento di memoria {}",
            args["id"].as_str().unwrap_or("?")
        ),
        _ => format!("Eseguire {tool} con argomenti {args}"),
    }
}

pub fn decide(catalog: &Catalog, req: &Request) -> Value {
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
            "destination": "local",
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
