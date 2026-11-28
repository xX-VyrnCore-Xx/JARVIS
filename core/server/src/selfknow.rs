//! Conoscenza di sé di JARVIS (ADR-0236): chi è, cosa sa fare, come è fatto, cosa conserva e cosa non può fare.
//!
//! Le risposte sono **deterministiche e costruite su dati veri** (versione, numero di strumenti, competenze,
//! connettori collegati): JARVIS sa presentarsi anche senza modello cloud e non inventa capacità. Per le domande
//! più libere su di sé la stessa scheda di fatti passa al modello nel prompt (`sheet`). Nessun dato personale:
//! solo conteggi e nomi di funzioni.

/// Fatti verificabili sullo stato attuale di JARVIS.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub version: String,
    pub os: String,
    pub tools: usize,
    pub skills: usize,
    pub connectors_installed: usize,
    pub connectors_authorized: usize,
    pub datasets: usize,
}

/// I nove regni di Yggdrasill: nome, titolo e cosa contengono.
pub const REALMS: [(&str, &str, &str); 9] = [
    (
        "Asgard",
        "Controllo & Comando",
        "plancia, chat, attività e conferme",
    ),
    (
        "Midgard",
        "Utente & Progetti",
        "profilo, utenti e workspace",
    ),
    (
        "Alfheim",
        "Intelligenza & Conoscenza",
        "modelli AI, agenti e dataset",
    ),
    (
        "Vanaheim",
        "Creatività & Contenuti",
        "voce, mappe, competenze e pacchetti",
    ),
    (
        "Jotunheim",
        "Sviluppo & Costruzione",
        "missioni, piani, strumenti e aggiornamenti",
    ),
    (
        "Svartalfheim",
        "Strumenti & Infrastruttura",
        "automazioni, connettori, dispositivi e sistema",
    ),
    ("Muspelheim", "Sicurezza & Difesa", "sicurezza e privacy"),
    (
        "Helheim",
        "Archiviazione & Memoria",
        "memoria e ricerche salvate",
    ),
    (
        "Niflheim",
        "Monitoraggio & Analisi",
        "registro di audit e notifiche",
    ),
];

/// Di cosa sta chiedendo l'utente quando parla di JARVIS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Identity,
    Capabilities,
    Architecture,
    Privacy,
    Realms,
    Limits,
    Version,
    Origin,
}

impl Topic {
    pub fn id(self) -> &'static str {
        match self {
            Topic::Identity => "identity",
            Topic::Capabilities => "capabilities",
            Topic::Architecture => "architecture",
            Topic::Privacy => "privacy",
            Topic::Realms => "realms",
            Topic::Limits => "limits",
            Topic::Version => "version",
            Topic::Origin => "origin",
        }
    }

    pub const ALL: [Topic; 8] = [
        Topic::Identity,
        Topic::Capabilities,
        Topic::Architecture,
        Topic::Privacy,
        Topic::Realms,
        Topic::Limits,
        Topic::Version,
        Topic::Origin,
    ];
}

/// Più parole di così: è una richiesta con un soggetto proprio, non una domanda su JARVIS.
const MAX_WORDS: usize = 14;

fn has(text: &str, needles: &[&str]) -> bool {
    let padded = format!(" {text} ");
    needles.iter().any(|n| padded.contains(&format!(" {n}")))
}

