//! Motore di intento e di contesto (ADR-0173): da una frase colloquiale ricava il **tipo** (domanda, istruzione,
//! osservazione, autorizzazione, annullamento, correzione), gli ambiti coinvolti (anche più d'uno), le entità, i
//! riferimenti («questo», «come prima», «fallo partire») risolti con il contesto recente e un grado di fiducia.
//!
//! È deterministico, locale e senza modello: non sostituisce il modello, gli dà una lettura strutturata e
//! verificabile. **Non concede mai nulla**: un'osservazione («non funziona un cazzo») o un'autorizzazione generica
//! («vai») non allargano permessi né avviano operazioni; le conferme restano quelle del Core. I riferimenti si
//! risolvono solo con ciò che il Core ha davvero osservato (conversazione, piani, esiti) e portano l'età del dato:
//! un riferimento vecchio è marcato `stale` e non è presentato come attuale.

use serde_json::{json, Value};

/// Tipo di enunciato.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Question,
    Instruction,
    /// Constatazione o sfogo («non funziona», «è un casino»): richiede al più una diagnosi in sola lettura.
    Observation,
    /// Assenso generico («sì», «vai»): vale solo come risposta a una conferma pendente del Core.
    Authorization,
    /// «Aspetta», «fermati», «lascia stare».
    Cancellation,
    /// «No, intendevo…», «sbagliato».
    Correction,
    Chitchat,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::Question => "question",
            Kind::Instruction => "instruction",
            Kind::Observation => "observation",
            Kind::Authorization => "authorization",
            Kind::Cancellation => "cancellation",
            Kind::Correction => "correction",
            Kind::Chitchat => "chitchat",
        }
    }
}

/// Elemento del contesto recente che un riferimento può designare.
#[derive(Clone, Debug)]
pub struct Item {
    /// `plan`, `file`, `message`, `failure`.
    pub kind: &'static str,
    pub label: String,
    pub state: String,
    /// Minuti dall'ultimo aggiornamento, se noto.
    pub age_minutes: Option<i64>,
}

/// Contesto recente disponibile al Core, già filtrato dai permessi di chi lo costruisce.
#[derive(Clone, Debug, Default)]
pub struct Recent {
    pub last_user: Option<String>,
    pub plans: Vec<Item>,
    pub files: Vec<Item>,
    pub failures: Vec<Item>,
    /// Applicazione o progetto in primo piano, se l'ambiente li espone (altrimenti `None`: non si inventa).
    pub active: Option<String>,
}

/// Oltre questa età (minuti) un riferimento non è presentato come attuale.
const STALE_AFTER_MINUTES: i64 = 24 * 60;

fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' => 'a',
            'è' | 'é' => 'e',
            'ì' => 'i',
            'ò' => 'o',
            'ù' => 'u',
            c if c.is_alphanumeric() || c == '%' || c == '.' || c == '/' || c == ':' => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|p| text == *p || text.starts_with(&format!("{p} ")))
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|n| format!(" {text} ").contains(&format!(" {n} ")) || text.contains(n))
}

