//! Stampa l'esito di ogni fixture: `cargo run -q --example report -- <file.json>...`
fn main() {
    for path in std::env::args().skip(1) {
        let file = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let contract =
            jarvis_contracts::Contract::from_name(file.split('.').next().unwrap()).expect("schema");
        let doc = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let r = jarvis_contracts::validate(contract, &doc);
        println!(
            "{file}: schema={} semantic={:?}",
            r.schema.len(),
            r.semantic
        );
    }
}
