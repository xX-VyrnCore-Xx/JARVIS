//! Valutazione del motore di intento (ADR-0173) con due insiemi **separati**: lo sviluppo (frasi del proprietario
//! su cui si sono regolate le regole) e la validazione (frasi nuove, mai usate per regolare). Il risultato della
//! validazione è la misura onesta della generalizzazione; la soglia protegge dalle regressioni.

use jarvis_core::intent::{classify, Kind};

fn load(data: &str) -> Vec<(String, String)> {
    data.lines()
        .filter_map(|l| l.split_once('|'))
        .map(|(m, k)| (m.trim().to_string(), k.trim().to_string()))
        .collect()
}

fn kind_id(kind: Kind) -> &'static str {
    kind.id()
}

fn measure(name: &str, set: &[(String, String)]) -> f64 {
    let mut wrong = Vec::new();
    for (message, expected) in set {
        let got = kind_id(classify(message).0);
        if got != expected {
            wrong.push(format!("{message:?}: atteso {expected}, ottenuto {got}"));
        }
    }
    let ok = set.len() - wrong.len();
    eprintln!("{name}: {ok}/{} tipi corretti", set.len());
    for w in &wrong {
        eprintln!("  ERRORE {w}");
    }
    100.0 * ok as f64 / set.len() as f64
}

#[test]
fn intent_kinds_on_development_and_validation_sets() {
    let dev = load(include_str!("fixtures/intent_dev.tsv"));
    let validation = load(include_str!("fixtures/intent_validation.tsv"));
    let dev_score = measure("sviluppo", &dev);
    let val_score = measure("validazione", &validation);
    assert!(dev_score >= 95.0, "sviluppo sotto il 95%: {dev_score:.1}");
    assert!(val_score >= 80.0, "validazione sotto l'80%: {val_score:.1}");
}
