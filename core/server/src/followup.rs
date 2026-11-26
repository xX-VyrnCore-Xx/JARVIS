//! Continuità fra i turni (ADR-0218): vincoli e seguiti del parlato («sistema solo il secondo», «il database non lo
//! devi toccare», «non pubblicarla ancora», «e la RAM?», «senza pagare», «stavolta sul server»).
//!
//! Sono lette dai **messaggi dell'utente** della chat, quindi valgono finché restano nella finestra recente e sono
//! rimosse da un'esplicita revoca («ora puoi pubblicarla»). Due livelli:
//! - **indicazioni** per il modello (quale elemento, quale criterio, quale ambiente), con ciò che il Core sa del turno
//!   precedente;
//! - **blocchi reali** sugli strumenti: «non toccare X» nega le scritture che citano X; «non pubblicare» nega gli
//!   strumenti che pubblicano, uniscono o rilasciano. Un blocco non si aggira cambiando strumento: è controllato dal
//!   Core a ogni chiamata.

use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// «il database non lo devi toccare»: nessuna scrittura che citi l'argomento.
    Exclude(String),
    /// «non pubblicarla ancora»: nessuno strumento che pubblica, unisce o rilascia.
    HoldPublish,
    /// «senza pagare»: solo soluzioni gratuite.
    FreeOnly,
    /// «solo il secondo»: agire solo sull'elemento N dell'ultimo elenco.
    Select(usize),
    /// «tieni soltanto quelli che…»: filtrare i risultati precedenti.
    Filter(String),
    /// «no, per progetto»: sostituire il criterio proposto.
    Replace(String),
    /// «riavvia solo quello non critico».
    NonCritical,
}

impl Constraint {
    pub fn id(&self) -> String {
        match self {
            Constraint::Exclude(s) => format!("exclude:{s}"),
            Constraint::HoldPublish => "hold_publish".into(),
            Constraint::FreeOnly => "free_only".into(),
            Constraint::Select(n) => format!("select:{n}"),
            Constraint::Filter(_) => "filter".into(),
            Constraint::Replace(s) => format!("replace:{s}"),
            Constraint::NonCritical => "scope:non_critical".into(),
        }
    }
}

/// Che tipo di seguito è l'ultimo messaggio rispetto al turno precedente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Followup {
    /// «Sì, e fallo partire»: conferma la domanda precedente e aggiunge un'azione.
    Confirm,
    /// «Quale?»: chiede di specificare un elemento dell'ultima risposta.
    Which,
    /// «Sistemala e riprova».
    FixAndRetry,
    /// «E la RAM?»: stesso contesto, altro oggetto.
    OtherSubject,
    /// «Qual è quella più leggera?»: confronto fra le alternative già trovate.
    Compare,
    /// «Quello di cui parlavamo ieri»: riferimento da cercare in memoria.
    MemoryReference,
    /// «Stavolta sul server»: stessa procedura, ambiente diverso.
    AdaptEnvironment,
}

impl Followup {
    pub fn id(self) -> &'static str {
        match self {
            Followup::Confirm => "confirm",
            Followup::Which => "which",
            Followup::FixAndRetry => "fix_and_retry",
            Followup::OtherSubject => "other_subject",
            Followup::Compare => "compare",
            Followup::MemoryReference => "memory_reference",
            Followup::AdaptEnvironment => "adapt_environment",
        }
    }
}

fn fold(text: &str) -> String {
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
        .collect::<Vec<_>>()
        .join(" ")
}

fn has(text: &str, needles: &[&str]) -> bool {
    let padded = format!(" {text} ");
    needles.iter().any(|n| padded.contains(&format!(" {n}")))
}

const ARTICLES: [&str; 12] = [
    "il", "lo", "la", "i", "gli", "le", "l", "un", "una", "del", "della", "al",
];
const FILLERS: [&str; 10] = [
    "aspetta", "ma", "pero", "no", "ok", "senti", "scusa", "dai", "allora", "guarda",
];

fn ordinal(word: &str) -> Option<usize> {
    Some(match word {
        "primo" | "prima" | "1" => 1,
        "secondo" | "seconda" | "2" => 2,
        "terzo" | "terza" | "3" => 3,
        "quarto" | "quarta" | "4" => 4,
        "quinto" | "quinta" | "5" => 5,
        "ultimo" | "ultima" => 99,
        _ => return None,
    })
}

