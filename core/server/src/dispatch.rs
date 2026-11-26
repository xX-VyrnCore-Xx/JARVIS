//! Squadra di agenti per una richiesta (ADR-0217): **agente principale** e **agenti di supporto**, ricavati da
//! segnali lessicali pesati. I nomi sono quelli di `agents::SPECIALISTS`: `system`, `monitoring` (sicurezza),
//! `research`, `coding`, `business`, `memory`, `automation`.
//!
//! Regole di fondo, ricavate dal dataset di instradamento (`tests/data/routing_dataset.tsv`):
//! - il **soggetto** della richiesta (la libreria, il server, il cliente) pesa più del verbo («cerca», «controlla»);
//! - una **cadenza o una condizione** («ogni giorno», «avvisami», «quando finisce») rende principale
//!   l'automazione;
//! - un **verbo di recupero** («ricostruisci», «recupera», «ricordami», «riprendi») rende principale la memoria;
//! - alcuni segnali **implicano un supporto** (una diagnosi del sistema chiama la sicurezza; una libreria chiama la
//!   ricerca web per verificarla).
//!
//! È deterministico e senza modello. Non sostituisce `agents::route` (che decide con quali strumenti rispondere):
//! dà al motore di intento e al pianificatore una lettura più ricca di *chi* dovrebbe collaborare.

pub const SYSTEM: &str = "system";
pub const SECURITY: &str = "monitoring";
pub const RESEARCH: &str = "research";
pub const CODING: &str = "coding";
pub const BUSINESS: &str = "business";
pub const MEMORY: &str = "memory";
pub const AUTOMATION: &str = "automation";

/// Un segnale: radice di parola (o frase), agente e peso. Il `$` finale vuole la parola intera.
struct Cue(&'static str, &'static str, u32);

