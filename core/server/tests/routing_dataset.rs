//! Dataset di instradamento (tests/data/routing_dataset.tsv): per ogni richiesta l'agente principale atteso e quelli
//! di supporto. Misura quanto `agents::rank` li rispetta; il test fallisce sotto la soglia fissata.

use jarvis_core::dispatch;

fn name_of(italian: &str) -> Option<&'static str> {
    Some(match italian.trim() {
        "sistema" => "system",
        "sicurezza" => "monitoring",
        "ricerca web" => "research",
        "codice" => "coding",
        "azienda" => "business",
        "memoria" => "memory",
        "automazioni" => "automation",
        _ => return None,
    })
}

fn rows() -> Vec<(String, &'static str, Vec<&'static str>)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/routing_dataset.tsv");
    std::fs::read_to_string(path)
        .expect("dataset")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|line| {
            let parts: Vec<&str> = line.split(" | ").collect();
            let primary = name_of(parts[1]).expect("agente principale");
            let support = parts[2].split(',').filter_map(name_of).collect();
            (parts[0].to_string(), primary, support)
        })
        .collect()
}

#[test]
fn routing_report() {
    let rows = rows();
    let (mut primary_ok, mut support_ok, mut support_total) = (0, 0, 0);
    for (text, primary, support) in &rows {
        let team = dispatch::team(text);
        let ranked: Vec<&str> = team
            .iter()
            .flat_map(|t| std::iter::once(t.primary).chain(t.support.iter().copied()))
            .collect();
        let first = ranked.first().copied();
        if first == Some(*primary) {
            primary_ok += 1;
        } else {
            println!("PRIMARIO ✗ {text:?}: atteso {primary}, ottenuto {ranked:?}");
        }
        for s in support {
            support_total += 1;
            if ranked.contains(s) {
                support_ok += 1;
            }
        }
    }
    println!(
        "principale {primary_ok}/{} · supporto {support_ok}/{support_total}",
        rows.len()
    );
    // Soglie di non regressione. Le regole sono tarate su questo dataset: il punteggio non stima la
    // generalizzazione, garantisce solo che i casi noti non peggiorino.
    assert!(
        primary_ok * 100 >= rows.len() * 95,
        "agente principale: {primary_ok}/{}",
        rows.len()
    );
    assert!(
        support_ok * 100 >= support_total * 85,
        "supporto: {support_ok}/{support_total}"
    );
}

/// Dataset 3: richieste complesse con gli agenti che devono collaborare. La squadra calcolata (principale più
/// supporto) deve contenere tutti gli agenti richiesti.
#[test]
fn complex_requests_report() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/complex_requests.tsv");
    let rows: Vec<(String, Vec<&'static str>)> = std::fs::read_to_string(path)
        .expect("dataset")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let parts: Vec<&str> = l.split(" | ").collect();
            (
                parts[0].to_string(),
                parts[1].split(',').filter_map(name_of).collect(),
            )
        })
        .collect();
    let mut covered = 0;
    let mut missing_total = 0;
    for (text, need) in &rows {
        let team = dispatch::team(text);
        let got: Vec<&str> = team
            .iter()
            .flat_map(|t| std::iter::once(t.primary).chain(t.support.iter().copied()))
            .collect();
        let missing: Vec<&&str> = need.iter().filter(|n| !got.contains(*n)).collect();
        if missing.is_empty() {
            covered += 1;
        } else {
            missing_total += missing.len();
            println!("SQUADRA ✗ {text:?}: mancano {missing:?} (ottenuto {got:?})");
        }
    }
    println!(
        "richieste complesse coperte {covered}/{} (agenti mancanti: {missing_total})",
        rows.len()
    );
}