/// Vincoli espressi in **un** messaggio dell'utente.
pub fn constraints_in(message: &str) -> Vec<Constraint> {
    let text = fold(message);
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    // Esclusioni: «il database non lo devi toccare», «non toccare il database», «senza toccare i file di config».
    for marker in [
        "non lo devi toccare",
        "non la devi toccare",
        "non li devi toccare",
        "non devi toccare",
        "non toccare",
        "senza toccare",
        "non modificare",
        "non devi modificare",
    ] {
        if let Some(at) = text.find(marker) {
            let before: Vec<&str> = text[..at]
                .split_whitespace()
                .filter(|w| !ARTICLES.contains(w) && !FILLERS.contains(w))
                .collect();
            let after: Vec<&str> = text[at + marker.len()..]
                .split_whitespace()
                .filter(|w| !ARTICLES.contains(w))
                .collect();
            let subject = if marker.contains("devi") && marker.starts_with("non l") {
                before.last().copied()
            } else {
                after.first().copied().or_else(|| before.last().copied())
            };
            if let Some(subject) = subject {
                out.push(Constraint::Exclude(subject.to_string()));
            }
            break;
        }
    }
    if has(
        &text,
        &[
            "non pubblicar",
            "non pubblicare",
            "non rilasciar",
            "non rilasciare",
            "non fare il rilascio",
            "non fare il deploy",
            "non deployare",
        ],
    ) {
        out.push(Constraint::HoldPublish);
    }
    if has(
        &text,
        &[
            "senza pagare",
            "gratis",
            "gratuit",
            "senza spendere",
            "a costo zero",
            "senza costi",
        ],
    ) {
        out.push(Constraint::FreeOnly);
    }
    // «solo il secondo», «soltanto la terza», «sistema solo il 2».
    for (i, w) in words.iter().enumerate() {
        if matches!(*w, "solo" | "soltanto" | "solamente") {
            if let Some(n) = words.iter().skip(i + 1).take(2).find_map(|x| ordinal(x)) {
                out.push(Constraint::Select(n));
            }
        }
    }
    for lead in [
        "tieni soltanto ",
        "tieni solo ",
        "tieni solamente ",
        "tieni quelli ",
        "filtra per ",
        "filtra ",
    ] {
        if let Some(at) = text.find(lead) {
            let criterion = text[at + lead.len()..].trim();
            if !criterion.is_empty() {
                out.push(Constraint::Filter(criterion.to_string()));
            }
            break;
        }
    }
    if (text.starts_with("no ") || text.starts_with("invece ")) && text.contains(" per ") {
        if let Some(at) = text.find(" per ") {
            let to = text[at + 5..].trim();
            if !to.is_empty() && to.split_whitespace().count() <= 3 {
                out.push(Constraint::Replace(to.to_string()));
            }
        }
    }
    if has(
        &text,
        &[
            "solo quello non critico",
            "quello non critico",
            "solo i non critici",
            "solo quelli non critici",
            "non critico",
        ],
    ) && has(&text, &["solo", "soltanto"])
    {
        out.push(Constraint::NonCritical);
    }
    out
}

/// Revoca esplicita di un blocco di pubblicazione.
fn releases_hold(message: &str) -> bool {
    has(
        &fold(message),
        &[
            "puoi pubblicar",
            "pubblicala",
            "pubblicalo",
            "ok pubblica",
            "procedi con la pubblicazione",
            "via libera",
            "ora puoi rilasciar",
            "rilasciala",
        ],
    )
}

/// Vincoli attivi nella chat: letti dai messaggi dell'utente dal più vecchio al più nuovo; una revoca toglie il blocco.
pub fn active_constraints(user_messages: &[String]) -> Vec<Constraint> {
    let mut active: Vec<Constraint> = Vec::new();
    for message in user_messages {
        if releases_hold(message) {
            active.retain(|c| *c != Constraint::HoldPublish);
        }
        for constraint in constraints_in(message) {
            let replaces = |c: &Constraint| match (&constraint, c) {
                (Constraint::Select(_), Constraint::Select(_)) => true,
                (Constraint::Filter(_), Constraint::Filter(_)) => true,
                (Constraint::Replace(_), Constraint::Replace(_)) => true,
                _ => *c == constraint,
            };
            active.retain(|c| !replaces(c));
            active.push(constraint);
        }
    }
    active
}

