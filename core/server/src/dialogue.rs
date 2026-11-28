//! Atti di conversazione (ADR-0217): il parlato quotidiano («ci sei?», «a che punto siamo?», «non farmi un papiro»,
//! «pensaci tu, mi fido») ha un'**intenzione** diversa dalla richiesta di un'attività. Questo modulo la riconosce,
//! locale e deterministico, e la traduce in due cose:
//!
//! - per **presenza**, **apertura** e **stato** una risposta costruita solo con ciò che il Core sa davvero (attività
//!   in corso, passi di un piano completati su totali, conferme in attesa): niente percentuali o tempi inventati;
//! - per gli altri atti un'**istruzione di stile o di metodo** per il modello (sii breve, approfondisci, diagnostica
//!   in sola lettura, verifica prima di rispondere…).
//!
//! **Non concede mai nulla**: «fai direttamente tu» e «mi fido» non allargano permessi né saltano conferme; «non ho
//! capito» non ferma i piani; uno sfogo avvia solo una diagnosi in sola lettura.

/// Come migliorare l'ultimo risultato.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refine {
    Improve,
    Professional,
    Simplify,
    Faster,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Presence,
    Opening,
    Status,
    Clarify,
    Brevity,
    Detail,
    Concretize,
    NextStep,
    Drop,
    Redirect,
    Problem,
    Refine(Refine),
    Urgency,
    Delegate,
    Alternatives,
    Understand,
    Grounding,
    Proceed,
}

impl Act {
    pub fn id(self) -> &'static str {
        match self {
            Act::Presence => "presence",
            Act::Opening => "opening",
            Act::Status => "status",
            Act::Clarify => "clarify",
            Act::Brevity => "brevity",
            Act::Detail => "detail",
            Act::Concretize => "concretize",
            Act::NextStep => "next_step",
            Act::Drop => "drop",
            Act::Redirect => "redirect",
            Act::Problem => "problem",
            Act::Refine(Refine::Improve) => "refine_improve",
            Act::Refine(Refine::Professional) => "refine_professional",
            Act::Refine(Refine::Simplify) => "refine_simplify",
            Act::Refine(Refine::Faster) => "refine_faster",
            Act::Urgency => "urgency",
            Act::Delegate => "delegate",
            Act::Alternatives => "alternatives",
            Act::Understand => "understand",
            Act::Grounding => "grounding",
            Act::Proceed => "proceed",
        }
    }

    /// Risposte che il Core sa dare da solo e deve dare da solo (con dati reali).
    pub fn answered_by_core(self) -> bool {
        matches!(self, Act::Presence | Act::Opening | Act::Status)
    }
}

/// Più parole di così: è una richiesta vera con un soggetto, non un atto di conversazione.
const MAX_WORDS: usize = 9;

