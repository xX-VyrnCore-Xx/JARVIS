//! Robustezza degli strumenti puri (ADR-0171): argomenti mancanti, di tipo sbagliato, estremi o ostili devono dare
//! un errore o un risultato, mai un panico. Solo strumenti senza effetti sul sistema.

use jarvis_core::store::Store;
use jarvis_core::tools::{execute, Context};
use jarvis_core::web::WebConfig;
use serde_json::{json, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

const PURE: &[&str] = &[
    "time.now",
    "text.count",
    "text.correct",
    "text.extract",
    "text.diff",
    "data.stats",
    "math.calc",
    "code.validate_json",
    "security.cvss",
    "it.subnet",
    "units.convert",
    "date.calc",
    "biz.vat",
    "biz.loan",
    "biz.margin",
    "biz.breakeven",
    "biz.npv",
    "biz.runway",
    "biz.unit_economics",
    "biz.priorities",
    "biz.cagr",
];

fn hostile() -> Vec<Value> {
    let long = "9".repeat(5000);
    vec![
        json!({}),
        json!(null),
        json!([]),
        json!("testo"),
        json!({"text": 5, "expression": [1], "cidr": {}, "vector": null}),
        json!({"expression": "1/0"}),
        json!({"expression": "((((((((((((((((((((1))))))))))))))))))))"}),
        json!({"expression": "9".repeat(4000)}),
        json!({"expression": "2^99999999"}),
        json!({"cidr": "0.0.0.0/0"}),
        json!({"cidr": "10.0.0.1/33"}),
        json!({"cidr": "::/0"}),
        json!({"vector": "CVSS:3.1/AV:N"}),
        json!({"text": long, "a": long, "b": long}),
        json!({"values": [], "numbers": []}),
        json!({"values": [1e308, 1e308, -1e308], "numbers": [1e308, 1e308]}),
        json!({"amount": -1.0, "rate": -5.0, "months": 0, "years": 0, "periods": 0}),
        json!({"amount": 1e308, "rate": 1e308, "months": 1000000, "years": 1000000}),
        json!({"price": 0, "cost": 0, "units": 0, "fixed": 0, "variable": 0}),
        json!({"flows": [], "rate": -1.0, "initial": 0}),
        json!({"value": f64::MAX, "from": "x", "to": "y"}),
        json!({"date": "9999-99-99", "days": i64::MAX, "operation": "add"}),
        json!({"text": "\u{0}\u{202e}\u{1F600}é\r\n\t", "old": "\u{0}", "new": "\u{1F600}"}),
    ]
}

#[test]
fn pure_tools_never_panic_on_hostile_arguments() {
    let dir = std::env::temp_dir().join(format!("jarvis-robust-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(Path::new(":memory:")).unwrap();
    let web = WebConfig::default();
    let ctx = Context {
        workspace: &dir,
        store: &store,
        web: &web,
    };
    let mut panics = Vec::new();
    for tool in PURE {
        for (index, args) in hostile().iter().enumerate() {
            let result = catch_unwind(AssertUnwindSafe(|| execute(tool, args, &ctx)));
            if result.is_err() {
                panics.push(format!("{tool} con argomento #{index}: {args:.80}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(panics.is_empty(), "panici:\n{}", panics.join("\n"));
}