const CUES: &[Cue] = &[
    // --- sistema: macchina, risorse, servizi, rete, contenitori, backup
    Cue("mac ", SYSTEM, 2),
    Cue("computer", SYSTEM, 2),
    Cue("pc$", SYSTEM, 2),
    Cue("ram$", SYSTEM, 3),
    Cue("cpu$", SYSTEM, 3),
    Cue("memoria ram", SYSTEM, 3),
    Cue("disco", SYSTEM, 3),
    Cue("dischi", SYSTEM, 3),
    Cue("processo", SYSTEM, 3),
    Cue("processi", SYSTEM, 3),
    Cue("docker", SYSTEM, 4),
    Cue("contenitor", SYSTEM, 3),
    Cue("server", SYSTEM, 3),
    Cue("serviz", SYSTEM, 2),
    Cue("rete$", SYSTEM, 3),
    Cue("connession", SYSTEM, 3),
    Cue("backup", SYSTEM, 4),
    Cue("infrastruttura", SYSTEM, 4),
    Cue("risorse", SYSTEM, 3),
    Cue("consumo", SYSTEM, 3),
    Cue("consumando", SYSTEM, 3),
    Cue("lent", SYSTEM, 3),
    Cue("non parte", SYSTEM, 3),
    Cue("prestazion", SYSTEM, 2),
    Cue("sistema operativo", SYSTEM, 3),
    Cue("avvio", SYSTEM, 1),
    Cue("riavvi", SYSTEM, 2),
    Cue("log di sistema", SYSTEM, 3),
    Cue("ambiente", SYSTEM, 1),
    Cue("installa", SYSTEM, 1),
    // --- sicurezza
    Cue("credenzial", SECURITY, 4),
    Cue("segret", SECURITY, 3),
    Cue("token", SECURITY, 2),
    Cue("password", SECURITY, 3),
    Cue("esposti", SECURITY, 6),
    Cue("esposto", SECURITY, 6),
    Cue("vulnerabil", SECURITY, 4),
    Cue("sicurezza", SECURITY, 4),
    Cue("sicuro", SECURITY, 5),
    Cue("sicure", SECURITY, 5),
    Cue("sicuri", SECURITY, 5),
    Cue("affidabil", SECURITY, 5),
    Cue("malware", SECURITY, 4),
    Cue("virus", SECURITY, 4),
    Cue("attacc", SECURITY, 3),
    Cue("isolato", SECURITY, 2),
    Cue("permess", SECURITY, 2),
    Cue("cifrat", SECURITY, 2),
    Cue("porte aperte", SECURITY, 4),
    Cue("firewall", SECURITY, 4),
    // --- ricerca web
    Cue("cerca", RESEARCH, 1),
    Cue("trova", RESEARCH, 1),
    Cue("trovami", RESEARCH, 1),
    Cue("ricerca", RESEARCH, 2),
    Cue("sul web", RESEARCH, 4),
    Cue("online", RESEARCH, 2),
    Cue("nuova versione", RESEARCH, 4),
    Cue("e uscit", RESEARCH, 3),
    Cue("uscita", RESEARCH, 3),
    Cue("aggiornate", RESEARCH, 3),
    Cue("aggiornati", RESEARCH, 3),
    Cue("informazioni", RESEARCH, 2),
    Cue("alternativ", RESEARCH, 2),
    Cue("gratuit", RESEARCH, 3),
    Cue("supportat", RESEARCH, 4),
    Cue("documentazione", RESEARCH, 2),
    Cue("notizie", RESEARCH, 3),
    Cue("quanto costa", RESEARCH, 2),
    Cue("recension", RESEARCH, 3),
    // --- codice
    Cue("algoritmo", CODING, 4),
    Cue("codice", CODING, 4),
    Cue("libreria", CODING, 4),
    Cue("librerie", CODING, 4),
    Cue("dipendenz", CODING, 3),
    Cue("repository", CODING, 3),
    Cue("progetto jarvis", CODING, 2),
    Cue("build", CODING, 3),
    Cue("release", CODING, 3),
    Cue("modifica", CODING, 3),
    Cue("sviluppo", CODING, 3),
    Cue("api$", CODING, 3),
    Cue("bug", CODING, 4),
    Cue("funzione", CODING, 2),
    Cue("compil", CODING, 3),
    Cue("tecnicamente", CODING, 3),
    Cue("fattibil", CODING, 2),
    Cue("velocizzare", CODING, 1),
    Cue("test$", CODING, 3),
    Cue("refactor", CODING, 4),
    // --- azienda
    Cue("client", BUSINESS, 4),
    Cue("commercial", BUSINESS, 4),
    Cue("preventiv", BUSINESS, 3),
    Cue("concorrent", BUSINESS, 4),
    Cue("azienda", BUSINESS, 3),
    Cue("aziendal", BUSINESS, 3),
    Cue("redditiz", BUSINESS, 4),
    Cue("costi", BUSINESS, 4),
    Cue("costo", BUSINESS, 3),
    Cue("fattur", BUSINESS, 3),
    Cue("strategia", BUSINESS, 3),
    Cue("clienti", BUSINESS, 4),
    Cue("vendit", BUSINESS, 3),
    Cue("opportunita", BUSINESS, 2),
    Cue("proporre", BUSINESS, 2),
    Cue("proposta", BUSINESS, 3),
    Cue("budget", BUSINESS, 4),
    Cue("guadagn", BUSINESS, 3),
    Cue("priorita dei miei progetti", BUSINESS, 2),
    // --- memoria
    Cue("ricostruisci", MEMORY, 5),
    Cue("recupera", MEMORY, 5),
    Cue("ricordami", MEMORY, 5),
    Cue("ricordi", MEMORY, 3),
    Cue("ricorda", MEMORY, 3),
    Cue("riprendi", MEMORY, 5),
    Cue("avevamo", MEMORY, 4),
    Cue("lasciato ieri", MEMORY, 4),
    Cue("decisioni", MEMORY, 4),
    Cue("scartato", MEMORY, 4),
    Cue("ieri", MEMORY, 2),
    Cue("l altra volta", MEMORY, 4),
    Cue("salva$", MEMORY, 3),
    Cue("memorizza", MEMORY, 4),
    Cue("dimentica", MEMORY, 4),
    Cue("priorita", MEMORY, 3),
    Cue("organizza tutto", MEMORY, 1),
    // --- automazioni
    Cue("ogni giorno", AUTOMATION, 6),
    Cue("ogni ora", AUTOMATION, 6),
    Cue("ogni settimana", AUTOMATION, 6),
    Cue("ogni cinque minuti", AUTOMATION, 6),
    Cue("ogni minuto", AUTOMATION, 6),
    Cue("ogni ", AUTOMATION, 4),
    Cue("avvisami", AUTOMATION, 6),
    Cue("avvertimi", AUTOMATION, 6),
    Cue("notificami", AUTOMATION, 6),
    Cue("segnalami", AUTOMATION, 4),
    Cue("automatizz", AUTOMATION, 6),
    Cue("automazion", AUTOMATION, 6),
    Cue("quando finisce", AUTOMATION, 6),
    Cue("quando viene", AUTOMATION, 6),
    Cue("pianific", AUTOMATION, 5),
    Cue("tieni d occhio", AUTOMATION, 5),
    Cue("monitora", AUTOMATION, 5),
];