pub(crate) fn fold(text: &str) -> String {
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

/// L'atto di conversazione della frase, se è uno; l'ordine dei controlli risolve le frasi vicine
/// («spiegamelo come si deve» è una spiegazione, «fallo come si deve» un miglioramento).
pub fn act(message: &str) -> Option<Act> {
    let text = fold(message);
    let words = text.split_whitespace().count();
    if text.is_empty() || words > MAX_WORDS {
        return None;
    }
    let t = text.as_str();
    if has(
        t,
        &[
            "ci sei",
            "mi senti",
            "sei ancora attivo",
            "sei online",
            "sei qui",
            "sei sveglio",
        ],
    ) {
        return Some(Act::Presence);
    }
    if has(
        t,
        &[
            "non ho capito",
            "non capisco",
            "non mi e chiaro",
            "spiegamelo",
            "spiegamela",
            "spiegami meglio",
            "rispiegamelo",
        ],
    ) {
        return Some(Act::Clarify);
    }
    if has(
        t,
        &[
            "punto della situazione",
            "a che punto siamo",
            "a che punto sei",
            "hai finito",
            "quanto manca",
            "ci stai mettendo cosi tanto",
            "ci metti cosi tanto",
            "ti blocca",
            "cosa hai combinato",
            "che hai combinato",
            "com e andata",
        ],
    ) {
        return Some(Act::Status);
    }
    if has(
        t,
        &[
            "che si dice",
            "da dove iniziamo",
            "da dove partiamo",
            "che abbiamo da fare",
            "cosa abbiamo da fare",
            "mettiamoci al lavoro",
            "cosa facciamo oggi",
        ],
    ) {
        return Some(Act::Opening);
    }
    if has(
        t,
        &[
            "girarci troppo intorno",
            "girarci intorno",
            "non farmi un papiro",
            "sii breve",
            "in breve",
            "vai al punto",
            "senza giri di parole",
        ],
    ) {
        return Some(Act::Brevity);
    }
    if has(
        t,
        &[
            "piu nel dettaglio",
            "nel dettaglio",
            "approfondisci",
            "piu dettagli",
        ],
    ) {
        return Some(Act::Detail);
    }
    if has(t, &["concretamente", "in pratica", "un esempio concreto"]) {
        return Some(Act::Concretize);
    }
    if has(
        t,
        &[
            "e quindi che facciamo",
            "quindi che facciamo",
            "che facciamo",
            "come procediamo",
            "qual e il prossimo passo",
        ],
    ) {
        return Some(Act::NextStep);
    }
    if has(
        t,
        &[
            "lascia perdere",
            "lasciamo perdere",
            "non importa piu",
            "vabbe lascia",
        ],
    ) {
        return Some(Act::Drop);
    }
    if has(
        t,
        &[
            "facciamo diversamente",
            "cambiamo approccio",
            "proviamo un altro modo",
            "facciamo in un altro modo",
        ],
    ) {
        return Some(Act::Redirect);
    }
    if has(
        t,
        &[
            "miglioralo",
            "miglioralo un po",
            "migliorala",
            "fallo come si deve",
            "fallo meglio",
            "fallo bene",
        ],
    ) {
        return Some(Act::Refine(Refine::Improve));
    }
    if has(
        t,
        &[
            "piu professionale",
            "rendilo professionale",
            "rendila professionale",
        ],
    ) {
        return Some(Act::Refine(Refine::Professional));
    }
    if has(
        t,
        &[
            "semplificalo",
            "semplificala",
            "troppo complicato",
            "troppo complessa",
            "piu semplice",
        ],
    ) {
        return Some(Act::Refine(Refine::Simplify));
    }
    if has(
        t,
        &[
            "fallo piu veloce",
            "piu veloce",
            "piu rapido",
            "velocizzalo",
            "velocizzala",
        ],
    ) {
        return Some(Act::Refine(Refine::Faster));
    }
    if has(
        t,
        &[
            "non voglio perdere tempo",
            "dai che dobbiamo finire",
            "ho fretta",
            "e urgente",
            "dobbiamo finire",
        ],
    ) {
        return Some(Act::Urgency);
    }
    if has(
        t,
        &[
            "fai direttamente tu",
            "pensaci tu",
            "fai tu",
            "decidi tu",
            "scegli tu",
            "mi fido",
            "ritieni piu opportuno",
            "ritieni opportuno",
        ],
    ) {
        return Some(Act::Delegate);
    }
    if has(
        t,
        &[
            "tutte le possibilita",
            "non accontentarti",
            "un modo migliore",
            "altre soluzioni",
            "altre strade",
        ],
    ) {
        return Some(Act::Alternatives);
    }
    if has(
        t,
        &[
            "capire cosa intendo",
            "capisci cosa intendo",
            "hai capito cosa intendo",
        ],
    ) {
        return Some(Act::Understand);
    }
    if has(
        t,
        &[
            "non inventarti",
            "non inventare",
            "se non lo sai",
            "controlla prima di rispondermi",
            "verifica prima di rispondere",
            "non tirare a indovinare",
        ],
    ) {
        return Some(Act::Grounding);
    }
    if has(
        t,
        &[
            "abbiamo fatto una cazzata",
            "abbiamo fatto un errore",
            "che cavolo e successo",
            "che cazzo e successo",
            "cosa e successo",
            "non va una mazza",
            "ma prima funzionava",
            "prima funzionava",
            "non dovrebbe fare cosi",
            "qualcosa che non torna",
            "abbiamo sbagliato",
            "non mi convince",
            "c e un problema",
        ],
    ) {
        return Some(Act::Problem);
    }
    if has(
        t,
        &[
            "vediamo di risolverla",
            "vediamo di risolvere",
            "andiamo avanti",
            "procediamo",
            "perfetto ora",
        ],
    ) {
        return Some(Act::Proceed);
    }
    None
}

/// Istruzione per il modello. Dice **come** rispondere, non concede nulla.
pub fn directive(act: Act) -> &'static str {
    match act {
        Act::Presence | Act::Opening | Act::Status => "Rispondi in una o due frasi con lo stato reale delle attività; non inventare lavori in corso.",
        Act::Clarify => "L'utente non ha capito: rispiega l'ultima risposta in modo più semplice, con frasi corte, un esempio concreto e senza gergo; poi chiedi in una riga se ora è chiaro. Non avviare nulla e non fermare i lavori in corso.",
        Act::Brevity => "Rispondi in al massimo tre frasi, prima la risposta e poi, se serve, il motivo; niente preamboli, niente elenchi lunghi. Offri il dettaglio solo se richiesto.",
        Act::Detail => "Approfondisci l'ultimo argomento restando sullo stesso tema: passaggi, motivi, casi limite e fonti.",
        Act::Concretize => "Rendi concreta l'ultima risposta: passi numerati, comandi o esempi reali e il risultato atteso di ciascuno.",
        Act::NextStep => "Proponi al massimo tre prossimi passi in ordine di priorità, dicendo cosa faresti subito e cosa richiede la conferma dell'utente.",
        Act::Drop => "L'utente lascia perdere: confermalo in una riga, non avviare altro e non cancellare il lavoro già fatto.",
        Act::Redirect => "L'utente cambia strada: abbandona l'approccio corrente senza difenderlo, proponi l'alternativa che ha indicato o chiedi quale, e conserva il lavoro già fatto.",
        Act::Problem => "L'utente segnala che qualcosa non va o non convince: non difenderti e non agire. Raccogli evidenze in sola lettura (stato, log, errori e modifiche recenti), dì cosa è cambiato rispetto a quando funzionava, distingui i fatti dalle ipotesi e proponi la correzione senza applicarla.",
        Act::Refine(Refine::Improve) => "Migliora l'ultimo risultato prodotto (testo, codice, piano) senza ricominciare da zero e dichiara in una riga cosa hai cambiato; se non c'è un risultato precedente chiedi quale.",
        Act::Refine(Refine::Professional) => "Riscrivi l'ultimo risultato con tono professionale e struttura chiara (titoli, elenchi, termini precisi), senza cambiarne il contenuto; dichiara in una riga cosa hai cambiato.",
        Act::Refine(Refine::Simplify) => "Semplifica l'ultimo risultato: meno passaggi, parole comuni, una sola idea per frase; non perdere nulla di essenziale e dichiara cosa hai tolto.",
        Act::Refine(Refine::Faster) => "Rendi più rapido l'ultimo risultato o il percorso: se riguarda l'esecuzione scegli la via più breve tra quelle sicure senza saltare conferme o verifiche; se riguarda il codice o un processo indica l'ottimizzazione con la misura che la giustifica.",
        Act::Urgency => "L'utente ha fretta: vai dritto, niente preamboli, una sola domanda e solo se indispensabile. Conferme e verifiche non si saltano.",
        Act::Delegate => "L'utente ti lascia la scelta: scegli l'opzione più prudente tra quelle ragionevoli, dichiara in una riga cosa hai scelto e perché, e procedi solo entro permessi e conferme già in vigore: la fiducia non concede permessi nuovi.",
        Act::Alternatives => "Valuta almeno tre alternative con vantaggi, rischi e costi, non fermarti alla prima; raccomanda una e spiega perché.",
        Act::Understand => "Ricostruisci l'intenzione dal contesto recente e dalla memoria, esponila in una riga e procedi; chiedi chiarimenti solo se due letture sono ugualmente plausibili.",
        Act::Grounding => "Rispondi solo con ciò che hai verificato (strumenti, memoria, fonti): controlla prima, indica da dove viene ogni dato e di' «non lo so» o «non l'ho verificato» quando manca. Non inventare nulla.",
        Act::Proceed => "Riprendi dal punto raggiunto: se c'è un'attività sospesa o un passo già concordato proseguilo, altrimenti dì qual è il prossimo passo.",
    }
}

