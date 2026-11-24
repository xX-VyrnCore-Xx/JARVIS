//! Strumenti di competenza per aziende, informatica e sicurezza difensiva (ADR-0117).
//! Tutti sono di calcolo o di sola lettura: nessuna rete, nessuna scrittura.

use std::io::Read;
use std::path::Path;

use serde_json::{json, Value};

use crate::tools::{resolve, ToolError};

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

// ------------------------------------------------------------------ CVSS v3.1

fn round_up(x: f64) -> f64 {
    let int = (x * 100_000.0).round() as i64;
    if int % 10_000 == 0 {
        int as f64 / 100_000.0
    } else {
        ((int / 10_000) + 1) as f64 / 10.0
    }
}

/// Punteggio base CVSS v3.1 da un vettore `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`.
pub fn cvss(vector: &str) -> Result<Value, ToolError> {
    let bad = || {
        err(
            "security.invalid_vector",
            "Vettore CVSS non valido: usa CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
        )
    };
    let mut parts = vector.trim().split('/');
    let head = parts.next().ok_or_else(bad)?;
    if head != "CVSS:3.1" && head != "CVSS:3.0" {
        return Err(bad());
    }
    let mut m = std::collections::HashMap::new();
    for part in parts {
        let (k, v) = part.split_once(':').ok_or_else(bad)?;
        if m.insert(k, v).is_some() {
            return Err(bad());
        }
    }
    let get = |k: &str| m.get(k).copied().ok_or_else(bad);
    let scope_changed = match get("S")? {
        "U" => false,
        "C" => true,
        _ => return Err(bad()),
    };
    let av = match get("AV")? {
        "N" => 0.85,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.2,
        _ => return Err(bad()),
    };
    let ac = match get("AC")? {
        "L" => 0.77,
        "H" => 0.44,
        _ => return Err(bad()),
    };
    let pr = match (get("PR")?, scope_changed) {
        ("N", _) => 0.85,
        ("L", false) => 0.62,
        ("L", true) => 0.68,
        ("H", false) => 0.27,
        ("H", true) => 0.5,
        _ => return Err(bad()),
    };
    let ui = match get("UI")? {
        "N" => 0.85,
        "R" => 0.62,
        _ => return Err(bad()),
    };
    let cia = |k: &str| -> Result<f64, ToolError> {
        match get(k)? {
            "H" => Ok(0.56),
            "L" => Ok(0.22),
            "N" => Ok(0.0),
            _ => Err(bad()),
        }
    };
    let iss = 1.0 - (1.0 - cia("C")?) * (1.0 - cia("I")?) * (1.0 - cia("A")?);
    let impact = if scope_changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15)
    } else {
        6.42 * iss
    };
    let exploitability = 8.22 * av * ac * pr * ui;
    let score = if impact <= 0.0 {
        0.0
    } else if scope_changed {
        round_up((1.08 * (impact + exploitability)).min(10.0))
    } else {
        round_up((impact + exploitability).min(10.0))
    };
    let severity = match score {
        0.0 => "nessuna",
        s if s < 4.0 => "bassa",
        s if s < 7.0 => "media",
        s if s < 9.0 => "alta",
        _ => "critica",
    };
    Ok(
        json!({"vector": vector.trim(), "base_score": score, "severity": severity,
              "exploitability": (exploitability * 10.0).round() / 10.0,
              "impact": (impact.max(0.0) * 10.0).round() / 10.0}),
    )
}

// ------------------------------------------------------------------ rete

