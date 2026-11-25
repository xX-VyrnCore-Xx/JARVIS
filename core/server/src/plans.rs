//! Piani di esecuzione (ADR-0152): attività complesse come grafo di passi, ciascuno uno strumento del catalogo.
//!
//! Questo modulo è la parte deterministica: validazione, ordine delle dipendenze, riferimenti fra passi
//! (`${passo.campo}`), stato complessivo e budget. L'esecuzione, che passa sempre da policy, conferme e audit,
//! sta in `orchestrator/plan_runner.rs`. Nessun ciclo illimitato: al massimo 20 passi, 2 ripetizioni per passo,
//! un budget di tempo e di passi eseguiti.

use serde_json::{json, Map, Value};

pub const MAX_STEPS: usize = 20;
pub const MAX_RETRIES: u64 = 2;
/// Passi indipendenti eseguiti insieme al massimo.
pub const MAX_PARALLEL: usize = 4;
pub const DEFAULT_SECONDS: u64 = 120;
pub const MAX_SECONDS: u64 = 900;
/// Dimensione massima del risultato di un passo conservato nel piano.
pub const OUTPUT_CHARS: usize = 4000;

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().trim().to_string()
}

/// Normalizza e valida la definizione di un piano. Non controlla che gli strumenti esistano: lo fa il Core.
pub fn validate(input: &Value) -> Result<Value, String> {
    let goal = text(input, "goal");
    if goal.is_empty() || goal.chars().count() > 300 {
        return Err("«goal» è obbligatorio (massimo 300 caratteri)".into());
    }
    let steps = input["steps"]
        .as_array()
        .ok_or("«steps» deve essere una lista")?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(format!("Un piano ha da 1 a {MAX_STEPS} passi"));
    }
    let mut out: Vec<Value> = Vec::new();
    for step in steps {
        let id = text(step, "id");
        if !valid_id(&id) {
            return Err(format!(
                "Identificativo di passo non valido: «{id}» (a-z, 0-9, - e _, massimo 32)"
            ));
        }
        if out.iter().any(|s| s["id"] == id.as_str()) {
            return Err(format!("Identificativo di passo duplicato: «{id}»"));
        }
        let tool = text(step, "tool");
        if tool.is_empty() || tool.len() > 64 {
            return Err(format!("Il passo «{id}» non indica lo strumento"));
        }
        let arguments = step.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err(format!(
                "Gli argomenti del passo «{id}» devono essere un oggetto"
            ));
        }
        let depends: Vec<String> = step["depends_on"]
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let retries = step["retries"].as_u64().unwrap_or(0);
        if retries > MAX_RETRIES {
            return Err(format!(
                "Il passo «{id}» ammette al massimo {MAX_RETRIES} ripetizioni"
            ));
        }
        let on_error = step["on_error"].as_str().unwrap_or("stop");
        if !matches!(on_error, "stop" | "continue") {
            return Err(format!("«on_error» del passo «{id}»: stop o continue"));
        }
        let undo = match step.get("undo").filter(|u| !u.is_null()) {
            None => Value::Null,
            Some(u) => {
                let undo_tool = text(u, "tool");
                if undo_tool.is_empty() || !u.get("arguments").is_none_or(Value::is_object) {
                    return Err(format!("L'annullamento del passo «{id}» non è valido"));
                }
                json!({"tool": undo_tool, "arguments": u.get("arguments").cloned().unwrap_or_else(|| json!({}))})
            }
        };
        let expect = match step.get("expect").filter(|e| !e.is_null()) {
            None => json!([]),
            Some(list) => validate_expect(list).map_err(|e| format!("Passo «{id}»: {e}"))?,
        };
        out.push(json!({
            "id": id, "tool": tool, "arguments": arguments, "depends_on": depends,
            "retries": retries, "on_error": on_error, "undo": undo, "expect": expect,
            "status": "pending", "attempts": 0, "output": Value::Null, "error": Value::Null,
        }));
    }
    for step in &out {
        for dep in step["depends_on"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if dep == step["id"] {
                return Err(format!("Il passo «{dep}» dipende da se stesso"));
            }
            if !out.iter().any(|s| s["id"] == dep) {
                return Err(format!(
                    "Il passo «{}» dipende da «{dep}», che non esiste",
                    step["id"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    if has_cycle(&out) {
        return Err("Le dipendenze formano un ciclo".into());
    }
    let seconds = input["budget"]["max_seconds"]
        .as_u64()
        .unwrap_or(DEFAULT_SECONDS)
        .clamp(1, MAX_SECONDS);
    let max_steps = input["budget"]["max_steps"]
        .as_u64()
        .unwrap_or(out.len() as u64 * (1 + MAX_RETRIES))
        .clamp(1, 80);
    let key = text(input, "idempotency_key");
    if key.len() > 64 {
        return Err("«idempotency_key» è lunga al massimo 64 caratteri".into());
    }
    Ok(json!({
        "goal": goal, "steps": out, "status": "pending",
        "budget": {"max_seconds": seconds, "max_steps": max_steps},
        "steps_executed": 0, "idempotency_key": if key.is_empty() { Value::Null } else { json!(key) },
        "events": [],
    }))
}

/// Ordinamento topologico (algoritmo di Kahn): vero se resta almeno un passo che non si può ordinare.
fn has_cycle(steps: &[Value]) -> bool {
    let mut done: Vec<&str> = Vec::new();
    loop {
        let before = done.len();
        for step in steps {
            let id = step["id"].as_str().unwrap_or_default();
            if !done.contains(&id)
                && step["depends_on"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .all(|d| done.contains(&d))
            {
                done.push(id);
            }
        }
        if done.len() == steps.len() {
            return false;
        }
        if done.len() == before {
            return true;
        }
    }
}

const EXPECT_OPS: [&str; 9] = [
    "exists",
    "not_empty",
    "eq",
    "ne",
    "gt",
    "ge",
    "lt",
    "le",
    "contains",
];

/// Condizioni di successo di un passo (validazione indipendente del risultato): `[{field, op, value}]`.
fn validate_expect(list: &Value) -> Result<Value, String> {
    let items = list.as_array().ok_or("«expect» deve essere una lista")?;
    if items.len() > 8 {
        return Err("al massimo 8 condizioni".into());
    }
    for item in items {
        let op = item["op"].as_str().unwrap_or_default();
        if !EXPECT_OPS.contains(&op) {
            return Err(format!(
                "operatore sconosciuto «{op}» ({})",
                EXPECT_OPS.join(", ")
            ));
        }
        if item["field"].as_str().is_none_or(str::is_empty) {
            return Err("ogni condizione indica «field»".into());
        }
        if !matches!(op, "exists" | "not_empty") && item.get("value").is_none() {
            return Err(format!("la condizione «{op}» richiede «value»"));
        }
    }
    Ok(list.clone())
}

/// Verifica le condizioni sul risultato di un passo; l'errore dice quale non è soddisfatta.
pub fn check_expect(output: &Value, expect: &Value) -> Result<(), String> {
    for item in expect.as_array().into_iter().flatten() {
        let field = item["field"].as_str().unwrap_or_default();
        let op = item["op"].as_str().unwrap_or_default();
        let actual = lookup(output, field);
        let wanted = &item["value"];
        let number = |v: &Value| v.as_f64();
        let ok = match (op, actual) {
            ("exists", found) => found.is_some(),
            (_, None) => false,
            ("not_empty", Some(v)) => match v {
                Value::Null => false,
                Value::String(s) => !s.is_empty(),
                Value::Array(a) => !a.is_empty(),
                Value::Object(o) => !o.is_empty(),
                _ => true,
            },
            ("eq", Some(v)) => v == wanted,
            ("ne", Some(v)) => v != wanted,
            ("gt", Some(v)) => number(v).zip(number(wanted)).is_some_and(|(a, b)| a > b),
            ("ge", Some(v)) => number(v).zip(number(wanted)).is_some_and(|(a, b)| a >= b),
            ("lt", Some(v)) => number(v).zip(number(wanted)).is_some_and(|(a, b)| a < b),
            ("le", Some(v)) => number(v).zip(number(wanted)).is_some_and(|(a, b)| a <= b),
            ("contains", Some(Value::String(s))) => wanted.as_str().is_some_and(|w| s.contains(w)),
            ("contains", Some(Value::Array(a))) => a.contains(wanted),
            _ => false,
        };
        if !ok {
            return Err(format!(
                "condizione non soddisfatta: «{field}» {op} {}",
                if wanted.is_null() {
                    String::new()
                } else {
                    wanted.to_string()
                }
            ));
        }
    }
    Ok(())
}

fn status_of<'a>(doc: &'a Value, id: &str) -> &'a str {
    doc["steps"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["id"] == id))
        .and_then(|s| s["status"].as_str())
        .unwrap_or("")
}

/// Indice del prossimo passo pronto: in attesa, con tutte le dipendenze concluse.
pub fn next_ready(doc: &Value) -> Option<usize> {
    doc["steps"].as_array()?.iter().position(|step| {
        step["status"] == "pending"
            && step["depends_on"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .all(|d| status_of(doc, d) == "done")
    })
}

/// Segna come saltati i passi che dipendono (anche indirettamente) da un passo fallito o saltato.
pub fn skip_blocked(doc: &mut Value) -> usize {
    let mut skipped = 0;
    loop {
        let blocked: Vec<usize> = doc["steps"]
            .as_array()
            .map(|steps| {
                steps
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s["status"] == "pending")
                    .filter(|(_, s)| {
                        s["depends_on"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .any(|d| {
                                matches!(status_of(doc, d), "failed" | "skipped" | "cancelled")
                            })
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        if blocked.is_empty() {
            return skipped;
        }
        for i in blocked {
            doc["steps"][i]["status"] = json!("skipped");
            skipped += 1;
        }
    }
}

/// Stato complessivo quando non c'è più nulla da eseguire.
pub fn final_status(doc: &Value) -> &'static str {
    let steps = doc["steps"].as_array().cloned().unwrap_or_default();
    if steps.iter().any(|s| s["status"] == "waiting_confirmation") {
        "waiting"
    } else if steps.iter().any(|s| s["status"] == "pending") {
        "running"
    } else if steps.iter().any(|s| s["status"] == "failed") {
        "failed"
    } else if steps
        .iter()
        .all(|s| matches!(s["status"].as_str(), Some("done" | "rolled_back")))
    {
        "succeeded"
    } else {
        "failed"
    }
}

fn lookup<'a>(output: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|p| !p.is_empty())
        .try_fold(output, |value, part| match part.parse::<usize>() {
            Ok(index) if value.is_array() => value.get(index),
            _ => value.get(part),
        })
}

fn reference(inner: &str, doc: &Value) -> Result<Value, String> {
    let (step_id, path) = inner.split_once('.').unwrap_or((inner, ""));
    let step = doc["steps"]
        .as_array()
        .and_then(|l| l.iter().find(|s| s["id"] == step_id))
        .ok_or_else(|| format!("Riferimento a un passo inesistente: «{step_id}»"))?;
    if step["status"] != "done" {
        return Err(format!("Il passo «{step_id}» non è concluso"));
    }
    lookup(&step["output"], path)
        .cloned()
        .ok_or_else(|| format!("Il passo «{step_id}» non ha il campo «{path}»"))
}

/// Sostituisce i riferimenti `"${passo.campo.sotto}"` con il risultato del passo (solo se la stringa è
/// interamente un riferimento, così il tipo del valore si conserva). Un riferimento a un passo non concluso o a un
/// campo assente è un errore.
pub fn resolve(arguments: &Value, doc: &Value) -> Result<Value, String> {
    match arguments {
        Value::String(s) => {
            // Riferimento unico: il valore conserva il proprio tipo.
            if let Some(inner) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
                if !inner.contains("${") && !inner.contains('}') {
                    return reference(inner, doc);
                }
            }
            if !s.contains("${") {
                return Ok(arguments.clone());
            }
            // Riferimenti dentro un testo: ognuno è sostituito dalla sua resa testuale.
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                out.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                let end = after.find('}').ok_or("Riferimento non chiuso: manca «}»")?;
                let value = reference(&after[..end], doc)?;
                match value {
                    Value::String(text) => out.push_str(&text),
                    other => out.push_str(&other.to_string()),
                }
                rest = &after[end + 1..];
            }
            out.push_str(rest);
            Ok(Value::String(out))
        }
        Value::Array(list) => list
            .iter()
            .map(|v| resolve(v, doc))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve(v, doc)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// Risultato ridotto per essere conservato nel piano.
pub fn clip_output(output: &Value) -> Value {
    let rendered = output.to_string();
    if rendered.chars().count() <= OUTPUT_CHARS {
        output.clone()
    } else {
        json!({"truncated": true, "text": rendered.chars().take(OUTPUT_CHARS).collect::<String>()})
    }
}

/// Copia del piano pronta per una nuova esecuzione (replay): stessi passi, nessun risultato.
pub fn reset(doc: &Value) -> Value {
    let mut copy = doc.clone();
    copy["status"] = json!("pending");
    copy["steps_executed"] = json!(0);
    copy["events"] = json!([]);
    copy["idempotency_key"] = Value::Null;
    for key in [
        "id",
        "started_at",
        "finished_at",
        "created_at",
        "updated_at",
    ] {
        if let Some(map) = copy.as_object_mut() {
            map.remove(key);
        }
    }
    if let Some(steps) = copy["steps"].as_array_mut() {
        for step in steps {
            step["status"] = json!("pending");
            step["attempts"] = json!(0);
            step["output"] = Value::Null;
            step["error"] = Value::Null;
            if let Some(map) = step.as_object_mut() {
                for key in ["approved", "started_at", "finished_at"] {
                    map.remove(key);
                }
            }
        }
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(steps: Value) -> Value {
        json!({"goal": "prova", "steps": steps})
    }

    #[test]
    fn a_valid_plan_is_normalised_with_a_budget() {
        let doc = validate(&plan(json!([
            {"id": "a", "tool": "time.now"},
            {"id": "b", "tool": "math.calc", "arguments": {"expression": "1+1"}, "depends_on": ["a"], "retries": 1}
        ])))
        .unwrap();
        assert_eq!(doc["status"], "pending");
        assert_eq!(doc["steps"][1]["on_error"], "stop");
        assert_eq!(doc["budget"]["max_seconds"], 120);
        assert_eq!(doc["budget"]["max_steps"], 6);
    }

    #[test]
    fn malformed_plans_are_rejected_with_a_reason() {
        for (steps, expected) in [
            (json!([]), "da 1 a"),
            (json!([{"id": "A", "tool": "x"}]), "non valido"),
            (
                json!([{"id": "a", "tool": "x"}, {"id": "a", "tool": "y"}]),
                "duplicato",
            ),
            (json!([{"id": "a", "tool": ""}]), "strumento"),
            (
                json!([{"id": "a", "tool": "x", "depends_on": ["z"]}]),
                "non esiste",
            ),
            (
                json!([{"id": "a", "tool": "x", "depends_on": ["a"]}]),
                "se stesso",
            ),
            (
                json!([{"id": "a", "tool": "x", "depends_on": ["b"]}, {"id": "b", "tool": "y", "depends_on": ["a"]}]),
                "ciclo",
            ),
            (
                json!([{"id": "a", "tool": "x", "retries": 9}]),
                "ripetizioni",
            ),
            (
                json!([{"id": "a", "tool": "x", "on_error": "ignora"}]),
                "on_error",
            ),
            (json!([{"id": "a", "tool": "x", "arguments": 3}]), "oggetto"),
        ] {
            let error = validate(&plan(steps.clone())).unwrap_err();
            assert!(error.contains(expected), "{steps} -> {error}");
        }
        assert!(validate(&json!({"goal": "", "steps": [{"id": "a", "tool": "x"}]})).is_err());
        let many: Vec<Value> = (0..21)
            .map(|i| json!({"id": format!("s{i}"), "tool": "time.now"}))
            .collect();
        assert!(validate(&plan(json!(many))).is_err());
    }

    #[test]
    fn steps_become_ready_when_their_dependencies_are_done() {
        let mut doc = validate(&plan(json!([
            {"id": "a", "tool": "time.now"},
            {"id": "b", "tool": "math.calc", "depends_on": ["a"]},
            {"id": "c", "tool": "time.now"}
        ])))
        .unwrap();
        assert_eq!(next_ready(&doc), Some(0));
        doc["steps"][0]["status"] = json!("done");
        assert_eq!(next_ready(&doc), Some(1));
        doc["steps"][1]["status"] = json!("done");
        assert_eq!(next_ready(&doc), Some(2));
        doc["steps"][2]["status"] = json!("done");
        assert_eq!(next_ready(&doc), None);
        assert_eq!(final_status(&doc), "succeeded");
    }

    #[test]
    fn a_failed_step_blocks_its_dependants_only() {
        let mut doc = validate(&plan(json!([
            {"id": "a", "tool": "x"},
            {"id": "b", "tool": "y", "depends_on": ["a"]},
            {"id": "c", "tool": "z", "depends_on": ["b"]},
            {"id": "d", "tool": "w"}
        ])))
        .unwrap();
        doc["steps"][0]["status"] = json!("failed");
        assert_eq!(skip_blocked(&mut doc), 2);
        assert_eq!(doc["steps"][1]["status"], "skipped");
        assert_eq!(doc["steps"][2]["status"], "skipped");
        assert_eq!(next_ready(&doc), Some(3));
        doc["steps"][3]["status"] = json!("done");
        assert_eq!(final_status(&doc), "failed");
    }

    #[test]
    fn references_carry_typed_values_between_steps() {
        let mut doc = validate(&plan(json!([
            {"id": "calc", "tool": "math.calc"},
            {"id": "next", "tool": "text.count", "depends_on": ["calc"]}
        ])))
        .unwrap();
        doc["steps"][0]["status"] = json!("done");
        doc["steps"][0]["output"] = json!({"result": 42, "list": [{"v": "x"}]});
        let args = resolve(&json!({"n": "${calc.result}", "t": "${calc.list.0.v}", "mixed": "risultato ${calc.result} per ${calc.list.0.v}!", "plain": "nessun riferimento", "all": ["${calc.result}"]}), &doc).unwrap();
        assert_eq!(
            args,
            json!({"n": 42, "t": "x", "mixed": "risultato 42 per x!", "plain": "nessun riferimento", "all": [42]})
        );
        assert!(resolve(&json!("aperto ${calc.result"), &doc)
            .unwrap_err()
            .contains("non chiuso"));
        assert!(resolve(&json!("testo ${non riferimento}"), &doc)
            .unwrap_err()
            .contains("inesistente"));
        assert!(resolve(&json!({"n": "${calc.assente}"}), &doc)
            .unwrap_err()
            .contains("campo"));
        assert!(resolve(&json!({"n": "${nessuno.x}"}), &doc)
            .unwrap_err()
            .contains("inesistente"));
        assert!(resolve(&json!({"n": "${next.x}"}), &doc)
            .unwrap_err()
            .contains("non è concluso"));
    }

    #[test]
    fn expectations_validate_the_result_of_a_step() {
        let ok = json!({"result": 42, "name": "alfa", "items": [1, 2], "empty": ""});
        for expect in [
            json!([{"field": "result", "op": "eq", "value": 42}]),
            json!([{"field": "result", "op": "gt", "value": 40}, {"field": "result", "op": "le", "value": 42}]),
            json!([{"field": "name", "op": "contains", "value": "lf"}, {"field": "items", "op": "contains", "value": 2}]),
            json!([{"field": "items", "op": "not_empty"}, {"field": "name", "op": "exists"}]),
            json!([]),
        ] {
            assert!(check_expect(&ok, &expect).is_ok(), "{expect}");
        }
        for expect in [
            json!([{"field": "result", "op": "eq", "value": 41}]),
            json!([{"field": "result", "op": "lt", "value": 10}]),
            json!([{"field": "assente", "op": "exists"}]),
            json!([{"field": "empty", "op": "not_empty"}]),
            json!([{"field": "name", "op": "ne", "value": "alfa"}]),
        ] {
            assert!(
                check_expect(&ok, &expect)
                    .unwrap_err()
                    .contains("non soddisfatta"),
                "{expect}"
            );
        }
        let bad = |expect: Value| {
            validate(&plan(json!([{"id": "a", "tool": "x", "expect": expect}]))).unwrap_err()
        };
        assert!(bad(json!([{"field": "a", "op": "boh"}])).contains("operatore"));
        assert!(bad(json!([{"op": "exists"}])).contains("field"));
        assert!(bad(json!([{"field": "a", "op": "eq"}])).contains("value"));
        assert!(bad(json!("x")).contains("lista"));
    }

    #[test]
    fn waiting_steps_pause_the_plan_and_replay_resets_everything() {
        let mut doc = validate(&plan(json!([{"id": "w", "tool": "fs.write"}]))).unwrap();
        doc["steps"][0]["status"] = json!("waiting_confirmation");
        assert_eq!(final_status(&doc), "waiting");
        doc["steps"][0]["status"] = json!("done");
        doc["steps"][0]["output"] = json!({"ok": true});
        doc["id"] = json!("p1");
        doc["steps_executed"] = json!(1);
        let copy = reset(&doc);
        assert_eq!(copy["steps"][0]["status"], "pending");
        assert!(copy["steps"][0]["output"].is_null());
        assert!(copy.get("id").is_none());
        assert_eq!(copy["steps_executed"], 0);
        assert_eq!(clip_output(&json!({"a": "b"})), json!({"a": "b"}));
        assert_eq!(clip_output(&json!("x".repeat(5000)))["truncated"], true);
    }
}