/// Il tipo di seguito di un messaggio breve, se lo è.
pub fn followup(message: &str) -> Option<Followup> {
    let text = fold(message);
    let words = text.split_whitespace().count();
    if text.is_empty() || words > 10 {
        return None;
    }
    if has(
        &text,
        &[
            "quello di cui parlavamo",
            "quella di cui parlavamo",
            "di cui parlavamo ieri",
            "quello di ieri",
            "quella di ieri",
            "come l altra volta",
        ],
    ) {
        return Some(Followup::MemoryReference);
    }
    if has(
        &text,
        &[
            "stavolta sul",
            "stavolta sulla",
            "questa volta sul",
            "questa volta sulla",
            "stavolta su",
            "ma stavolta",
        ],
    ) {
        return Some(Followup::AdaptEnvironment);
    }
    if has(&text, &["e riprova", "riprova", "e riprovaci"])
        && has(
            &text,
            &["sistemal", "correggil", "risolvil", "aggiustal", "sistema"],
        )
    {
        return Some(Followup::FixAndRetry);
    }
    if text == "quale"
        || text == "quali"
        || has(
            &text,
            &["quale servizio", "quale dei", "quali dei", "quale di"],
        ) && words <= 4
    {
        return Some(Followup::Which);
    }
    if has(
        &text,
        &[
            "qual e quella",
            "qual e quello",
            "quale e quella",
            "quale e quello",
            "qual e la piu",
            "qual e il piu",
        ],
    ) {
        return Some(Followup::Compare);
    }
    if (text.starts_with("e la ")
        || text.starts_with("e il ")
        || text.starts_with("e le ")
        || text.starts_with("e i ")
        || text.starts_with("e lo ")
        || text.starts_with("e l "))
        && words <= 5
    {
        return Some(Followup::OtherSubject);
    }
    if (text.starts_with("si ") || text.starts_with("ok ") || text.starts_with("esatto "))
        && words >= 3
        && has(
            &text,
            &[
                "fallo", "falla", "avvialo", "avviala", "partire", "procedi", "fai",
            ],
        )
    {
        return Some(Followup::Confirm);
    }
    None
}