/// L'argomento della domanda su JARVIS, se la frase è proprio una domanda su di lui.
pub fn topic(message: &str) -> Option<Topic> {
    let text = crate::dialogue::fold(message);
    let words = text.split_whitespace().count();
    if text.is_empty() || words > MAX_WORDS {
        return None;
    }
    let t = text.as_str();
    // I limiti e l'origine prima dell'identità: «cosa non sai fare» contiene «cosa sai fare».
    if has(
        t,
        &[
            "cosa non sai fare",
            "cosa non puoi fare",
            "che cosa non sai fare",
            "quali sono i tuoi limiti",
            "i tuoi limiti",
            "dove sbagli",
            "cosa non riesci a fare",
        ],
    ) {
        return Some(Topic::Limits);
    }
    if has(
        t,
        &[
            "chi ti ha creato",
            "chi ti ha fatto",
            "chi ti ha costruito",
            "chi ti ha programmato",
            "chi ti ha sviluppato",
            "chi e il tuo creatore",
            "chi ti ha ideato",
            "sei di nvidia",
            "sei un prodotto nvidia",
            "sei nemotron",
            "ti ha creato nvidia",
            "ti ha creato anthropic",
            "ti ha creato openai",
            "sei un modello di nvidia",
            "sei chatgpt",
            "sei claude",
            "che modello sei",
            "quale modello sei",
            "che modello usi",
            "quale modello usi",
        ],
    ) {
        return Some(Topic::Origin);
    }
    if has(
        t,
        &[
            "che versione sei",
            "che versione di jarvis",
            "quale versione sei",
            "che versione stai usando",
            "versione di jarvis",
        ],
    ) {
        return Some(Topic::Version);
    }
    if has(t, &["yggdrasill", "yggdrasil", "nove regni", "9 regni"]) {
        return Some(Topic::Realms);
    }
    if has(
        t,
        &[
            "che dati conservi",
            "quali dati conservi",
            "dove salvi i dati",
            "dove tieni i miei dati",
            "i miei dati sono al sicuro",
            "cosa invii al cloud",
            "cosa mandi al cloud",
            "cosa esce dal computer",
            "la mia privacy",
            "come proteggi i miei dati",
            "cosa salvi di me",
        ],
    ) {
        return Some(Topic::Privacy);
    }
    if has(
        t,
        &[
            "come funzioni",
            "come sei fatto",
            "come sei fatta",
            "di cosa sei fatto",
            "di cosa sei fatta",
            "come sei costruito",
            "come sei costruita",
            "dove giri",
            "che tecnologie usi",
            "qual e la tua architettura",
            "su cosa giri",
            "come sei strutturato",
            "come sei strutturata",
        ],
    ) {
        return Some(Topic::Architecture);
    }
    if has(
        t,
        &[
            "cosa sai fare",
            "che cosa sai fare",
            "cosa puoi fare",
            "che cosa puoi fare",
            "a cosa servi",
            "che funzioni hai",
            "quali funzioni hai",
            "quali strumenti hai",
            "che strumenti hai",
            "cosa posso chiederti",
            "come ti uso",
            "come posso usarti",
            "in cosa puoi aiutarmi",
            "come puoi aiutarmi",
            "quali competenze hai",
            "che competenze hai",
        ],
    ) {
        return Some(Topic::Capabilities);
    }
    if has(
        t,
        &[
            "chi sei",
            "chi e jarvis",
            "presentati",
            "auto presentazione",
            "autopresentazione",
            "fammi una presentazione",
            "fai una breve presentazione",
            "una presentazione di te",
            "parlami di te",
            "cosa sei",
            "che cosa sei",
            "come ti chiami",
            "descriviti",
            "raccontami di te",
            "dimmi chi sei",
            "fai una presentazione",
            "presenta te stesso",
            "che cos e jarvis",
        ],
    ) {
        return Some(Topic::Identity);
    }
    None
}