/// Tipo di enunciato e fiducia nella classificazione.
pub fn classify(message: &str) -> (Kind, f32) {
    let text = fold(message);
    let words = text.split_whitespace().count();
    if text.is_empty() {
        return (Kind::Chitchat, 0.2);
    }
    if starts_with_any(
        &text,
        &[
            "aspetta",
            "fermati",
            "fermo",
            "stop",
            "lascia stare",
            "lascia perdere",
            "annulla",
            "basta",
            "ferma tutto",
            "interrompi",
        ],
    ) {
        return (Kind::Cancellation, 0.95);
    }
    if starts_with_any(
        &text,
        &[
            "no",
            "intendevo",
            "volevo dire",
            "non quello",
            "non quella",
            "sbagliato",
            "mi sono spiegato male",
            "non intendevo",
            "correggo",
        ],
    ) && words > 1
    {
        return (Kind::Correction, 0.85);
    }
    if words <= 4
        && starts_with_any(
            &text,
            &[
                "si",
                "ok",
                "va bene",
                "procedi",
                "vai",
                "confermo",
                "autorizzo",
                "certo",
                "d accordo",
            ],
        )
    {
        return (Kind::Authorization, 0.8);
    }
    if starts_with_any(
        &text,
        &[
            "ciao",
            "buongiorno",
            "buonasera",
            "grazie",
            "grazie mille",
            "jarvis ci sei",
            "ci sei",
            "come stai",
        ],
    ) {
        return (Kind::Chitchat, 0.9);
    }
    let frustration = [
        "non funziona",
        "non va",
        "non parte",
        "si e bloccato",
        "e bloccato",
        "che cavolo",
        "che cazzo",
        "e un casino",
        "che casino",
        "mi sono perso",
        "e rotto",
        "non capisco cosa sta succedendo",
        "cosa sta succedendo",
    ];
    let imperative_start = [
        "controlla",
        "sistema",
        "sistemalo",
        "fai",
        "fallo",
        "avvia",
        "apri",
        "cerca",
        "trova",
        "crea",
        "scrivi",
        "aggiorna",
        "installa",
        "mostra",
        "dimmi",
        "calcola",
        "ricorda",
        "dai",
    ];
    let first = text.split_whitespace().next().unwrap_or_default();
    let interrogative = [
        "cosa", "come", "perche", "quanto", "quanta", "quale", "quali", "chi", "dove", "quando",
        "puoi", "riesci", "sai",
    ]
    .contains(&first);
    if contains_any(&text, &frustration)
        && !starts_with_any(&text, &imperative_start)
        && !interrogative
    {
        return (Kind::Observation, 0.8);
    }
    if message.trim_end().ends_with('?')
        || [
            "cosa", "come", "perche", "quanto", "quanta", "quale", "quali", "chi", "dove",
            "quando", "puoi", "riesci", "sai",
        ]
        .contains(&first)
    {
        return (Kind::Question, 0.8);
    }
    if starts_with_any(&text, &imperative_start) || words >= 2 {
        return (Kind::Instruction, if words >= 3 { 0.7 } else { 0.55 });
    }
    (Kind::Chitchat, 0.4)
}

/// «Aspetta», «fermati», «stop»: sospendere (si può riprendere). «Annulla», «lascia stare»: annullare.
pub fn is_soft_stop(message: &str) -> bool {
    starts_with_any(
        &fold(message),
        &[
            "aspetta",
            "fermati",
            "fermo",
            "stop",
            "basta",
            "ferma tutto",
            "interrompi",
        ],
    )
}

/// «Riprendi», «continua», «vai avanti»: riprendere un'attività sospesa (solo se ce n'è una).
pub fn is_resume(message: &str) -> bool {
    starts_with_any(
        &fold(message),
        &[
            "riprendi",
            "continua",
            "vai avanti",
            "ok riprendi",
            "ricomincia da dove eri",
        ],
    )
}

/// «Senza modificare…», «solo in lettura»: vincolo di sola lettura da applicare all'attività che riprende.
pub fn wants_read_only(message: &str) -> bool {
    let text = fold(message);
    [
        "senza modificare",
        "senza scrivere",
        "senza cancellare",
        "solo lettura",
        "solo in lettura",
        "senza toccare",
    ]
    .iter()
    .any(|p| text.contains(p))
}

/// «Prima fammi vedere cosa vuoi fare», «simula»: mostrare cosa farebbe un piano senza eseguirlo.
pub fn wants_preview(message: &str) -> bool {
    let text = fold(message);
    [
        "prima fammi vedere",
        "prima mostrami",
        "fammi vedere cosa vuoi fare",
        "cosa faresti",
        "simula",
        "prova a secco",
        "dry run",
    ]
    .iter()
    .any(|p| text.contains(p))
}

