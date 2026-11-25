//! Valutazione dell'instradamento (ADR-0171): corpus sintetico di richieste reali in italiano con l'agente atteso.
//! Misura l'accuratezza, fallisce sotto la soglia e stampa gli errori. Il corpus guida la messa a punto delle
//! parole chiave: ogni errore corretto in `agents.rs` resta qui come caso di regressione.

use jarvis_core::agents::{route, GENERAL};

const CORPUS: &[(&str, &str)] = &[
    // research
    ("cerca sul web le ultime notizie su Rust", "research"),
    ("fai una ricerca sui migliori router wifi", "research"),
    ("apri https://example.org e riassumi", "research"),
    ("cosa dice wikipedia su Alan Turing", "research"),
    ("naviga su example.org e dimmi il titolo", "research"),
    ("cerca online un ristorante a Torino", "research"),
    ("quali sono le notizie di oggi", "research"),
    // system
    ("quali processi usano più CPU", "system"),
    ("il computer è lento, controlla cosa rallenta", "system"),
    ("mostrami le informazioni di sistema", "system"),
    ("quanta RAM sto usando", "system"),
    ("calcola la sottorete 192.168.1.0/24", "system"),
    ("apri il task manager", "system"),
    ("quanto spazio libero ho sul disco", "system"),
    ("che versione del sistema operativo ho", "system"),
    // automation
    ("ogni giorno alle 9 ricordami di fare backup", "automation"),
    ("pianifica un controllo ogni ora", "automation"),
    ("crea un'automazione per i report settimanali", "automation"),
    ("tra 10 minuti avvisami", "automation"),
    ("ogni settimana invia il riepilogo", "automation"),
    // memory
    ("ricorda che il mio gruppo sanguigno è 0+", "memory"),
    ("cosa ricordi di me", "memory"),
    ("dimentica quello che ti ho detto sul lavoro", "memory"),
    ("preferisco il caffè senza zucchero", "memory"),
    ("ricordati di comprare il latte", "memory"),
    // business
    ("calcola l'iva al 22% su 1000 euro", "business"),
    ("quanto costa la rata del mutuo di 150000 euro", "business"),
    ("calcola il margine su un prodotto da 80 euro", "business"),
    ("qual è il break even con costi fissi 5000", "business"),
    ("prepara un business plan per un'app", "business"),
    ("analizza il cash flow dei prossimi sei mesi", "business"),
    ("calcola il ROI di questo investimento", "business"),
    ("quanto è il fatturato medio mensile", "business"),
    ("calcola il CAGR dal 2020 al 2025", "business"),
    ("qual è il runway con 200000 euro", "business"),
    ("fai un'analisi swot della mia azienda", "business"),
    ("prepara l'ordine del giorno del consiglio", "business"),
    // coding
    ("scrivi uno script python per rinominare i file", "coding"),
    ("fai il refactoring di questa funzione", "coding"),
    ("fai una code review di questo codice", "coding"),
    ("valida il json: {\"a\":1}", "coding"),
    ("debug di questo errore in javascript", "coding"),
    ("scrivi test unitari per la funzione somma", "coding"),
    ("compila il progetto rust", "coding"),
    ("esegui git status", "coding"),
    (
        "scrivi un programma in c che stampa i primi dieci numeri primi",
        "coding",
    ),
    // monitoring / sicurezza
    ("calcola il cvss 9.8", "monitoring"),
    (
        "controlla la postura di sicurezza del computer",
        "monitoring",
    ),
    ("c'è una vulnerabilità nota nel mio router", "monitoring"),
    ("come rispondo a un attacco ransomware", "monitoring"),
    ("ho ricevuto un'email di phishing", "monitoring"),
    ("fai un audit di sicurezza", "monitoring"),
    ("quanta batteria mi resta", "system"),
    ("quanta memoria sto usando", GENERAL), // ambigua: memoria JARVIS o RAM
    ("scrivi una poesia sul disco volante", GENERAL),
    // generalista: nessuno o più ambiti
    ("ciao, come stai?", GENERAL),
    ("grazie mille", GENERAL),
    ("che ore sono", GENERAL),
    ("raccontami una barzelletta", GENERAL),
    ("leggi il file note.txt", GENERAL),
    ("traduci in inglese: buongiorno a tutti", GENERAL),
    ("scrivi una email di auguri a Marco", GENERAL),
    ("ogni ora controlla le informazioni di sistema", GENERAL),
    ("riassumi questo testo in tre righe", GENERAL),
    ("quanto fa 15 per 23", GENERAL),
];

#[test]
fn routing_accuracy_stays_above_threshold() {
    let mut wrong = Vec::new();
    for (message, expected) in CORPUS {
        let got = route(message).map_or(GENERAL, |a| a.name);
        if got != *expected {
            wrong.push(format!("{message:?}: atteso {expected}, ottenuto {got}"));
        }
    }
    let total = CORPUS.len();
    let accuracy = 100.0 * (total - wrong.len()) as f64 / total as f64;
    eprintln!(
        "accuratezza instradamento: {accuracy:.1}% ({}/{total})",
        total - wrong.len()
    );
    for line in &wrong {
        eprintln!("  ERRORE {line}");
    }
    assert!(
        wrong.is_empty(),
        "{} errori su {total}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
