//! Auto-miglioramento controllato (ADR-0163). JARVIS **osserva** ciò che è successo (piani riusciti, esiti degli
//! strumenti, prestazioni degli agenti) e **propone** miglioramenti; non cambia mai da solo il proprio codice, le
//! regole di sicurezza, i permessi o le chiavi. L'unica azione applicabile è salvare come procedura un piano che il
//! proprietario ha già eseguito più volte con successo, e solo dopo la sua approvazione. Tutto il resto è un
//! suggerimento da valutare. L'analisi è deterministica e non invia nulla a un modello.

use serde_json::{json, Value};
use std::collections::BTreeMap;

const MIN_REPEATS: usize = 3;
const MIN_RUNS: u64 = 5;
pub const MAX_DISMISSED: usize = 200;

fn signature(steps: &Value) -> String {
    steps
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["tool"].as_str())
        .collect::<Vec<_>>()
        .join(">")
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 32 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn short_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(text.as_bytes())[..4])
}

/// Elenco dei suggerimenti: `plans` dal più recente, `templates` già salvati, `dismissed` gli id scartati.
pub fn analyze(
    plans: &[Value],
    tool_stats: &[Value],
    agent_stats: &[Value],
    templates: &[Value],
    dismissed: &[String],
) -> Vec<Value> {
    let mut out = Vec::new();
    let known: Vec<String> = templates.iter().map(|t| signature(&t["steps"])).collect();
    let taken: Vec<&str> = templates
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    // Procedure ricorrenti: stessa sequenza di strumenti, almeno 2 passi, ripetuta con successo.
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for plan in plans.iter().filter(|p| p["status"] == "succeeded") {
        let sig = signature(&plan["steps"]);
        if sig.contains('>') {
            groups.entry(sig).or_default().push(plan);
        }
    }
    for (sig, group) in groups {
        if group.len() < MIN_REPEATS || known.contains(&sig) {
            continue;
        }
        let latest = group[0];
        let id = format!("proc-{}", short_hash(&sig));
        let mut name = slug(latest["goal"].as_str().unwrap_or_default());
        if name.is_empty() {
            name = format!("procedura-{}", short_hash(&sig));
        }
        while taken.contains(&name.as_str()) {
            name = format!("{}-2", name.chars().take(37).collect::<String>());
        }
        out.push(json!({
            "id": id, "kind": "save_procedure", "severity": "info",
            "title": format!("Salva «{}» come procedura", latest["goal"].as_str().unwrap_or_default()),
            "detail": format!("La stessa sequenza ({}) è riuscita {} volte: salvandola la ripeti con un gesto.", sig.replace('>', " → "), group.len()),
            "evidence": {"runs": group.len(), "tools": sig.split('>').collect::<Vec<_>>()},
            "action": {"type": "save_template", "plan_id": latest["id"], "name": name},
        }));
    }

    // Strumenti che falliscono spesso.
    for stat in tool_stats {
        let (runs, failures) = (
            stat["runs"].as_u64().unwrap_or(0),
            stat["failures"].as_u64().unwrap_or(0),
        );
        if runs >= MIN_RUNS && failures * 2 >= runs {
            let tool = stat["tool"].as_str().unwrap_or_default();
            out.push(json!({
                "id": format!("tool-{tool}"), "kind": "review_tool", "severity": "warning",
                "title": format!("Lo strumento «{tool}» fallisce spesso"),
                "detail": format!("{failures} errori su {runs} esecuzioni; ultimo errore: {}. Controlla permessi, configurazione o dipendenze.",
                    stat["last_error"].as_str().unwrap_or("non indicato")),
                "evidence": {"runs": runs, "failures": failures, "last_error": stat["last_error"]},
                "action": null,
            }));
        }
    }

    // Agenti con pochi successi.
    for stat in agent_stats {
        let runs = stat["runs"].as_u64().unwrap_or(0);
        let rate = stat["success_rate"].as_f64().unwrap_or(100.0);
        if runs >= MIN_RUNS && rate < 50.0 {
            let agent = stat["agent"].as_str().unwrap_or_default();
            out.push(json!({
                "id": format!("agent-{agent}"), "kind": "review_agent", "severity": "warning",
                "title": format!("L'agente «{agent}» riesce poco"),
                "detail": format!("{rate}% di successi su {runs} lavori: valuta di restringere il suo ambito o di cambiare modello."),
                "evidence": {"runs": runs, "success_rate": rate},
                "action": null,
            }));
        }
    }
    out.retain(|s| !dismissed.iter().any(|d| s["id"] == d.as_str()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(id: &str, goal: &str, tools: &[&str], status: &str) -> Value {
        json!({"id": id, "goal": goal, "status": status,
            "steps": tools.iter().enumerate().map(|(i, t)| json!({"id": format!("s{i}"), "tool": t})).collect::<Vec<_>>()})
    }

    #[test]
    fn repeated_successful_sequences_become_a_procedure_suggestion() {
        let plans = vec![
            plan(
                "p4",
                "Controllo del repository",
                &["git.status", "deps.list"],
                "succeeded",
            ),
            plan(
                "p3",
                "Controllo del repository",
                &["git.status", "deps.list"],
                "succeeded",
            ),
            plan("p2", "Altro", &["git.status", "deps.list"], "failed"),
            plan(
                "p1",
                "Controllo del repository",
                &["git.status", "deps.list"],
                "succeeded",
            ),
            plan("p0", "Singolo", &["math.calc"], "succeeded"),
        ];
        let found = analyze(&plans, &[], &[], &[], &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        let s = &found[0];
        assert_eq!(s["kind"], "save_procedure");
        assert_eq!(s["action"]["plan_id"], "p4", "usa il piano più recente");
        assert_eq!(s["action"]["name"], "controllo-del-repository");
        assert_eq!(s["evidence"]["runs"], 3);
        assert!(s["detail"]
            .as_str()
            .unwrap()
            .contains("git.status → deps.list"));
    }

    #[test]
    fn existing_templates_dismissals_and_short_plans_are_respected() {
        let plans: Vec<Value> = (0..4)
            .map(|i| {
                plan(
                    &format!("p{i}"),
                    "Giro",
                    &["git.status", "deps.list"],
                    "succeeded",
                )
            })
            .collect();
        let template =
            json!({"name": "giro", "steps": [{"tool": "git.status"}, {"tool": "deps.list"}]});
        assert!(
            analyze(&plans, &[], &[], &[template], &[]).is_empty(),
            "già salvata"
        );
        let first = analyze(&plans, &[], &[], &[], &[]);
        let id = first[0]["id"].as_str().unwrap().to_string();
        assert!(analyze(&plans, &[], &[], &[], &[id]).is_empty(), "scartata");
        let single: Vec<Value> = (0..5)
            .map(|i| plan(&format!("q{i}"), "Uno", &["math.calc"], "succeeded"))
            .collect();
        assert!(
            analyze(&single, &[], &[], &[], &[]).is_empty(),
            "un solo passo non è una procedura"
        );
        // Il nome proposto non collide con uno esistente.
        let taken = json!({"name": "giro", "steps": [{"tool": "altro"}]});
        let renamed = analyze(&plans, &[], &[], &[taken], &[]);
        assert_eq!(renamed[0]["action"]["name"], "giro-2");
    }

    #[test]
    fn unreliable_tools_and_agents_are_flagged_only_with_enough_evidence() {
        let tools = vec![
            json!({"tool": "web.fetch", "runs": 8, "failures": 5, "last_error": "policy.web_disabled"}),
            json!({"tool": "math.calc", "runs": 50, "failures": 2, "last_error": null}),
            json!({"tool": "nuovo", "runs": 3, "failures": 3, "last_error": "x"}),
        ];
        let agents = vec![
            json!({"agent": "coding", "runs": 6, "success_rate": 33.3}),
            json!({"agent": "ricerca", "runs": 2, "success_rate": 0.0}),
            json!({"agent": "generale", "runs": 20, "success_rate": 90.0}),
        ];
        let found = analyze(&[], &tools, &agents, &[], &[]);
        let ids: Vec<&str> = found.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["tool-web.fetch", "agent-coding"]);
        assert!(
            found.iter().all(|s| s["action"].is_null()),
            "nessuna azione automatica"
        );
        assert!(found[0]["detail"]
            .as_str()
            .unwrap()
            .contains("policy.web_disabled"));
    }
}