/// Un piano visto dal Core: passi completati su totali (mai percentuali o tempi stimati).
#[derive(Clone, Debug, Default)]
pub struct PlanLine {
    pub goal: String,
    pub status: String,
    pub done: usize,
    pub total: usize,
    pub next: Option<String>,
}

/// Ciò che il Core sa davvero, al momento della domanda.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub running_tasks: Vec<String>,
    pub plans: Vec<PlanLine>,
    pub confirmations: usize,
    pub last_failure: Option<String>,
}

impl Snapshot {
    pub fn is_idle(&self) -> bool {
        self.running_tasks.is_empty() && self.plans.is_empty() && self.confirmations == 0
    }
}

fn status_word(status: &str) -> &'static str {
    match status {
        "running" => "in corso",
        "paused" => "in pausa",
        "waiting" => "in attesa di conferma",
        "pending" => "pronto da avviare",
        "interrupted" => "interrotto",
        _ => "aperto",
    }
}

/// Risposta del Core per presenza, apertura e stato; `None` per gli altri atti.
pub fn reply(act: Act, snap: &Snapshot) -> Option<String> {
    let quanto_manca = false;
    let _ = quanto_manca;
    match act {
        Act::Presence => Some(if snap.is_idle() {
            "Ci sono, ti ascolto. Al momento non ho attività in corso né conferme in attesa."
                .to_string()
        } else {
            format!("Ci sono, ti ascolto. {}", summary(snap, true))
        }),
        Act::Opening => Some(if snap.is_idle() {
            "Nessuna attività aperta né conferma in attesa: possiamo partire da dove vuoi. Dimmi l'obiettivo, oppure «che si può fare» e ti propongo qualcosa in base a ciò che so di te.".to_string()
        } else {
            format!("{} Da quale partiamo?", summary(snap, false))
        }),
        Act::Status => Some(if snap.is_idle() {
            match &snap.last_failure {
                Some(failure) => format!("Non ho attività in corso né conferme in attesa. L'ultima cosa andata male: {failure}."),
                None => "Non ho attività in corso né conferme in attesa: non c'è nulla da finire.".to_string(),
            }
        } else {
            format!("{}{}", summary(snap, false), eta_note(snap))
        }),
        _ => None,
    }
}

