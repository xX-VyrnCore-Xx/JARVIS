//! Visibilità dei connettori (ADR-0220): quali strumenti dei servizi collegati l'agente **vede** per una richiesta.
//!
//! Il problema: gli strumenti di un connettore (`connector.gmail.cerca`, `connector.google_calendar.eventi`…) sono
//! offerti al modello solo se rientrano nell'elenco inviato con la delega, che ha un limite. Con l'ordinamento per
//! nome del connettore, un connettore con molte operazioni (GitHub ne ha 25) poteva occupare tutto lo spazio e
//! Gmail o Calendar non venivano nemmeno proposti; chi parlava di «email» invece di «gmail» non li nominava.
//!
//! Qui l'ordine è per **pertinenza** (nome, sinonimi italiani, parole dell'operazione e del sommario) e la scelta è
//! **equa fra connettori**: prima l'operazione più pertinente di ciascuno, poi i posti restanti per pertinenza, così
//! nessun servizio viene escluso da un altro e una richiesta specifica trova comunque tutte le sue operazioni.
//! Non cambia permessi: un'operazione che scrive all'esterno chiede comunque la conferma.

/// Un'operazione di connettore candidata.
pub struct Candidate<'a> {
    /// Nome completo dello strumento, `connector.<id>.<operazione>`.
    pub name: &'a str,
    pub summary: &'a str,
}