/// Indicazioni per il modello su vincoli e seguito, con ciò che il Core ricorda del turno precedente.
pub fn hint_lines(
    message: &str,
    constraints: &[Constraint],
    previous_user: Option<&str>,
    last_reply: Option<&str>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let before = |what: &str| -> String {
        match previous_user {
            Some(p) => format!("{what} («{}»)", p.chars().take(90).collect::<String>()),
            None => {
                format!("{what} (non c'è una richiesta precedente: chiedi a cosa si riferisce)")
            }
        }
    };
    let last = last_reply.map(|r| r.chars().take(140).collect::<String>());
    if let Some(f) = followup(message) {
        lines.push(match f {
            Followup::Confirm => format!(
                "«Sì» risponde alla domanda che hai appena fatto{}: esegui quell'azione e poi quella aggiunta (per «fallo partire» usa la procedura di avvio del progetto in questione), nei limiti di permessi e conferme.",
                last.as_ref().map(|l| format!(" («{l}»)")).unwrap_or_default()
            ),
            Followup::Which => format!(
                "«Quale?» chiede di specificare un elemento della tua ultima risposta{}: indicalo dalle evidenze già raccolte, senza ripetere un controllo generico.",
                last.as_ref().map(|l| format!(" («{l}»)")).unwrap_or_default()
            ),
            Followup::FixAndRetry => "Correggi la causa indicata nell'ultimo esito in modo compatibile (versioni e file di blocco coerenti), poi ripeti l'operazione; se fallisce ancora riporta l'errore reale senza tentativi alla cieca.".to_string(),
            Followup::OtherSubject => format!(
                "È un seguito di una richiesta precedente: stesso dispositivo o ambito, stessa azione, ora sull'altro oggetto nominato. {}",
                before("Richiesta precedente")
            ),
            Followup::Compare => format!(
                "Confronta le alternative già individuate nell'ultima risposta{} secondo il criterio chiesto, con i dati verificati; non cercarne di nuove.",
                last.as_ref().map(|l| format!(" («{l}»)")).unwrap_or_default()
            ),
            Followup::MemoryReference => "Il riferimento va cercato nella memoria e nelle chat recenti (argomento di ieri o discusso prima): chiedi una precisazione solo se i candidati sono ambigui.".to_string(),
            Followup::AdaptEnvironment => format!(
                "Riusa la procedura già individuata adattandola al nuovo ambiente e verifica la compatibilità (sistema, versioni, permessi) prima di eseguire. {}",
                before("Procedura precedente")
            ),
        });
    }
    for c in constraints {
        lines.push(match c {
            Constraint::Exclude(s) => format!("Vincolo dell'utente: non toccare «{s}». Escludilo dal piano; il Core nega comunque le scritture che lo citano."),
            Constraint::HoldPublish => "Vincolo dell'utente: non pubblicare né rilasciare. Conserva gli artefatti in locale; il Core nega gli strumenti che pubblicano, uniscono o rilasciano finché l'utente non dà via libera.".to_string(),
            Constraint::FreeOnly => "Vincolo dell'utente: solo soluzioni gratuite. Mantieni l'obiettivo iniziale e scarta ciò che richiede un pagamento, indicando i limiti dei piani gratuiti.".to_string(),
            Constraint::Select(n) if *n == 99 => "Vincolo dell'utente: agisci solo sull'ultimo elemento dell'ultimo elenco; non toccare gli altri.".to_string(),
            Constraint::Select(n) => format!("Vincolo dell'utente: agisci solo sull'elemento numero {n} dell'ultimo elenco che hai dato; non toccare gli altri."),
            Constraint::Filter(c) => format!("Vincolo dell'utente: dei risultati precedenti tieni solo quelli che soddisfano «{c}», verificandolo caso per caso; scarta gli altri dichiarando quanti."),
            Constraint::Replace(to) => format!("L'utente sostituisce il criterio proposto con «{to}»: aggiorna il piano e conferma in una riga cosa cambia."),
            Constraint::NonCritical => "Vincolo dell'utente: agisci solo sull'elemento non critico. Identificalo dalle evidenze (ruolo, dipendenze, impatto), verifica i permessi prima di agire e, se non è univoco, chiedi quale.".to_string(),
        });
    }
    lines
}

fn mentions(args: &str, subject: &str) -> bool {
    let aliases: &[&str] = match subject {
        "database" | "db" | "dati" => &[
            "database",
            ".db",
            ".sqlite",
            "sqlite",
            "migration",
            "migrazion",
            "schema.sql",
            "postgres",
            "mysql",
        ],
        _ => &[],
    };
    args.contains(subject) || aliases.iter().any(|a| args.contains(a))
}

/// Blocco reale: `Some(motivo)` se un vincolo attivo vieta la chiamata. Le letture non sono mai bloccate dalle
/// esclusioni; la pubblicazione è bloccata per nome dello strumento.
pub fn blocks(
    constraints: &[Constraint],
    tool: &str,
    effects: &str,
    arguments: &Value,
) -> Option<String> {
    let name = tool.to_lowercase();
    let args = arguments.to_string().to_lowercase();
    for c in constraints {
        match c {
            Constraint::HoldPublish => {
                let publishing = [
                    "release",
                    "rilascio",
                    "rilasci",
                    "publish",
                    "pubblic",
                    "deploy",
                    "unisci",
                    "merge",
                    "avvia_workflow",
                    "riesegui",
                    "push",
                ]
                .iter()
                .any(|k| name.contains(k));
                if publishing && effects != "read" && effects != "none" {
                    return Some("Hai chiesto di non pubblicare ancora: conservo il lavoro in locale finché non dai via libera.".into());
                }
            }
            Constraint::Exclude(subject)
                if effects != "read" && effects != "none" && mentions(&args, subject) =>
            {
                return Some(format!("Hai chiesto di non toccare «{subject}»: questa operazione lo modificherebbe, quindi non la eseguo."));
            }
            _ => {}
        }
    }
    None
}