/// La frase parla di JARVIS o gli chiede di sé in modo più libero: alla risposta del modello si aggiunge la scheda.
pub fn mentions_self(message: &str) -> bool {
    let text = crate::dialogue::fold(message);
    has(
        &text,
        &[
            "jarvis",
            "yggdrasill",
            "yggdrasil",
            "te stesso",
            "di te",
            "ti chiami",
            "sei in grado",
            "sai fare",
            "puoi fare",
            "le tue funzioni",
            "le tue capacita",
            "i tuoi strumenti",
            "le tue competenze",
            "i tuoi dati",
            "tua architettura",
            "come sei",
            "chi sei",
            "cosa sei",
        ],
    )
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn realms_line() -> String {
    REALMS
        .iter()
        .map(|(name, title, what)| format!("{name} ({title}: {what})"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Risposta alla domanda su JARVIS, costruita sui fatti veri.
pub fn answer(topic: Topic, facts: &Facts) -> String {
    let tools = plural(facts.tools, "strumento", "strumenti");
    let skills = plural(facts.skills, "competenza", "competenze");
    let connectors = if facts.connectors_authorized > 0 {
        format!(
            "ne ho {} collegati e autorizzati",
            facts.connectors_authorized
        )
    } else {
        "per ora non ne ho collegati: si collegano da Connettori".to_string()
    };
    match topic {
        Topic::Identity => format!(
            "Sono JARVIS, il tuo assistente personale: giro sul tuo computer (versione {}) e vivo in Yggdrasill, un mondo di nove regni che raccolgono le mie funzioni. \
             Posso conversare e ragionare con modelli di linguaggio (solo se li autorizzi), usare {tools} con permessi e conferme, applicare {skills}, ricordare ciò che mi chiedi di ricordare, \
             collegarmi a servizi esterni come posta e calendario ({connectors}), parlare con voce maschile, ascoltare «Hey Jarvis» e portare avanti missioni e automazioni in background, \
             coordinandomi con agenti specializzati (sistema, ricerca, codice, memoria, azienda, automazioni, sicurezza). \
             Tutto ciò che tocca file, rete o altri servizi passa da policy, conferme e un registro firmato. Chiedimi «cosa sai fare» per i dettagli.",
            facts.version
        ),
        Topic::Capabilities => format!(
            "Ecco cosa so fare davvero, in gruppi: \
             parlare e scrivere (rispondere, riassumere, correggere, tradurre); ricordare e ritrovare (memoria, ricerche salvate); \
             usare il computer (file, applicazioni, schermo, strumenti di sistema, sul Mac anche mouse e tastiera, sempre con permessi); \
             sviluppare (leggere il codice, mappare un progetto, eseguire test in un ambiente isolato, usare git); \
             automatizzare (flussi, promemoria, missioni lunghe, n8n); collegarmi a servizi esterni ({connectors}); cercare sul web con fonti; \
             migliorarmi (dataset privati su Hugging Face, che tu accetti). Oggi ho {tools} e {skills}. \
             Le azioni che cambiano qualcosa, escono dal computer o costano chiedono la tua conferma. Dimmi un obiettivo e ti dico come lo affronterei."
        ),
        Topic::Architecture => format!(
            "Sono fatto così: un Core scritto in Rust, sul tuo computer, che custodisce memoria, permessi, segreti cifrati e registro di audit; \
             un livello di intelligenza in Python che sceglie lo strumento giusto (offline con regole, oppure con i modelli NVIDIA NIM se li abiliti); \
             le app per macOS (SwiftUI e 3D), Windows e Android; i contratti JSON che fissano cosa può chiedere ciascuna parte; connettori e server MCP per i servizi esterni. \
             Ogni richiesta passa dallo stesso percorso: capire, scegliere gli strumenti, controllare i permessi, eseguire, verificare l'esito e registrarlo. Versione {}.",
            facts.version
        ),
        Topic::Privacy => "I tuoi dati restano sul tuo computer: memoria, conversazioni e impostazioni stanno nella cartella dei dati, i segreti sono cifrati con una chiave custodita dal portachiavi del sistema, \
             e ogni azione importante finisce in un registro firmato. Verso il cloud esce solo ciò che serve a rispondere (il messaggio, i turni recenti se la memoria della conversazione è attiva, i ricordi pertinenti) \
             e solo verso il provider che scegli; mai password, chiavi o token. I connettori e il web si usano soltanto quando glielo chiedi. Nella pagina Privacy vedi cosa conservo, cosa può uscire e chi ha il permesso.".to_string(),
        Topic::Realms => format!(
            "Yggdrasill è l'albero che rappresenta il mio mondo, con nove regni; ognuno raccoglie un gruppo di funzioni: {}. \
             Nell'app tocchi un regno e vedi le sue pagine.",
            realms_line()
        ),
        Topic::Limits => "Ecco i miei limiti, senza giri di parole: non agisco su file, rete o servizi senza i permessi (e, dove serve, la tua conferma); non vedo lo schermo oltre al riconoscimento del testo; \
             i modelli cloud richiedono una chiave e la rete, e senza di essi rispondo solo con le regole locali; i servizi esterni funzionano solo se li hai collegati e autorizzati; \
             posso sbagliare, quindi per le cose importanti ti mostro cosa ho fatto e come verificarlo. Se non so una cosa, te lo dico invece di inventarla.".to_string(),
        Topic::Version => format!(
            "Sono JARVIS versione {}, in esecuzione su {}. Le novità di ogni versione sono nelle note della release, e da Aggiornamenti vedo se ce n'è una più recente.",
            facts.version,
            os_name(&facts.os)
        ),
        Topic::Origin => "Sono una creazione personale del mio proprietario, non un prodotto di NVIDIA né di altre aziende: il codice sta nel suo repository (Core in Rust, intelligenza in Python, app per macOS, Windows e Android) ed è stato sviluppato con l'assistenza di Claude Code. \
             I modelli di linguaggio sono solo motori intercambiabili che uso se li abilita; io resto JARVIS e giro sul suo computer.".to_string(),
    }
}

fn os_name(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

/// Scheda di fatti per il modello: la si aggiunge al prompt quando la richiesta parla di JARVIS.
pub fn sheet(facts: &Facts) -> String {
    format!(
        "Informazioni verificate su di te, JARVIS (usale se ti chiedono di te; non aggiungere capacità che qui non compaiono): \
         versione {}, sistema {}; {} e {} attivi; {} servizi collegati su {} installati; {} dataset di addestramento privati. \
         Sei l'assistente personale del proprietario, nel mondo di Yggdrasill con nove regni: {}. \
         Architettura: Core in Rust sul computer del proprietario (memoria, permessi, segreti cifrati, registro di audit firmato), intelligenza in Python (regole offline e modelli NVIDIA NIM se abilitati), \
         app per macOS, Windows e Android. Agenti: generalista, sistema, ricerca, codice, memoria, azienda, automazioni, sicurezza. \
         Privacy: i dati restano sul computer; al cloud esce solo il testo necessario e mai segreti. Le azioni che modificano, escono dal computer o costano chiedono conferma. \
         Per presentarti: nome, ruolo, tre o quattro cose concrete che sai fare, come ti mantieni sicuro, e un invito a dire l'obiettivo.",
        facts.version,
        os_name(&facts.os),
        plural(facts.tools, "strumento", "strumenti"),
        plural(facts.skills, "competenza", "competenze"),
        facts.connectors_authorized,
        facts.connectors_installed,
        facts.datasets,
        realms_line()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            version: "9.9.9".into(),
            os: "macos".into(),
            tools: 120,
            skills: 31,
            connectors_installed: 3,
            connectors_authorized: 2,
            datasets: 6,
        }
    }

    #[test]
    fn questions_about_jarvis_are_recognised() {
        let cases = [
            ("chi sei?", Topic::Identity),
            ("Presentati, per favore", Topic::Identity),
            ("fai una breve autopresentazione", Topic::Identity),
            ("parlami di te", Topic::Identity),
            ("cosa sai fare?", Topic::Capabilities),
            ("a cosa servi", Topic::Capabilities),
            ("quali strumenti hai?", Topic::Capabilities),
            ("come funzioni?", Topic::Architecture),
            ("di cosa sei fatto", Topic::Architecture),
            ("che dati conservi di me?", Topic::Privacy),
            ("i miei dati sono al sicuro?", Topic::Privacy),
            ("cos'è Yggdrasill?", Topic::Realms),
            ("spiegami i nove regni", Topic::Realms),
            ("cosa non sai fare?", Topic::Limits),
            ("quali sono i tuoi limiti", Topic::Limits),
            ("che versione sei?", Topic::Version),
            ("chi ti ha creato?", Topic::Origin),
        ];
        for (text, want) in cases {
            assert_eq!(topic(text), Some(want), "{text}");
        }
        // Richieste vere con un soggetto proprio non sono domande su JARVIS.
        for text in [
            "apri jarvis nelle impostazioni",
            "cosa sai fare di questo file e come lo ripari se è rotto in più punti del codice",
            "scrivi una presentazione per il progetto di domani con dieci diapositive e un riepilogo finale",
            "ciao",
            "",
        ] {
            assert_eq!(topic(text), None, "{text}");
        }
    }

    #[test]
    fn answers_use_the_true_facts_and_never_invent_capabilities() {
        let f = facts();
        let who = answer(Topic::Identity, &f);
        assert!(
            who.contains("JARVIS")
                && who.contains("9.9.9")
                && who.contains("120 strumenti")
                && who.contains("31 competenze")
        );
        assert!(who.contains("ne ho 2 collegati"), "{who}");
        let nothing = answer(
            Topic::Identity,
            &Facts {
                connectors_authorized: 0,
                ..f.clone()
            },
        );
        assert!(nothing.contains("per ora non ne ho collegati"), "{nothing}");
        let one = answer(
            Topic::Capabilities,
            &Facts {
                tools: 1,
                skills: 1,
                ..f.clone()
            },
        );
        assert!(
            one.contains("1 strumento ") && one.contains("1 competenza"),
            "{one}"
        );
        let realms = answer(Topic::Realms, &f);
        for (name, _, _) in REALMS {
            assert!(realms.contains(name), "{name}");
        }
        assert_eq!(REALMS.len(), 9);
        assert!(answer(Topic::Version, &f).contains("macOS"));
        // Niente frasi da modello generico, né dati personali.
        for topic in Topic::ALL {
            let text = answer(topic, &f).to_lowercase();
            assert!(
                !text.contains("come modello di linguaggio") && !text.contains("non ho coscienza"),
                "{text}"
            );
            assert!(!text.contains('@') && !text.contains("http"), "{text}");
        }
    }

    #[test]
    fn the_model_sheet_carries_the_facts_and_the_guardrail() {
        let text = sheet(&facts());
        assert!(
            text.contains("9.9.9") && text.contains("120 strumenti") && text.contains("Asgard")
        );
        assert!(text.contains("non aggiungere capacità"));
        assert!(mentions_self("raccontami come funziona jarvis"));
        assert!(mentions_self("quali sono le tue competenze"));
        assert!(!mentions_self("quanto fa 12 per 12"));
    }
}