/// Segnali che **chiamano un supporto** anche senza parole del suo ambito: (agente principale, segnale, supporto).
const IMPLIED: &[(&str, &str, &str)] = &[
    (SYSTEM, "lent", SECURITY),
    (SYSTEM, "rete$", SECURITY),
    (SYSTEM, "backup", SECURITY),
    (SYSTEM, "infrastruttura", SECURITY),
    (SYSTEM, "processo", RESEARCH),
    (SYSTEM, "docker", CODING),
    (SYSTEM, "non parte", CODING),
    (SYSTEM, "consumo", CODING),
    (CODING, "libreria", RESEARCH),
    (CODING, "algoritmo", RESEARCH),
    (CODING, "fattibil", RESEARCH),
    (CODING, "isolato", SYSTEM),
    (CODING, "isolato", SECURITY),
    (CODING, "qualita", SECURITY),
    (CODING, "release", SECURITY),
    (SECURITY, "credenzial", CODING),
    (SECURITY, "modifica", CODING),
    (SECURITY, "librer", RESEARCH),
    (SECURITY, "dipendenz", CODING),
    (SECURITY, "esposti", SYSTEM),
    (SECURITY, "dipendenz", RESEARCH),
    (BUSINESS, "client", RESEARCH),
    (BUSINESS, "commercial", MEMORY),
    (BUSINESS, "costi", CODING),
    (BUSINESS, "concorrent", RESEARCH),
    (BUSINESS, "redditiz", RESEARCH),
    (BUSINESS, "richieste dei clienti", MEMORY),
    (BUSINESS, "tutto quello che riguarda", MEMORY),
    (BUSINESS, "tutto quello che riguarda", SYSTEM),
    (BUSINESS, "sito", RESEARCH),
    (MEMORY, "decisioni", CODING),
    (MEMORY, "scartato", CODING),
    (MEMORY, "preventiv", BUSINESS),
    (MEMORY, "priorita", BUSINESS),
    (RESEARCH, "salva", MEMORY),
    (RESEARCH, "soluzione", CODING),
    (RESEARCH, "supportat", CODING),
    (AUTOMATION, "server", SYSTEM),
    (AUTOMATION, "repository", CODING),
    (AUTOMATION, "download", SECURITY),
    (AUTOMATION, "procedura", SYSTEM),
    (AUTOMATION, "anomali", SYSTEM),
];

/// Archetipo di richiesta **composita**: tutte le famiglie di segnali devono comparire (ognuna con almeno un
/// segnale), e allora collaborano gli agenti indicati per ottenere il risultato atteso.
pub struct Playbook {
    pub id: &'static str,
    pub all_of: &'static [&'static [&'static str]],
    /// Il primo è l'agente principale.
    pub agents: &'static [&'static str],
    pub expected: &'static str,
}