fn summary(snap: &Snapshot, short: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for task in snap.running_tasks.iter().take(3) {
        parts.push(format!("Sto lavorando a «{task}»."));
    }
    for plan in snap.plans.iter().take(if short { 1 } else { 5 }) {
        let mut line = format!(
            "Piano «{}»: {}, {} passi su {} completati",
            plan.goal,
            status_word(&plan.status),
            plan.done,
            plan.total
        );
        if let Some(next) = &plan.next {
            line.push_str(&format!("; prossimo: {next}"));
        }
        line.push('.');
        parts.push(line);
    }
    if snap.confirmations > 0 {
        parts.push(format!(
            "{} {} in attesa della tua risposta.",
            snap.confirmations,
            if snap.confirmations == 1 {
                "conferma"
            } else {
                "conferme"
            }
        ));
    }
    if !short {
        if let Some(failure) = &snap.last_failure {
            parts.push(format!("Ultimo intoppo: {failure}."));
        }
    }
    parts.join(" ")
}

/// Per «quanto manca»: niente percentuali né tempi, solo ciò che è misurabile.
fn eta_note(snap: &Snapshot) -> String {
    if snap.plans.iter().any(|p| p.total > 0) {
        " Non stimo tempi o percentuali che non posso misurare: ti dico solo quanti passi restano."
            .to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset() -> Vec<(String, String)> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/datasets/colloquial_dataset.tsv");
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| {
                let (text, act) = l.rsplit_once(" | ").unwrap();
                (text.to_string(), act.to_string())
            })
            .collect()
    }

    #[test]
    fn every_colloquial_phrase_of_the_dataset_gets_its_act() {
        let rows = dataset();
        assert_eq!(rows.len(), 50);
        for (text, want) in rows {
            assert_eq!(act(&text).map(Act::id), Some(want.as_str()), "{text}");
        }
    }

    #[test]
    fn real_requests_are_not_mistaken_for_conversation() {
        for text in [
            "Controlla lo stato del server e dimmi se i backup sono riusciti",
            "Cerca un'alternativa a questa libreria",
            "Prepara un preventivo per il cliente Rossi con tutte le voci",
            "Fammi un'analisi dei costi di questo progetto",
        ] {
            assert_eq!(act(text), None, "{text}");
        }
    }

    #[test]
    fn replies_use_only_what_the_core_knows() {
        let idle = Snapshot::default();
        assert!(reply(Act::Presence, &idle).unwrap().contains("Ci sono"));
        assert!(reply(Act::Status, &idle)
            .unwrap()
            .contains("non c'è nulla da finire"));
        let busy = Snapshot {
            running_tasks: vec!["backup del progetto".into()],
            plans: vec![PlanLine {
                goal: "release".into(),
                status: "running".into(),
                done: 3,
                total: 7,
                next: Some("esegui i test".into()),
            }],
            confirmations: 1,
            last_failure: None,
        };
        let text = reply(Act::Status, &busy).unwrap();
        assert!(
            text.contains("3 passi su 7")
                && text.contains("esegui i test")
                && text.contains("1 conferma"),
            "{text}"
        );
        assert!(!text.contains('%'), "mai percentuali inventate: {text}");
        assert!(text.contains("Non stimo tempi"), "{text}");
        assert!(reply(Act::Brevity, &busy).is_none());
        assert!(Act::Presence.answered_by_core() && !Act::Delegate.answered_by_core());
    }

    #[test]
    fn trust_and_confusion_never_grant_or_stop_anything() {
        assert!(directive(Act::Delegate).contains("non concede permessi"));
        assert!(directive(Act::Clarify).contains("non fermare i lavori"));
        assert!(directive(Act::Problem).contains("sola lettura"));
    }
}
