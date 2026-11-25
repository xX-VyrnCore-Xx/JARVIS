//! Collaborazione fra specialisti (ADR-0177): una richiesta con più ambiti diversi («controlla la CPU e poi calcola
//! l'IVA…») viene divisa e ogni parte va allo specialista giusto, **una dopo l'altra**. Le parti condividono un
//! contesto (gli esiti di quelle già fatte, come dati e non come istruzioni), nessuna scrittura avviene in parallelo
//! (così non ci sono conflitti sullo stesso elemento), uno specialista che fallisce cede la parte al generalista e il
//! risultato è un'unica risposta con l'esito di ciascuna parte. Ogni parte passa da policy, conferme e audit come
//! una richiesta a sé; se una parte chiede una conferma, le successive non partono finché il proprietario non decide.

use std::collections::BTreeSet;

use super::*;

/// Parti al massimo per richiesta: oltre, la richiesta resta al generalista.
const MAX_PARTS: usize = 4;

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

impl Jarvis {
    /// Le parti della richiesta con lo specialista di ciascuna, se toccano **almeno due specialisti diversi**.
    pub(super) fn collab_parts(&self, message: &str) -> Option<Vec<(String, &'static str)>> {
        let parts = crate::intent::split_intents(message);
        if parts.len() < 2 || parts.len() > MAX_PARTS {
            return None;
        }
        let routed: Vec<(String, &'static str)> = parts
            .into_iter()
            .map(|part| {
                let agent = self
                    .route_agent(&part)
                    .map_or(crate::agents::GENERAL, |a| a.name);
                (part, agent)
            })
            .collect();
        let specialists: BTreeSet<&str> = routed
            .iter()
            .map(|(_, a)| *a)
            .filter(|a| *a != crate::agents::GENERAL)
            .collect();
        (specialists.len() >= 2).then_some(routed)
    }

    pub(super) fn collaborate(
        &mut self,
        message: &str,
        routed: Vec<(String, &'static str)>,
    ) -> Result<Outcome, CoreError> {
        let mut task = Task::new(message, json!({"kind": "owner", "id": "owner"}));
        task.transition(
            "planned",
            "Richiesta con più ambiti: collaborazione fra specialisti",
        );
        task.transition("running", "Specialisti in esecuzione, uno dopo l'altro");
        self.in_collab = true;
        let mut out = Outcome::default();
        let mut lines: Vec<String> = Vec::new();
        let mut summaries: Vec<String> = Vec::new();
        let mut waiting_at: Option<usize> = None;
        for (i, (part, agent)) in routed.iter().enumerate() {
            self.shared_context = (!summaries.is_empty()).then(|| summaries.join("\n"));
            let failed = |r: &Result<Outcome, CoreError>| match r {
                Ok(o) => o.task["state"] == "failed",
                Err(_) => true,
            };
            let mut result = self.chat_inner(part);
            let mut used = *agent;
            let mut fallback = false;
            if failed(&result) && *agent != crate::agents::GENERAL {
                // Ripiego: la parte passa al generalista, con le stesse policy e conferme.
                self.force_general = true;
                result = self.chat_inner(part);
                self.force_general = false;
                used = crate::agents::GENERAL;
                fallback = true;
            }
            match result {
                Ok(part_out) => {
                    let reply = part_out.reply.clone().unwrap_or_default();
                    out.steps.push(json!({
                        "kind": "collaboration", "part": i + 1, "text": clip(part, 120),
                        "agent": used, "requested_agent": agent, "fallback": fallback,
                        "state": part_out.task["state"],
                    }));
                    out.sources.extend(part_out.sources.clone());
                    out.backend = out.backend.take().or(part_out.backend.clone());
                    if part_out.confirmation.is_some() {
                        out.confirmation = part_out.confirmation.clone();
                        lines.push(format!(
                            "**{}. {used}** — chiede la tua conferma prima di proseguire.",
                            i + 1
                        ));
                        waiting_at = Some(i);
                        break;
                    }
                    summaries.push(format!("{used}: {}", clip(&reply, 300)));
                    lines.push(format!(
                        "**{}. {used}**{} — {}",
                        i + 1,
                        if fallback { " (ripiego)" } else { "" },
                        reply
                    ));
                }
                Err(error) => {
                    let text = match error {
                        CoreError::BadRequest(m)
                        | CoreError::Unavailable(m)
                        | CoreError::NotFound(m)
                        | CoreError::Gone(m)
                        | CoreError::Unauthorized(m)
                        | CoreError::Internal(m) => m,
                    };
                    out.steps.push(json!({
                        "kind": "collaboration", "part": i + 1, "text": clip(part, 120),
                        "agent": used, "requested_agent": agent, "fallback": fallback, "state": "error",
                    }));
                    lines.push(format!("**{}. {used}** — non completata: {text}", i + 1));
                }
            }
        }
        self.in_collab = false;
        self.shared_context = None;
        self.force_general = false;
        let mut text = format!(
            "Richiesta con {} parti, svolta con {} specialisti in ordine:\n{}",
            routed.len(),
            routed
                .iter()
                .map(|(_, a)| *a)
                .collect::<BTreeSet<_>>()
                .len(),
            lines.join("\n")
        );
        if let Some(i) = waiting_at {
            let rest: Vec<String> = routed[i + 1..].iter().map(|(p, _)| clip(p, 80)).collect();
            if !rest.is_empty() {
                text.push_str(&format!(
                    "\nParti non avviate (dopo la conferma scrivimi di proseguire): {}.",
                    rest.join("; ")
                ));
            }
        }
        task.transition(
            "succeeded",
            if waiting_at.is_some() {
                "Collaborazione sospesa in attesa di conferma"
            } else {
                "Collaborazione conclusa"
            },
        );
        self.finish(task, out, text)
    }
}