/// Radici di parola che indicano un connettore anche senza nominarlo.
fn aliases(namespace: &str) -> &'static [&'static str] {
    match namespace {
        "gmail" => &[
            "email", "mail", "e mail", "post", "messagg", "inbox", "casella", "mittent", "allegat",
            "bozza", "bozze",
        ],
        "google_calendar" => &[
            "calendar",
            "event",
            "appuntament",
            "riunion",
            "agenda",
            "impegn",
            "meeting",
            "scadenz",
            "libero",
            "disponibil",
        ],
        "github" => &[
            "repositor",
            "repo",
            "pull",
            "issue",
            "branch",
            "ramo",
            "rami",
            "commit",
            "workflow",
            "actions",
            "pipeline",
            "rilasci",
        ],
        "n8n" => &["automazion", "workflow"],
        _ => &[],
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' => 'a',
            'è' | 'é' => 'e',
            'ì' => 'i',
            'ò' => 'o',
            'ù' => 'u',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn mentions(request: &[String], padded: &str, stem: &str) -> bool {
    if stem.contains(' ') {
        padded.contains(stem)
    } else {
        request.iter().any(|w| w.starts_with(stem))
    }
}

/// Punteggio di pertinenza di un'operazione per la richiesta.
fn score(request: &[String], padded: &str, explicit: Option<&str>, candidate: &Candidate) -> u32 {
    let mut total = 0;
    if explicit == Some(candidate.name) {
        total += 1000;
    }
    let mut parts = candidate.name.splitn(3, '.').skip(1);
    let namespace = parts.next().unwrap_or_default();
    let operation = parts.next().unwrap_or_default();
    if namespace.split('_').all(|p| request.iter().any(|w| w == p)) {
        total += 50;
    }
    if aliases(namespace)
        .iter()
        .any(|a| mentions(request, padded, a))
    {
        total += 30;
    }
    for part in operation.split(['_', '.']).filter(|p| p.len() >= 4) {
        let stem: String = part.chars().take(5).collect();
        if request.iter().any(|w| w.starts_with(&stem)) {
            total += 20;
        }
    }
    let overlap = words(candidate.summary)
        .iter()
        .filter(|w| w.len() >= 5)
        .filter(|w| {
            let stem: String = w.chars().take(5).collect();
            request.iter().any(|r| r.starts_with(&stem))
        })
        .count()
        .min(4) as u32;
    total + overlap * 5
}

/// Indici dei candidati scelti, nell'ordine da offrire al modello: l'operazione più pertinente di ogni connettore
/// (connettori ordinati per pertinenza), poi le altre per pertinenza, fino a `limit`.
pub fn pick(
    request: &str,
    explicit: Option<&str>,
    candidates: &[Candidate],
    limit: usize,
) -> Vec<usize> {
    let request_words = words(request);
    let padded = format!(" {} ", request_words.join(" "));
    let mut groups: Vec<(String, Vec<(u32, usize)>)> = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let namespace = candidate
            .name
            .split('.')
            .nth(1)
            .unwrap_or_default()
            .to_string();
        let points = score(&request_words, &padded, explicit, candidate);
        match groups.iter_mut().find(|(n, _)| *n == namespace) {
            Some((_, ops)) => ops.push((points, index)),
            None => groups.push((namespace, vec![(points, index)])),
        }
    }
    for (_, ops) in &mut groups {
        ops.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    groups.sort_by(|a, b| {
        let best = |g: &(String, Vec<(u32, usize)>)| g.1.first().map_or(0, |o| o.0);
        best(b).cmp(&best(a)).then(a.0.cmp(&b.0))
    });
    // Fase 1: l'operazione più pertinente di **ogni** connettore (nessuno resta invisibile).
    let mut chosen: Vec<usize> = groups
        .iter()
        .filter_map(|(_, ops)| ops.first().map(|o| o.1))
        .take(limit)
        .collect();
    // Fase 2: i posti restanti per pertinenza; a parità di punteggio si alterna fra connettori.
    let mut rest: Vec<(u32, usize, usize, usize)> = Vec::new();
    for (group, (_, ops)) in groups.iter().enumerate() {
        for (position, (points, index)) in ops.iter().enumerate().skip(1) {
            rest.push((*points, position, group, *index));
        }
    }
    rest.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, _, _, index) in rest {
        if chosen.len() >= limit {
            break;
        }
        chosen.push(index);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<(String, String)> {
        let mut out = Vec::new();
        // GitHub per primo e con molte operazioni: il caso che escludeva gli altri.
        for op in [
            "repository",
            "issue",
            "notifiche",
            "crea_issue",
            "info_repo",
            "rami",
            "radice",
            "contenuto",
            "workflow",
            "esecuzioni",
            "pull_request",
            "commenta",
            "imposta_issue",
            "crea_repo",
            "modifica_repo",
            "scrivi_file",
            "elimina_file",
            "crea_ramo",
            "crea_pr",
            "unisci_pr",
            "avvia_workflow",
            "riesegui_run",
            "annulla_run",
            "abilita_workflow",
            "disabilita_workflow",
        ] {
            out.push((
                format!("connector.github.{op}"),
                "Operazione GitHub".to_string(),
            ));
        }
        for (op, summary) in [
            ("cerca", "Cerca le email"),
            ("leggi", "Legge un messaggio"),
            ("bozza", "Crea una bozza"),
            ("invia", "Invia un messaggio"),
            ("etichette", "Elenca le etichette"),
        ] {
            out.push((format!("connector.gmail.{op}"), summary.to_string()));
        }
        for (op, summary) in [
            ("eventi", "Prossimi eventi del calendario"),
            ("crea_evento", "Crea un evento"),
            ("calendari", "Elenca i calendari"),
        ] {
            out.push((
                format!("connector.google_calendar.{op}"),
                summary.to_string(),
            ));
        }
        out
    }

    fn chosen(request: &str, limit: usize) -> Vec<String> {
        let all = catalog();
        let candidates: Vec<Candidate> = all
            .iter()
            .map(|(n, s)| Candidate {
                name: n,
                summary: s,
            })
            .collect();
        pick(request, None, &candidates, limit)
            .into_iter()
            .map(|i| all[i].0.clone())
            .collect()
    }

    #[test]
    fn gmail_and_calendar_are_offered_even_when_github_has_many_operations() {
        let offered = chosen("Leggi le mie email non lette", 16);
        assert_eq!(offered.len(), 16);
        assert!(offered[0].starts_with("connector.gmail."), "{offered:?}");
        assert!(
            offered
                .iter()
                .any(|n| n.starts_with("connector.google_calendar.")),
            "il calendario non è escluso: {offered:?}"
        );
        let by_calendar = chosen("Che appuntamenti ho domani in agenda?", 16);
        assert!(
            by_calendar[0].starts_with("connector.google_calendar."),
            "{by_calendar:?}"
        );
        assert!(
            by_calendar
                .iter()
                .any(|n| n.starts_with("connector.gmail.")),
            "{by_calendar:?}"
        );
    }

    #[test]
    fn even_an_unrelated_request_keeps_every_connector_visible() {
        let offered = chosen("Dimmi una barzelletta", 9);
        for prefix in [
            "connector.github.",
            "connector.gmail.",
            "connector.google_calendar.",
        ] {
            assert!(
                offered.iter().any(|n| n.starts_with(prefix)),
                "{prefix} assente: {offered:?}"
            );
        }
    }

    #[test]
    fn an_explicit_command_comes_first_and_github_requests_still_find_github() {
        let all = catalog();
        let candidates: Vec<Candidate> = all
            .iter()
            .map(|(n, s)| Candidate {
                name: n,
                summary: s,
            })
            .collect();
        let first = pick(
            "connettore gmail invia",
            Some("connector.gmail.invia"),
            &candidates,
            4,
        );
        assert_eq!(all[first[0]].0, "connector.gmail.invia");
        let github = chosen("Apri una issue nel repository e fai una pull request", 6);
        assert!(github[0].starts_with("connector.github."), "{github:?}");
        assert!(github.iter().any(|n| n.contains("issue")), "{github:?}");
    }
}
