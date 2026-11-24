//! Mascheramento locale dei dati sensibili in un testo (ADR-0122). Funzioni pure, senza rete.
//!
//! Riconosce: indirizzi e-mail, chiavi e segreti noti, numeri di carta (con controllo di Luhn), IBAN (con controllo
//! mod 97), codice fiscale italiano, numeri di telefono, altre lunghe sequenze di cifre, indirizzi IPv4 e i valori
//! che seguono etichette come «password», «PIN» o «CVV». Nomi, indirizzi postali e testo libero non sono
//! riconoscibili in modo affidabile: il mascheramento riduce l'esposizione, non la elimina.

use std::collections::BTreeMap;

use serde_json::{json, Value};

fn luhn(digits: &str) -> bool {
    let mut sum = 0;
    for (i, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

fn iban_valid(token: &str) -> bool {
    let t: String = token
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if !(15..=34).contains(&t.len()) || !t.is_ascii() {
        return false;
    }
    let b = t.as_bytes();
    if !(b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit())
    {
        return false;
    }
    if !b.iter().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let rearranged = format!("{}{}", &t[4..], &t[..4]);
    let mut rem: u32 = 0;
    for c in rearranged.chars() {
        let value = match c {
            '0'..='9' => c.to_digit(10).unwrap(),
            'A'..='Z' => c as u32 - 'A' as u32 + 10,
            _ => return false,
        };
        rem = if value >= 10 {
            (rem * 100 + value) % 97
        } else {
            (rem * 10 + value) % 97
        };
    }
    rem == 1
}

fn fiscal_code(token: &str) -> bool {
    let t = token.to_ascii_uppercase();
    let b = t.as_bytes();
    t.len() == 16
        && t.is_ascii()
        && b[..6].iter().all(u8::is_ascii_uppercase)
        && b[6..8].iter().all(u8::is_ascii_digit)
        && b[8].is_ascii_uppercase()
        && b[9..11].iter().all(u8::is_ascii_digit)
        && b[11].is_ascii_uppercase()
        && b[12..15].iter().all(u8::is_ascii_digit)
        && b[15].is_ascii_uppercase()
}

fn ipv4(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u16>().is_ok_and(|n| n <= 255)
        })
}

fn email(token: &str) -> bool {
    match token.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        None => false,
    }
}

const LABELS: [&str; 9] = [
    "password",
    "passwd",
    "pwd",
    "pin",
    "cvv",
    "cvc",
    "otp",
    "codice di verifica",
    "secret",
];

