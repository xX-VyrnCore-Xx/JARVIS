//! Stesse fixture e stesse attese del test Python: prova di compatibilità Rust–Python.

use std::fs;
use std::path::{Path, PathBuf};

use jarvis_contracts::{audit_chain_errors, validate, Contract};
use serde_json::Value;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/contracts/fixtures")
}

fn fixtures(kind: &str) -> Vec<(String, Contract, Value)> {
    let mut out: Vec<_> = fs::read_dir(fixture_dir().join(kind))
        .expect("directory delle fixture")
        .map(|entry| entry.expect("voce").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let prefix = file.split('.').next().unwrap();
            let contract =
                Contract::from_name(prefix).unwrap_or_else(|| panic!("schema sconosciuto: {file}"));
            let doc = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            (file, contract, doc)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn stream(name: &str) -> Vec<Value> {
    fs::read_to_string(fixture_dir().join("streams").join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn every_contract_has_valid_and_invalid_fixtures() {
    for kind in ["valid", "invalid"] {
        let covered: Vec<Contract> = fixtures(kind).into_iter().map(|f| f.1).collect();
        for contract in Contract::ALL {
            assert!(
                covered.contains(&contract),
                "{} senza fixture {kind}",
                contract.name()
            );
        }
    }
}

#[test]
fn valid_fixtures_pass() {
    for (file, contract, doc) in fixtures("valid") {
        let report = validate(contract, &doc);
        assert!(report.is_valid(), "{file}: {report:?}");
    }
}

#[test]
fn invalid_fixtures_are_rejected_by_the_expected_layer() {
    for (file, contract, doc) in fixtures("invalid") {
        let report = validate(contract, &doc);
        if file.ends_with(".semantic.json") {
            assert!(
                report.schema.is_empty(),
                "{file} deve superare lo schema: {report:?}"
            );
            assert!(
                !report.semantic.is_empty(),
                "{file} doveva violare un invariante"
            );
        } else {
            assert!(
                !report.schema.is_empty(),
                "{file} doveva essere respinta dallo schema"
            );
        }
    }
}

#[test]
fn audit_chain_matches_python_hashes() {
    let events = stream("audit-event.valid.jsonl");
    for event in &events {
        assert!(validate(Contract::AuditEvent, event).is_valid());
    }
    assert_eq!(audit_chain_errors(&events), Vec::<String>::new());
}

#[test]
fn audit_chain_detects_tampering_and_deletion() {
    let tampered = audit_chain_errors(&stream("audit-event.tampered.jsonl"));
    assert!(
        tampered.contains(&"hash non corrispondente all'evento 1".to_string()),
        "{tampered:?}"
    );
    let deleted = audit_chain_errors(&stream("audit-event.deleted-event.jsonl"));
    assert!(
        deleted.contains(&"prev_hash errato all'evento 1".to_string()),
        "{deleted:?}"
    );
}
