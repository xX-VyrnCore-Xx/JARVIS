//! Dataset di identità (domande su JARVIS) e di attivazione delle competenze: misurano le regole reali del Core.
//! Il test fallisce sotto la soglia fissata e stampa le discordanze.

use jarvis_core::datasets;

fn report(name: &str) -> serde_json::Value {
    let dir = std::env::temp_dir().join(format!("jarvis-selfdata-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = datasets::evaluate(&dir, name).expect("valutazione");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn identity_questions_are_recognised() {
    let r = report("jarvis-identity");
    println!("{r:#}");
    let accuracy = r["accuracy_percent"].as_f64().unwrap();
    assert!(accuracy >= 97.0, "identità: {accuracy}% — {}", r["misses"]);
}

#[test]
fn skills_activate_on_their_requests() {
    let r = report("jarvis-skills");
    println!("{r:#}");
    let accuracy = r["accuracy_percent"].as_f64().unwrap();
    assert!(
        accuracy >= 90.0,
        "competenze: {accuracy}% — {}",
        r["misses"]
    );
}