const PLAYBOOKS: &[Playbook] = &[
    Playbook {
        id: "repo_fix_build",
        all_of: &[
            &["repositor", "progetto", "codebase"],
            &["errori", "errore", "bug", "problemi"],
            &["build", "compila", "eseguibile", "release"],
        ],
        agents: &[CODING, SYSTEM, SECURITY],
        expected: "modifiche verificate, test eseguiti e build generata",
    },
    Playbook {
        id: "client_proposal",
        all_of: &[
            &["cliente", "clienti", "azienda"],
            &["proposta", "preventivo", "offerta"],
            &["analizza", "attivita", "trovami", "trova", "cerca"],
        ],
        agents: &[RESEARCH, BUSINESS],
        expected: "cliente reale, necessità documentate e proposta personalizzata",
    },
    Playbook {
        id: "server_performance",
        all_of: &[
            &["server", "macchina", "sistema"],
            &[
                "prestazion",
                "performance",
                "piu veloce",
                "velocizz",
                "ottimizz",
                "lent",
            ],
            &[
                "migliorare",
                "migliorarle",
                "possiamo",
                "come",
                "dimmi",
                "consigli",
            ],
        ],
        agents: &[SYSTEM, SECURITY, RESEARCH],
        expected: "diagnostica basata su metriche effettive e interventi consigliati",
    },
    Playbook {
        id: "resume_project",
        all_of: &[
            &["recupera", "riprendi", "ripristina il contesto"],
            &["progetto", "lavoro", "attivita"],
            &["sviluppo", "continua", "prosegui", "andare avanti"],
        ],
        agents: &[MEMORY, CODING, SYSTEM],
        expected:
            "contesto ripristinato e attività ripresa senza sovrascrivere modifiche esistenti",
    },
    Playbook {
        id: "tech_migration",
        all_of: &[
            &[
                "nuova tecnologia",
                "nuovo framework",
                "nuova libreria",
                "alternativa tecnologica",
            ],
            &[
                "migrare",
                "migrazione",
                "conviene",
                "passare a",
                "sostituire",
            ],
        ],
        agents: &[RESEARCH, CODING],
        expected: "analisi comparativa, rischi, benefici e strategia di migrazione",
    },
    Playbook {
        id: "server_provision",
        all_of: &[
            &["prepara", "configura", "predisponi", "installa"],
            &["server", "macchina", "ambiente"],
            &[
                "ospitare",
                "eseguire l applicazione",
                "per questa applicazione",
                "per ospitare",
                "deploy",
            ],
        ],
        agents: &[SYSTEM, CODING, SECURITY],
        expected: "requisiti verificati, configurazione pianificata e ambiente validato",
    },
    Playbook {
        id: "env_only_failure",
        all_of: &[
            &["build", "test", "compilazione", "applicazione"],
            &[
                "fallisce",
                "non riesce",
                "si rompe",
                "va in errore",
                "non funziona",
            ],
            &["soltanto", "solo", "unicamente"],
            &["server", "produzione", "ambiente", "ci"],
        ],
        agents: &[CODING, SYSTEM],
        expected: "confronto tra ambienti e identificazione della causa effettiva",
    },
    Playbook {
        id: "cost_reduction",
        all_of: &[
            &["ridurre", "risparmiare", "tagliare", "diminuire"],
            &["costi", "spese", "bolletta"],
        ],
        agents: &[BUSINESS, SYSTEM, CODING],
        expected: "analisi economica e tecnica con alternative verificabili",
    },
    Playbook {
        id: "project_priorities",
        all_of: &[
            &[
                "tutti i miei progetti",
                "i miei progetti",
                "tutti i progetti",
            ],
            &[
                "conviene",
                "priorita",
                "primo",
                "terminare",
                "finire",
                "concludere",
            ],
        ],
        agents: &[MEMORY, CODING, BUSINESS],
        expected: "priorità motivate considerando avanzamento, complessità e valore",
    },
    Playbook {
        id: "automation_discovery",
        all_of: &[
            &[
                "ripeto",
                "ripetitive",
                "ripetute",
                "frequentemente",
                "di continuo",
            ],
            &["automazion", "automatizz", "proponi"],
        ],
        agents: &[MEMORY, AUTOMATION, SYSTEM],
        expected: "procedure candidate con frequenza, benefici e rischi",
    },
    Playbook {
        id: "software_safety",
        all_of: &[
            &[
                "programma",
                "software",
                "applicazione",
                "app$",
                "eseguibile",
            ],
            &[
                "sicuro",
                "sicura",
                "affidabile",
                "malware",
                "virus",
                "fidarmi",
            ],
            &[
                "installarlo",
                "installarla",
                "installare",
                "prima di",
                "scaricarlo",
                "eseguirlo",
            ],
        ],
        agents: &[RESEARCH, SECURITY, SYSTEM],
        expected: "verifica della provenienza e valutazione del rischio prima dell'installazione",
    },
    Playbook {
        id: "plan_tomorrow",
        all_of: &[
            &["domani", "settimana", "giornata"],
            &["organizza", "pianifica", "prepara"],
            &["impegni", "lavoro", "attivita", "appuntamenti"],
        ],
        agents: &[MEMORY, AUTOMATION, BUSINESS],
        expected: "pianificazione basata sulle informazioni realmente disponibili",
    },
    Playbook {
        id: "idea_design",
        all_of: &[
            &["idea", "progetto nuovo", "concetto"],
            &[
                "progettala",
                "progettalo",
                "progetta",
                "realizzarla",
                "realizzarlo",
                "fattibil",
            ],
            &["serve", "occorre", "dimmi", "cosa"],
        ],
        agents: &[CODING, RESEARCH, BUSINESS],
        expected: "requisiti, architettura, fattibilità, costi e piano operativo",
    },
    Playbook {
        id: "server_offline",
        all_of: &[
            &["server", "sito", "servizio", "macchina"],
            &[
                "offline",
                "non risponde",
                "e giu",
                "irraggiungibile",
                "down",
                "spento",
            ],
            &["ripristin", "riparalo", "rimettilo", "sistemalo", "risolvi"],
        ],
        agents: &[SYSTEM, SECURITY, AUTOMATION],
        expected: "diagnostica e ripristino entro le autorizzazioni con verifica finale",
    },
    Playbook {
        id: "review_todays_changes",
        all_of: &[
            &["modifiche", "cambiamenti", "commit", "lavoro fatto"],
            &["oggi", "ultime ore", "di ieri", "recenti"],
            &["verifica", "controlla", "rivedi", "riepiloga"],
        ],
        agents: &[CODING, MEMORY, SECURITY],
        expected: "riepilogo delle modifiche, test e criticità residue",
    },
];