/// Calcoli di sottorete IPv4 da una notazione CIDR (`192.168.1.0/24`).
pub fn subnet(cidr: &str) -> Result<Value, ToolError> {
    let bad = || {
        err(
            "it.invalid_cidr",
            "Indirizzo non valido: usa per esempio 192.168.1.0/24",
        )
    };
    let (addr, len) = cidr.trim().split_once('/').ok_or_else(bad)?;
    let ip: std::net::Ipv4Addr = addr.parse().map_err(|_| bad())?;
    let len: u32 = len.parse().map_err(|_| bad())?;
    if len > 32 {
        return Err(bad());
    }
    let mask: u32 = if len == 0 { 0 } else { u32::MAX << (32 - len) };
    let value = u32::from(ip);
    let network = value & mask;
    let broadcast = network | !mask;
    let (hosts, first, last) = match len {
        32 => (1u64, network, network),
        31 => (2, network, broadcast),
        _ => ((1u64 << (32 - len)) - 2, network + 1, broadcast - 1),
    };
    let text = |n: u32| std::net::Ipv4Addr::from(n).to_string();
    Ok(json!({
        "input": cidr.trim(), "network": text(network), "prefix": len, "netmask": text(mask),
        "wildcard": text(!mask), "broadcast": text(broadcast),
        "first_host": text(first), "last_host": text(last), "usable_hosts": hosts,
        "private": ip.is_private() || ip.is_loopback() || ip.is_link_local(),
    }))
}

// ------------------------------------------------------------------ integrità

pub fn sha256_hex(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// SHA-256 di un file della workspace, con confronto facoltativo con il valore atteso.
pub fn hash_file(workspace: &Path, args: &Value) -> Result<Value, ToolError> {
    let rel = args["path"].as_str().unwrap_or_default();
    let file = resolve(workspace, rel, true)?;
    if !file.is_file() {
        return Err(err(
            "security.not_a_file",
            "È richiesto un file della workspace",
        ));
    }
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    if size > 2 * 1024 * 1024 * 1024 {
        return Err(err("security.too_large", "Il file supera 2 GiB"));
    }
    let digest = sha256_hex(&file).map_err(|_| err("security.unreadable", "File non leggibile"))?;
    let mut out = json!({"path": rel, "algorithm": "sha256", "digest": digest, "bytes": size});
    if let Some(expected) = args["expected"].as_str() {
        let expected = expected.trim().to_ascii_lowercase();
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err(
                "security.invalid_digest",
                "Il valore atteso deve essere un SHA-256 di 64 caratteri esadecimali",
            ));
        }
        out["matches"] = json!(expected == digest);
    }
    Ok(out)
}

// ------------------------------------------------------------------ aziende