/// Divide una frase in richieste distinte («fai il backup e poi riavvia il server»).
pub fn split_intents(message: &str) -> Vec<String> {
    let mut parts = vec![message.to_string()];
    for sep in [
        " e poi ",
        ", poi ",
        " poi ",
        ";",
        " e anche ",
        " dopodiche ",
        " dopo ",
        " quindi ",
    ] {
        parts = parts
            .into_iter()
            .flat_map(|p| {
                p.split(sep)
                    .map(|x| x.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
    }
    parts.retain(|p| p.split_whitespace().count() >= 2);
    if parts.is_empty() {
        vec![message.trim().to_string()]
    } else {
        parts
    }
}

/// Entità riconoscibili senza modello: numeri, percentuali, durate, file e indirizzi.
pub fn entities(message: &str) -> Value {
    let mut numbers = Vec::new();
    let mut files = Vec::new();
    let mut urls = Vec::new();
    let mut durations = Vec::new();
    let tokens: Vec<&str> = message.split_whitespace().collect();
    for (i, raw) in tokens.iter().enumerate() {
        let token = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '%' && c != '/');
        if token.starts_with("http://") || token.starts_with("https://") {
            urls.push(token.to_string());
        } else if let Some((name, ext)) = token.rsplit_once('.') {
            if !name.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && !ext.chars().all(|c| c.is_ascii_digit())
            {
                files.push(token.to_string());
            }
        }
        let numeric = token.trim_end_matches('%').replace(',', ".");
        if let Ok(n) = numeric.parse::<f64>() {
            numbers.push(if token.ends_with('%') {
                json!({"value": n, "unit": "%"})
            } else {
                json!({"value": n})
            });
            if let Some(unit) = tokens.get(i + 1) {
                let unit = fold(unit);
                if [
                    "secondi",
                    "minuti",
                    "minuto",
                    "ore",
                    "ora",
                    "giorni",
                    "giorno",
                    "settimane",
                    "mesi",
                ]
                .contains(&unit.as_str())
                {
                    durations.push(format!("{n} {unit}"));
                }
            }
        }
    }
    json!({"numbers": numbers, "files": files, "urls": urls, "durations": durations})
}

fn age(item: &Item) -> String {
    match item.age_minutes {
        Some(m) if m < 1 => "adesso".into(),
        Some(m) if m < 60 => format!("{m} min fa"),
        Some(m) if m < 24 * 60 => format!("{} h fa", m / 60),
        Some(m) => format!("{} giorni fa", m / (24 * 60)),
        None => "data ignota".into(),
    }
}

fn describe(item: &Item) -> Value {
    json!({
        "kind": item.kind, "label": item.label, "state": item.state, "age": age(item),
        "stale": item.age_minutes.is_some_and(|m| m > STALE_AFTER_MINUTES),
    })
}

/// Riferimenti deittici della frase risolti con il contesto: ogni voce dice **da dove** viene la risoluzione.
pub fn resolve(message: &str, recent: &Recent) -> Vec<Value> {
    let text = fold(message);
    let mut out = Vec::new();
    let by_state = |states: &[&str]| -> Option<&Item> {
        recent
            .plans
            .iter()
            .find(|p| states.contains(&p.state.as_str()))
    };
    let entry = |phrase: &str, target: Option<&Item>, source: &str, candidates: Vec<Value>| {
        json!({
            "phrase": phrase,
            "resolved": target.map(describe),
            "source": source,
            "candidates": candidates,
        })
    };
    let all: Vec<Value> = recent
        .plans
        .iter()
        .chain(recent.files.iter())
        .map(describe)
        .collect();
    if contains_any(
        &text,
        &[
            "fallo partire",
            "avvialo",
            "avviala",
            "fallo andare",
            "falla partire",
            "lancialo",
        ],
    ) {
        let target = by_state(&["pending", "paused", "interrupted"])
            .or_else(|| recent.files.first())
            .or_else(|| recent.plans.first());
        out.push(entry(
            "fallo partire",
            target,
            "contesto recente",
            all.clone(),
        ));
    }
    if contains_any(
        &text,
        &[
            "sistemalo",
            "sistemala",
            "risolvilo",
            "risolvila",
            "aggiustalo",
            "correggilo",
        ],
    ) {
        let target = recent
            .failures
            .first()
            .or_else(|| by_state(&["failed"]))
            .or_else(|| recent.files.first());
        out.push(entry(
            "sistemalo",
            target,
            "ultimo errore osservato",
            all.clone(),
        ));
    }
    if contains_any(
        &text,
        &[
            "continua",
            "riprendi",
            "riprendi da dove eravamo rimasti",
            "da dove eravamo rimasti",
            "vai avanti",
        ],
    ) {
        let target = by_state(&["paused", "interrupted", "failed", "running", "pending"]);
        out.push(entry("continua", target, "piani", all.clone()));
    }
    if contains_any(
        &text,
        &[
            "come prima",
            "come l altra volta",
            "come ieri",
            "come avevamo fatto",
            "come l ultima volta",
        ],
    ) {
        out.push(json!({
            "phrase": "come prima", "resolved": null, "source": "memoria (procedure ed episodi)",
            "candidates": [], "needs_memory_lookup": true,
        }));
    }
    // Il pronome da risolvere sta in frasi brevi o in fondo alla frase; «di questa modifica» in una richiesta
    // lunga è un sintagma già completo e non va risolto.
    let deictics = [
        "questo",
        "questa",
        "quello",
        "quella",
        "quel file",
        "questi",
        "quelli",
    ];
    let short = text.split_whitespace().count() <= 5
        || text
            .split_whitespace()
            .last()
            .is_some_and(|w| deictics.contains(&w));
    if short && contains_any(&text, &deictics) {
        let target = recent.files.first().or_else(|| recent.plans.first());
        out.push(entry(
            "questo/quello",
            target,
            "ultimo elemento citato",
            all,
        ));
    }
    out
}

/// Analisi completa: tipo, ambiti, entità, riferimenti, fiducia e (se serve) una domanda di chiarimento.
pub fn analyze(message: &str, recent: &Recent) -> Value {
    let (kind, mut confidence) = classify(message);
    let parts = split_intents(message);
    let mut domains: Vec<(&'static str, usize)> = Vec::new();
    for part in &parts {
        for (name, score) in crate::agents::rank(part) {
            match domains.iter_mut().find(|(n, _)| *n == name) {
                Some(entry) => entry.1 += score,
                None => domains.push((name, score)),
            }
        }
    }
    domains.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    let references = resolve(message, recent);
    let unresolved: Vec<&Value> = references
        .iter()
        .filter(|r| r["resolved"].is_null() && r["needs_memory_lookup"].is_null())
        .collect();
    let tie = domains.len() > 1 && domains[0].1 == domains[1].1;
    let vague = domains.is_empty() && matches!(kind, Kind::Instruction) && references.is_empty();
    let ambiguous = !unresolved.is_empty() || tie || vague;
    if ambiguous {
        confidence = (confidence * 0.6).min(0.5);
    } else if !domains.is_empty() {
        confidence = (confidence + 0.15).min(0.98);
    }
    let clarification = if !unresolved.is_empty() {
        Some(format!(
            "Non trovo a cosa ti riferisci con «{}»: indicami l'elemento (progetto, file, piano o servizio).",
            unresolved[0]["phrase"].as_str().unwrap_or("questo")
        ))
    } else if tie {
        Some(format!(
            "La richiesta può riguardare {} o {}: quale intendi?",
            domains[0].0, domains[1].0
        ))
    } else {
        None
    };
    let implicit = match kind {
        Kind::Observation => {
            Some("diagnosi in sola lettura: raccogli evidenze, non modificare nulla")
        }
        Kind::Cancellation => Some("fermare o sospendere l'attività in corso"),
        Kind::Correction => Some("correggere la richiesta o la risposta precedente"),
        _ => None,
    };
    json!({
        "kind": kind.id(),
        "confidence": (confidence * 100.0).round() / 100.0,
        "domains": domains.iter().map(|(n, s)| json!({"agent": n, "score": s})).collect::<Vec<_>>(),
        "multi_intent": parts.len() > 1,
        "parts": parts,
        "entities": entities(message),
        "references": references,
        "ambiguous": ambiguous,
        "clarification": clarification,
        "implicit_intent": implicit,
        "grants_permission": false,
        "active_context": recent.active,
    })
}

/// Indicazione per il modello ricavata dall'analisi, o `None` se non aggiunge nulla. È testo del Core, non una
/// fonte esterna: dice cosa il Core ha capito e ricorda i limiti (nessuna autorizzazione implicita).
pub fn hint(analysis: &Value) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    match analysis["kind"].as_str().unwrap_or_default() {
        "observation" => lines.push(
            "L'ultimo messaggio è un'osservazione o uno sfogo, non un'autorizzazione: avvia una diagnosi in sola lettura (stato, log, errori recenti), riferisci le evidenze e proponi la correzione senza applicarla."
                .into(),
        ),
        "cancellation" => lines.push(
            "L'utente vuole fermare o sospendere l'attività in corso: confermalo in una riga e non avviare altro."
                .into(),
        ),
        "correction" => lines.push(
            "L'utente sta correggendo la richiesta o la risposta precedente: applica la correzione e dichiara in una riga cosa cambia."
                .into(),
        ),
        "authorization" => lines.push(
            "L'assenso generico vale solo per una conferma già proposta; se non ce n'è una, chiedi a cosa si riferisce."
                .into(),
        ),
        _ => {}
    }
    for r in analysis["references"].as_array().into_iter().flatten() {
        let phrase = r["phrase"].as_str().unwrap_or_default();
        if r["needs_memory_lookup"] == true {
            lines.push(format!(
                "«{phrase}»: cerca nella memoria la procedura o l'episodio più simile e verifica che valga ancora."
            ));
        } else if let Some(item) = r["resolved"].as_object() {
            let stale = if item["stale"] == true {
                " (dato vecchio: verifica che sia ancora attuale)"
            } else {
                ""
            };
            lines.push(format!(
                "«{phrase}» si riferisce probabilmente a {} «{}» ({}, {}){stale}.",
                item["kind"].as_str().unwrap_or("elemento"),
                item["label"].as_str().unwrap_or_default(),
                item["state"].as_str().unwrap_or_default(),
                item["age"].as_str().unwrap_or_default()
            ));
        } else {
            lines.push(format!(
                "«{phrase}» non è identificabile dal contesto: chiedi a cosa si riferisce, indicando le opzioni note."
            ));
        }
    }
    if analysis["multi_intent"] == true {
        lines.push(
            "La frase contiene più richieste: trattale in ordine e riferisci l'esito di ciascuna."
                .into(),
        );
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(state: &str, label: &str, age: i64) -> Item {
        Item {
            kind: "plan",
            label: label.into(),
            state: state.into(),
            age_minutes: Some(age),
        }
    }

    #[test]
    fn utterance_kinds_are_told_apart() {
        for (text, kind) in [
            ("Aspetta, fermati", Kind::Cancellation),
            ("lascia stare", Kind::Cancellation),
            ("No, intendevo il server di test", Kind::Correction),
            ("ok vai", Kind::Authorization),
            ("Non funziona un cazzo", Kind::Observation),
            ("È un casino", Kind::Observation),
            ("Che cavolo sta succedendo?", Kind::Observation),
            ("Quanta RAM sto usando?", Kind::Question),
            ("Controlla se ci sono bug", Kind::Instruction),
            ("grazie mille", Kind::Chitchat),
        ] {
            assert_eq!(classify(text).0, kind, "{text}");
        }
    }

    #[test]
    fn frustration_is_never_an_authorization() {
        let a = analyze("Non funziona un cazzo", &Recent::default());
        assert_eq!(a["kind"], "observation");
        assert_eq!(a["grants_permission"], false);
        assert!(hint(&a).unwrap().contains("sola lettura"));
        let ok = analyze("ok vai", &Recent::default());
        assert_eq!(ok["grants_permission"], false);
    }

    #[test]
    fn references_resolve_with_the_context_and_declare_their_source_and_age() {
        let recent = Recent {
            plans: vec![plan("paused", "Aggiorna il server", 12)],
            ..Recent::default()
        };
        let a = analyze("Fallo partire", &recent);
        assert_eq!(
            a["references"][0]["resolved"]["label"],
            "Aggiorna il server"
        );
        assert_eq!(a["references"][0]["resolved"]["age"], "12 min fa");
        assert_eq!(a["ambiguous"], false);
        let old = Recent {
            plans: vec![plan("failed", "Backup notturno", 3 * 24 * 60)],
            ..Recent::default()
        };
        let a = analyze("Riprendi da dove eravamo rimasti", &old);
        assert_eq!(a["references"][0]["resolved"]["stale"], true);
        assert!(hint(&a).unwrap().contains("dato vecchio"));
    }

    #[test]
    fn an_unresolvable_reference_asks_instead_of_guessing() {
        let a = analyze("Sistemalo", &Recent::default());
        assert_eq!(a["ambiguous"], true);
        assert!(a["clarification"].as_str().unwrap().contains("sistemalo"));
        assert!(a["confidence"].as_f64().unwrap() <= 0.5);
    }

    #[test]
    fn as_before_needs_memory_and_is_not_flagged_as_unresolved() {
        let a = analyze("Fai come l'altra volta", &Recent::default());
        assert_eq!(a["references"][0]["needs_memory_lookup"], true);
        assert_eq!(a["ambiguous"], false);
    }

    #[test]
    fn several_requests_and_entities_are_extracted() {
        let a = analyze(
            "Fai il backup di dati.zip e poi riavvia il server tra 10 minuti",
            &Recent::default(),
        );
        assert_eq!(a["multi_intent"], true);
        assert_eq!(a["parts"].as_array().unwrap().len(), 2);
        assert_eq!(a["entities"]["files"][0], "dati.zip");
        assert_eq!(a["entities"]["durations"][0], "10 minuti");
    }

    #[test]
    fn ties_between_domains_ask_which_one() {
        let a = analyze("controlla il server e i bug", &Recent::default());
        assert!(a["domains"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn stop_resume_read_only_and_preview_phrases_are_recognised() {
        assert!(is_soft_stop("Aspetta, fermati."));
        assert!(!is_soft_stop("lascia stare"));
        assert!(is_resume("Riprendi quello che stavi facendo"));
        assert!(is_resume("Continua, ma senza modificare il database"));
        assert!(wants_read_only("Continua, ma senza modificare il database"));
        assert!(!wants_read_only("Continua"));
        assert!(wants_preview("Prima fammi vedere cosa vuoi fare."));
        assert!(!is_resume("cerca la ricevuta"));
    }
}