/// L'archetipo composito della richiesta, se uno solo corrisponde (a parità vince il più specifico, cioè quello con
/// più famiglie di segnali).
pub fn playbook(message: &str) -> Option<&'static Playbook> {
    let plain = fold(message);
    PLAYBOOKS
        .iter()
        .filter(|p| {
            p.all_of
                .iter()
                .all(|group| group.iter().any(|cue| hit(&plain, cue)))
        })
        .max_by_key(|p| p.all_of.len())
}

/// Principale e supporto di una richiesta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Team {
    pub primary: &'static str,
    pub support: Vec<&'static str>,
}

fn fold(text: &str) -> String {
    let lowered: String = text
        .to_lowercase()
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
        .collect();
    format!(
        " {} ",
        lowered.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

fn hit(plain: &str, cue: &str) -> bool {
    match cue.strip_suffix('$') {
        Some(word) => plain.contains(&format!(" {word} ")),
        None if cue.ends_with(' ') => plain.contains(&format!(" {cue}")),
        None => {
            plain.contains(&format!(" {cue}"))
                || plain.contains(&format!("{cue} ")) && cue.contains(' ')
        }
    }
}

/// Punteggio per agente, in ordine di agente stabile.
pub fn scores(message: &str) -> Vec<(&'static str, u32)> {
    let plain = fold(message);
    let mut totals: Vec<(&'static str, u32)> = Vec::new();
    for Cue(cue, agent, weight) in CUES {
        if hit(&plain, cue) {
            match totals.iter_mut().find(|(a, _)| a == agent) {
                Some(entry) => entry.1 += weight,
                None => totals.push((agent, *weight)),
            }
        }
    }
    totals
}

/// La squadra per la richiesta, o `None` se nessun segnale riconosce un ambito.
pub fn team(message: &str) -> Option<Team> {
    if let Some(book) = playbook(message) {
        return Some(Team {
            primary: book.agents[0],
            support: book.agents[1..].to_vec(),
        });
    }
    let plain = fold(message);
    let mut totals = scores(message);
    if totals.is_empty() {
        return None;
    }
    // A parità vince l'agente col segnale singolo più forte; poi l'ordine fisso (automazione, memoria, …).
    let order = [
        AUTOMATION, MEMORY, CODING, SECURITY, BUSINESS, SYSTEM, RESEARCH,
    ];
    let strongest = |agent: &str| {
        CUES.iter()
            .filter(|Cue(c, a, _)| *a == agent && hit(&plain, c))
            .map(|Cue(_, _, w)| *w)
            .max()
            .unwrap_or(0)
    };
    totals.sort_by(|(a, sa), (b, sb)| {
        sb.cmp(sa).then(strongest(b).cmp(&strongest(a))).then(
            order
                .iter()
                .position(|x| x == a)
                .cmp(&order.iter().position(|x| x == b)),
        )
    });
    let primary = totals[0].0;
    let mut support: Vec<&'static str> = Vec::new();
    for (agent, _) in totals.iter().skip(1) {
        support.push(agent);
    }
    for (main, cue, extra) in IMPLIED {
        if *main == primary && hit(&plain, cue) && !support.contains(extra) && *extra != primary {
            support.push(extra);
        }
    }
    support.truncate(3);
    Some(Team { primary, support })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_beats_verb_and_schedules_win() {
        let t = team("Cerca una libreria che possa sostituire questa").unwrap();
        assert_eq!(t.primary, CODING);
        assert!(t.support.contains(&RESEARCH));
        assert_eq!(
            team("Ogni giorno verifica che il server sia operativo")
                .unwrap()
                .primary,
            AUTOMATION
        );
        assert_eq!(
            team("Ricostruisci tutte le decisioni prese sul progetto")
                .unwrap()
                .primary,
            MEMORY
        );
        assert!(team("grazie mille").is_none());
    }

    /// Frasi scritte dopo la taratura, mai usate per regolare i segnali: misurano la generalizzazione.
    #[test]
    fn unseen_requests_get_the_right_primary_agent() {
        let cases = [
            ("Quanto spazio mi resta sul disco?", SYSTEM),
            ("Cerca le ultime notizie su Rust", RESEARCH),
            ("Scrivi un test per questa funzione", CODING),
            ("Prepara un preventivo per il cliente Rossi", BUSINESS),
            ("Ricordami cosa avevamo deciso sul database", MEMORY),
            ("Ogni mattina mandami un riepilogo delle email", AUTOMATION),
            ("Ci sono password salvate in chiaro nel progetto?", SECURITY),
            ("Il mio PC si surriscalda, controlla", SYSTEM),
            ("Trova un'alternativa gratuita a Photoshop", RESEARCH),
            ("Quanto fatturato ho fatto questo mese?", BUSINESS),
            ("Correggi il bug nel modulo di login", CODING),
            ("Avvisami se il sito va offline", AUTOMATION),
        ];
        for (text, want) in cases {
            assert_eq!(team(text).map(|t| t.primary), Some(want), "{text}");
        }
    }

    /// Formulazioni nuove dello stesso tipo di richiesta composita (mai usate per tarare gli archetipi).
    #[test]
    fn composite_archetypes_generalise_to_new_wording() {
        let cases = [
            (
                "Cerca un nuovo cliente, studia la sua azienda e scrivi un'offerta su misura",
                "client_proposal",
            ),
            (
                "Il sito è offline: scopri la causa e rimettilo in piedi",
                "server_offline",
            ),
            (
                "Vorrei ridurre le spese dell'infrastruttura senza perdere funzioni",
                "cost_reduction",
            ),
            (
                "Controlla se questa app è sicura prima di scaricarla",
                "software_safety",
            ),
            (
                "Organizza la mia giornata di domani con tutti gli appuntamenti",
                "plan_tomorrow",
            ),
            ("Rivedi le modifiche di oggi", "review_todays_changes"),
        ];
        for (text, want) in cases {
            assert_eq!(playbook(text).map(|p| p.id), Some(want), "{text}");
        }
        // Una richiesta semplice non è composita.
        assert!(playbook("Controlla perché il Mac è diventato lento").is_none());
    }
}