/// Maschera le etichette seguite da un valore: «Password: abc123» → «Password: [segreto]».
fn mask_labelled(line: &str, counts: &mut BTreeMap<&'static str, u32>) -> Option<String> {
    let lower = line.to_lowercase();
    for label in LABELS {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(label) {
            let start = from + pos;
            let end = start + label.len();
            let before_ok = lower[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            let rest = &line[end..];
            let after = rest.trim_start_matches([' ', '\t']);
            if before_ok && (after.starts_with(':') || after.starts_with('=')) {
                *counts.entry("segreto").or_default() += 1;
                let sep = rest.len() - after.len() + 1;
                return Some(format!("{}{} [segreto]", &line[..end], &rest[..sep]));
            }
            from = end;
        }
    }
    None
}

/// Testo mascherato e conteggio per categoria.
pub fn redact(text: &str) -> (String, Value) {
    let mut counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut out_lines = Vec::new();
    for line in text.lines() {
        if let Some(masked) = mask_labelled(line, &mut counts) {
            out_lines.push(masked);
            continue;
        }
        // Gruppi di cifre separati da spazi o trattini (carte e telefoni scritti a blocchi).
        let words: Vec<&str> = line.split(' ').collect();
        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        while i < words.len() {
            // Carta: fino a 5 gruppi consecutivi di sole cifre (o con trattini) che insieme fanno 13-19 cifre.
            let mut taken = 0;
            let mut digits = String::new();
            for (n, w) in words[i..].iter().take(5).enumerate() {
                let d: String = w.chars().filter(char::is_ascii_digit).collect();
                let pure = !w.is_empty() && w.chars().all(|c| c.is_ascii_digit() || c == '-');
                if !pure || d.is_empty() {
                    break;
                }
                digits.push_str(&d);
                if (13..=19).contains(&digits.len()) && luhn(&digits) {
                    taken = n + 1;
                }
                if digits.len() > 19 {
                    break;
                }
            }
            if taken > 0 {
                *counts.entry("carta").or_default() += 1;
                out.push("[carta]".into());
                i += taken;
                continue;
            }
            let word = words[i];
            // Parole singole: separa la punteggiatura ai margini.
            let core = word.trim_matches(|c: char| {
                !(c.is_alphanumeric() || matches!(c, '@' | '.' | '+' | '-' | '_'))
            });
            let lead = &word[..word.find(core).unwrap_or(0)];
            let trail = &word[lead.len() + core.len()..];
            let digits_only: String = core.chars().filter(char::is_ascii_digit).collect();
            let replacement = if core.is_empty() {
                None
            } else if email(core) {
                Some("email")
            } else if crate::cloud::contains_credentials(core) {
                Some("segreto")
            } else if iban_valid(core) {
                Some("iban")
            } else if fiscal_code(core) {
                Some("codice fiscale")
            } else if ipv4(core) {
                Some("ip")
            } else if digits_only.len() >= 9
                && core
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | '/'))
            {
                Some(if core.starts_with('+') || digits_only.len() <= 11 {
                    "telefono"
                } else {
                    "numero"
                })
            } else {
                None
            };
            match replacement {
                Some(kind) => {
                    *counts.entry(kind).or_default() += 1;
                    out.push(format!("{lead}[{kind}]{trail}"));
                }
                None => out.push(word.to_string()),
            }
            i += 1;
        }
        out_lines.push(out.join(" "));
    }
    let total: u32 = counts.values().sum();
    (
        out_lines.join("\n"),
        json!({"total": total, "by_kind": counts}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_and_financial_data_are_masked() {
        let text = "Scrivi a mario.rossi@example.com oppure al +39 333 1234567.\nIBAN IT60X0542811101000000123456 e carta 4111 1111 1111 1111\nCF RSSMRA85T10A562S, server 192.168.1.20\nPassword: hunter2\nPIN = 4821";
        let (masked, counts) = redact(text);
        for secret in [
            "mario.rossi@example.com",
            "IT60X0542811101000000123456",
            "4111",
            "RSSMRA85T10A562S",
            "192.168.1.20",
            "hunter2",
            "4821",
        ] {
            assert!(!masked.contains(secret), "{secret} in {masked}");
        }
        for tag in [
            "[email]",
            "[iban]",
            "[carta]",
            "[codice fiscale]",
            "[ip]",
            "[segreto]",
        ] {
            assert!(masked.contains(tag), "{tag} in {masked}");
        }
        assert!(counts["total"].as_u64().unwrap() >= 7, "{counts}");
    }

    #[test]
    fn ordinary_text_and_numbers_are_left_alone() {
        let text = "Riunione alle 15:30 del 24/11/2026, stanza 4. Totale 1.250,00 euro. Versione 1.13.290.";
        let (masked, counts) = redact(text);
        assert_eq!(masked, text);
        assert_eq!(counts["total"], 0);
    }

    #[test]
    fn validators_reject_lookalikes() {
        assert!(luhn("4111111111111111") && !luhn("4111111111111112"));
        assert!(
            iban_valid("IT60 X054 2811 1010 0000 0123 456")
                && !iban_valid("IT61X0542811101000000123456")
        );
        assert!(fiscal_code("RSSMRA85T10A562S") && !fiscal_code("RSSMRA85T10A562"));
        let (masked, _) = redact("ordine 4111 1111 1111 1112 confermato");
        assert!(
            masked.contains("4111"),
            "un numero che non supera Luhn non è una carta: {masked}"
        );
    }
}