/// Analisi per l'API e per il motore di intento.
pub fn analysis(
    message: &str,
    user_history: &[String],
    previous_user: Option<&str>,
    last_reply: Option<&str>,
) -> Value {
    let mut all: Vec<String> = user_history.to_vec();
    all.push(message.to_string());
    let constraints = active_constraints(&all);
    json!({
        "followup": followup(message).map(Followup::id),
        "constraints": constraints.iter().map(Constraint::id).collect::<Vec<_>>(),
        "hints": hint_lines(message, &constraints, previous_user, last_reply),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversations() -> Vec<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/data/conversation_dataset.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn every_conversation_of_the_dataset_is_understood() {
        let all = conversations();
        assert_eq!(all.len(), 15);
        for conv in all {
            let id = conv["id"].as_u64().unwrap();
            let turns: Vec<(String, String)> = conv["turns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    (
                        t[0].as_str().unwrap().to_string(),
                        t[1].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            let (last_role, last) = turns.last().unwrap().clone();
            assert_eq!(last_role, "user");
            let users: Vec<String> = turns
                .iter()
                .take(turns.len() - 1)
                .filter(|(r, _)| r == "user")
                .map(|(_, t)| t.clone())
                .collect();
            let reply = turns
                .iter()
                .rev()
                .skip(1)
                .find(|(r, _)| r == "jarvis")
                .map(|(_, t)| t.clone());
            let a = analysis(
                &last,
                &users,
                users.last().map(String::as_str),
                reply.as_deref(),
            );
            let expect = &conv["expect"];
            if let Some(f) = expect["followup"].as_str() {
                assert_eq!(a["followup"], f, "conversazione {id}: {a}");
                assert!(
                    !a["hints"].as_array().unwrap().is_empty(),
                    "conversazione {id}: nessuna indicazione"
                );
            }
            for want in expect["constraints"].as_array().into_iter().flatten() {
                let want = want.as_str().unwrap();
                let got: Vec<&str> = a["constraints"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                assert!(
                    got.contains(&want),
                    "conversazione {id}: manca {want} in {got:?}"
                );
            }
            if let Some(act) = expect["act"].as_str() {
                assert_eq!(
                    crate::dialogue::act(&last).map(crate::dialogue::Act::id),
                    Some(act),
                    "conversazione {id}"
                );
            }
            for b in expect["blocks"].as_array().into_iter().flatten() {
                let all_users: Vec<String> = users
                    .iter()
                    .cloned()
                    .chain(std::iter::once(last.clone()))
                    .collect();
                let constraints = active_constraints(&all_users);
                let blocked = blocks(
                    &constraints,
                    b["tool"].as_str().unwrap(),
                    "write_local",
                    &b["args"],
                )
                .is_some();
                assert_eq!(
                    blocked,
                    b["blocked"].as_bool().unwrap(),
                    "conversazione {id}: {b}"
                );
            }
        }
    }

    #[test]
    fn a_hold_is_lifted_only_by_an_explicit_release_and_reads_are_never_blocked() {
        let held = active_constraints(&[
            "Prepara una release".into(),
            "Non pubblicarla ancora.".into(),
        ]);
        assert_eq!(held, vec![Constraint::HoldPublish]);
        assert!(blocks(
            &held,
            "connector.github.unisci_pr",
            "write_external",
            &json!({})
        )
        .is_some());
        assert!(blocks(&held, "connector.github.info_repo", "read", &json!({})).is_none());
        let freed = active_constraints(&[
            "Non pubblicarla ancora.".into(),
            "Ok, ora puoi pubblicarla.".into(),
        ]);
        assert!(freed.is_empty(), "{freed:?}");
        let excluded = active_constraints(&["Il database non lo devi toccare.".into()]);
        assert!(blocks(
            &excluded,
            "shell.run",
            "write_local",
            &json!({"command": "sqlite3 app.db 'drop table x'"})
        )
        .is_some());
        assert!(
            blocks(&excluded, "fs.read", "read", &json!({"path": "app.db"})).is_none(),
            "leggere è consentito"
        );
    }

    #[test]
    fn selections_and_filters_replace_the_previous_ones() {
        let c = active_constraints(&[
            "Sistema solo il secondo.".into(),
            "No, solo il terzo.".into(),
            "Senza pagare".into(),
        ]);
        assert!(
            c.contains(&Constraint::Select(3))
                && !c.contains(&Constraint::Select(2))
                && c.contains(&Constraint::FreeOnly)
        );
    }
}