fn cents(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// IVA: `add` aggiunge l'imposta a un imponibile, `remove` scorpora da un totale.
pub fn vat(args: &Value) -> Result<Value, ToolError> {
    let amount = args["amount"]
        .as_f64()
        .ok_or_else(|| err("biz.invalid", "Importo mancante"))?;
    let rate = args["rate"].as_f64().unwrap_or(22.0);
    if !(0.0..=100.0).contains(&rate) || !amount.is_finite() || amount < 0.0 {
        return Err(err("biz.invalid", "Importo o aliquota fuori intervallo"));
    }
    let (net, gross) = match args["mode"].as_str().unwrap_or("add") {
        "add" => (amount, amount * (1.0 + rate / 100.0)),
        "remove" => (amount / (1.0 + rate / 100.0), amount),
        _ => return Err(err("biz.invalid", "Modalità non valida: add o remove")),
    };
    Ok(
        json!({"net": cents(net), "vat": cents(gross - net), "gross": cents(gross), "rate_percent": rate}),
    )
}

/// Rata di un finanziamento con ammortamento alla francese (rata costante).
pub fn loan(args: &Value) -> Result<Value, ToolError> {
    let principal = args["principal"].as_f64().unwrap_or(-1.0);
    let rate = args["annual_rate_percent"].as_f64().unwrap_or(-1.0);
    let months = args["months"].as_u64().unwrap_or(0);
    if principal.is_nan()
        || principal <= 0.0
        || !(0.0..=100.0).contains(&rate)
        || months == 0
        || months > 1200
    {
        return Err(err(
            "biz.invalid",
            "Capitale, tasso (0-100) o durata (1-1200 mesi) non validi",
        ));
    }
    let i = rate / 100.0 / 12.0;
    let n = months as f64;
    let payment = if i == 0.0 {
        principal / n
    } else {
        principal * i / (1.0 - (1.0 + i).powf(-n))
    };
    let total = payment * n;
    Ok(
        json!({"monthly_payment": cents(payment), "total_paid": cents(total),
              "total_interest": cents(total - principal), "months": months}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvss_matches_published_scores() {
        let score = |v: &str| cvss(v).unwrap()["base_score"].as_f64().unwrap();
        assert_eq!(score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"), 9.8);
        assert_eq!(score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H"), 10.0);
        assert_eq!(score("CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N"), 6.1);
        assert_eq!(score("CVSS:3.1/AV:L/AC:L/PR:L/UI:N/S:U/C:H/I:H/A:H"), 7.8);
        assert_eq!(score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N"), 0.0);
        assert_eq!(
            cvss("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").unwrap()["severity"],
            "critica"
        );
        for bad in [
            "",
            "CVSS:2.0/AV:N",
            "CVSS:3.1/AV:X/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
            "CVSS:3.1/AV:N/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
            "CVSS:3.1/AV:N",
        ] {
            assert!(cvss(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn subnets_are_computed() {
        let s = subnet("192.168.1.77/24").unwrap();
        assert_eq!(s["network"], "192.168.1.0");
        assert_eq!(s["broadcast"], "192.168.1.255");
        assert_eq!(s["first_host"], "192.168.1.1");
        assert_eq!(s["usable_hosts"], 254);
        assert_eq!(s["private"], true);
        assert_eq!(subnet("10.0.0.0/8").unwrap()["usable_hosts"], 16_777_214);
        assert_eq!(subnet("8.8.8.8/32").unwrap()["usable_hosts"], 1);
        assert_eq!(subnet("8.8.8.0/31").unwrap()["usable_hosts"], 2);
        assert_eq!(subnet("0.0.0.0/0").unwrap()["netmask"], "0.0.0.0");
        for bad in ["10.0.0.0", "10.0.0.0/33", "300.1.1.1/8", "a/8"] {
            assert!(subnet(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn files_are_hashed_and_compared() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-hash-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let ok = hash_file(
            &dir,
            &json!({"path": "a.txt", "expected": abc.to_uppercase()}),
        )
        .unwrap();
        assert_eq!(ok["digest"], abc);
        assert_eq!(ok["matches"], true);
        let no = hash_file(&dir, &json!({"path": "a.txt", "expected": "0".repeat(64)})).unwrap();
        assert_eq!(no["matches"], false);
        assert!(hash_file(&dir, &json!({"path": "a.txt", "expected": "xyz"})).is_err());
        assert!(hash_file(&dir, &json!({"path": "../x"})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vat_and_loans_are_computed() {
        let add = vat(&json!({"amount": 100.0})).unwrap();
        assert_eq!(
            (add["vat"].as_f64().unwrap(), add["gross"].as_f64().unwrap()),
            (22.0, 122.0)
        );
        let remove = vat(&json!({"amount": 122.0, "mode": "remove"})).unwrap();
        assert_eq!(
            (
                remove["net"].as_f64().unwrap(),
                remove["vat"].as_f64().unwrap()
            ),
            (100.0, 22.0)
        );
        assert!(vat(&json!({"amount": -1.0})).is_err());
        let l = loan(&json!({"principal": 100000.0, "annual_rate_percent": 3.5, "months": 240}))
            .unwrap();
        assert_eq!(l["monthly_payment"], 579.96);
        assert_eq!(
            loan(&json!({"principal": 1200.0, "annual_rate_percent": 0.0, "months": 12})).unwrap()
                ["monthly_payment"],
            100.0
        );
        assert!(
            loan(&json!({"principal": 0.0, "annual_rate_percent": 1.0, "months": 12})).is_err()
        );
    }
}
