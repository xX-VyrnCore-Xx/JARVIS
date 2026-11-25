//! Strumenti di analisi di testo e numeri (ADR-0150): statistiche, estrazione di elementi e confronto di testi.
//! Come `expert.rs` sono solo di calcolo: nessuna rete, nessuna scrittura, nessun dato personale salvato.

use serde_json::{json, Value};

use crate::tools::ToolError;

const MAX_VALUES: usize = 5_000;
const MAX_LINES: usize = 400;
const MAX_ITEMS: usize = 100;

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn round(value: f64, decimals: i32) -> f64 {
    let factor = 10f64.powi(decimals);
    (value * factor).round() / factor
}

/// Numeri in un testo: separati da spazi, virgole, punti e virgola o righe. Accetta la virgola decimale
/// («3,5») solo quando non separa più numeri sulla stessa riga.
fn numbers_in(text: &str) -> Vec<f64> {
    let mut out = Vec::new();
    for line in text.lines() {
        let decimal_comma =
            !line.contains(';') && line.matches(',').count() == 1 && !line.contains(' ');
        let normalized = if decimal_comma {
            line.replace(',', ".")
        } else {
            line.to_string()
        };
        for token in normalized.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            let token = token
                .trim_matches(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'));
            if let Ok(value) = token.parse::<f64>() {
                if value.is_finite() {
                    out.push(value);
                }
            }
        }
    }
    out
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = p * (sorted.len() - 1) as f64;
    let (low, high) = (rank.floor() as usize, rank.ceil() as usize);
    sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f64)
}

/// Statistiche descrittive di una lista di numeri (`values`) o dei numeri trovati in un testo (`text`).
pub fn stats(args: &Value) -> Result<Value, ToolError> {
    let mut values: Vec<f64> = match args["values"].as_array() {
        Some(list) => list.iter().filter_map(Value::as_f64).collect(),
        None => numbers_in(args["text"].as_str().unwrap_or_default()),
    };
    if values.is_empty() {
        return Err(err(
            "data.invalid",
            "Servono dei numeri: indica «values» o un testo con numeri",
        ));
    }
    if values.len() > MAX_VALUES {
        return Err(err(
            "data.invalid",
            format!("Al massimo {MAX_VALUES} valori"),
        ));
    }
    if values.iter().any(|v| v.abs() > 1e15) {
        return Err(err("data.invalid", "Valori troppo grandi (massimo 1e15)"));
    }
    let n = values.len() as f64;
    let sum: f64 = values.iter().sum();
    let mean = sum / n;
    let squares: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    values.sort_by(f64::total_cmp);
    let q1 = percentile(&values, 0.25);
    let q3 = percentile(&values, 0.75);
    let iqr = q3 - q1;
    let outliers: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| *v < q1 - 1.5 * iqr || *v > q3 + 1.5 * iqr)
        .take(20)
        .collect();
    Ok(json!({
        "count": values.len(),
        "sum": round(sum, 4),
        "mean": round(mean, 4),
        "median": round(percentile(&values, 0.5), 4),
        "min": values[0],
        "max": values[values.len() - 1],
        "range": round(values[values.len() - 1] - values[0], 4),
        "q1": round(q1, 4),
        "q3": round(q3, 4),
        "std_dev_population": round((squares / n).sqrt(), 4),
        "std_dev_sample": (values.len() > 1).then(|| round((squares / (n - 1.0)).sqrt(), 4)),
        "outliers": outliers,
    }))
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if list.len() < MAX_ITEMS && !list.contains(&item) {
        list.push(item);
    }
}

fn trim_edges(token: &str) -> &str {
    token.trim_matches(|c: char| ".,;:!?()[]{}<>\"'«»".contains(c))
}

fn is_email(token: &str) -> bool {
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn is_url(token: &str) -> bool {
    (token.starts_with("http://") || token.starts_with("https://"))
        && token.len() > 8
        && token[8..].contains('.')
}

/// Date numeriche `2026-11-25`, `25/11/2026`, `25.11.2026`.
fn is_date(token: &str) -> bool {
    let parts: Vec<&str> = token.split(['-', '/', '.']).collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && (parts[0].len() == 4 || parts[2].len() == 4 || parts[2].len() == 2)
        && parts.iter().map(|p| p.len()).sum::<usize>() >= 8
}

/// Numeri di telefono: da 8 a 15 cifre, con spazi, trattini o prefisso «+».
fn phones_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, out: &mut Vec<String>| {
        let digits = current.chars().filter(char::is_ascii_digit).count();
        let candidate = current.trim().to_string();
        if (8..=15).contains(&digits) && !candidate.contains(['.', '/']) && !is_date(&candidate) {
            push_unique(out, candidate);
        }
        current.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit()
            || (c == '+' && current.trim().is_empty())
            || ((c == ' ' || c == '-') && !current.is_empty())
        {
            current.push(c);
        } else {
            flush(&mut current, &mut out);
        }
    }
    flush(&mut current, &mut out);
    out
}

/// Estrae email, indirizzi web, date, hashtag o telefoni da un testo.
pub fn extract(args: &Value) -> Result<Value, ToolError> {
    let text = args["text"].as_str().unwrap_or_default();
    let kind = args["kind"].as_str().unwrap_or("all");
    if !["all", "email", "url", "date", "hashtag", "phone"].contains(&kind) {
        return Err(err(
            "text.invalid",
            "kind: all, email, url, date, hashtag o phone",
        ));
    }
    let want = |k: &str| kind == "all" || kind == k;
    let (mut emails, mut urls, mut dates, mut hashtags) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for token in text.split_whitespace() {
        let token = trim_edges(token);
        if want("email") && is_email(token) {
            push_unique(&mut emails, token.to_string());
        } else if want("url") && is_url(token) {
            push_unique(&mut urls, token.to_string());
        } else if want("date") && is_date(token) {
            push_unique(&mut dates, token.to_string());
        } else if want("hashtag")
            && token.len() > 1
            && token.starts_with('#')
            && token[1..].chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            push_unique(&mut hashtags, token.to_string());
        }
    }
    let phones = if want("phone") {
        phones_in(text)
    } else {
        Vec::new()
    };
    Ok(json!({
        "emails": emails, "urls": urls, "dates": dates, "hashtags": hashtags, "phones": phones,
        "total": emails.len() + urls.len() + dates.len() + hashtags.len() + phones.len(),
    }))
}

/// Confronto riga per riga di due testi (sottosequenza comune più lunga): righe tolte, aggiunte e somiglianza.
pub fn diff(args: &Value) -> Result<Value, ToolError> {
    let a: Vec<&str> = args["a"].as_str().unwrap_or_default().lines().collect();
    let b: Vec<&str> = args["b"].as_str().unwrap_or_default().lines().collect();
    if a.len() > MAX_LINES || b.len() > MAX_LINES {
        return Err(err(
            "text.invalid",
            format!("Al massimo {MAX_LINES} righe per testo"),
        ));
    }
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            removed.push(json!({"line": i + 1, "text": a[i]}));
            i += 1;
        } else {
            added.push(json!({"line": j + 1, "text": b[j]}));
            j += 1;
        }
    }
    for (k, line) in a.iter().enumerate().skip(i) {
        removed.push(json!({"line": k + 1, "text": line}));
    }
    for (k, line) in b.iter().enumerate().skip(j) {
        added.push(json!({"line": k + 1, "text": line}));
    }
    let common = table[0][0];
    let total = a.len().max(b.len());
    Ok(json!({
        "unchanged": common,
        "removed": removed,
        "added": added,
        "identical": removed.is_empty() && added.is_empty(),
        "similarity_percent": if total == 0 { 100.0 } else { round(common as f64 * 100.0 / total as f64, 1) },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_are_computed_and_outliers_are_flagged() {
        let out = stats(&json!({"values": [2, 4, 4, 4, 5, 5, 7, 9]})).unwrap();
        assert_eq!(out["count"], 8);
        assert_eq!(out["mean"], 5.0);
        assert_eq!(out["median"], 4.5);
        assert_eq!(out["std_dev_population"], 2.0);
        let text = stats(&json!({"text": "Vendite: 10, 12; 11\n13 10 400"})).unwrap();
        assert_eq!(text["count"], 6);
        assert_eq!(text["max"], 400.0);
        assert_eq!(text["outliers"], json!([400.0]));
        assert_eq!(stats(&json!({"text": "3,5"})).unwrap()["mean"], 3.5);
        assert_eq!(
            stats(&json!({"values": [7]})).unwrap()["std_dev_sample"],
            Value::Null
        );
        assert!(stats(&json!({"text": "nessun numero"})).is_err());
        assert!(stats(&json!({"values": [1e20]})).is_err());
    }

    #[test]
    fn items_are_extracted_without_duplicates() {
        let text = "Scrivi a mario.rossi@esempio.it o (info@esempio.it), vedi https://esempio.org/pagina. \
                    Scadenza 2026-11-30 e 05/12/2026. #Budget #q4 Chiama +39 02 1234 5678. mario.rossi@esempio.it";
        let all = extract(&json!({"text": text})).unwrap();
        assert_eq!(
            all["emails"],
            json!(["mario.rossi@esempio.it", "info@esempio.it"])
        );
        assert_eq!(all["urls"], json!(["https://esempio.org/pagina"]));
        assert_eq!(all["dates"], json!(["2026-11-30", "05/12/2026"]));
        assert_eq!(all["hashtags"], json!(["#Budget", "#q4"]));
        assert_eq!(all["phones"], json!(["+39 02 1234 5678"]));
        let only = extract(&json!({"text": text, "kind": "email"})).unwrap();
        assert_eq!(only["urls"], json!([]));
        assert!(extract(&json!({"text": "x", "kind": "carta"})).is_err());
    }

    #[test]
    fn texts_are_compared_by_line() {
        let out = diff(&json!({"a": "uno\ndue\ntre", "b": "uno\ndue bis\ntre\nquattro"})).unwrap();
        assert_eq!(out["unchanged"], 2);
        assert_eq!(out["removed"], json!([{"line": 2, "text": "due"}]));
        assert_eq!(
            out["added"],
            json!([{"line": 2, "text": "due bis"}, {"line": 4, "text": "quattro"}])
        );
        assert_eq!(out["identical"], false);
        assert_eq!(
            diff(&json!({"a": "x", "b": "x"})).unwrap()["similarity_percent"],
            100.0
        );
        let big = "riga\n".repeat(401);
        assert!(diff(&json!({"a": big, "b": ""})).is_err());
    }
}
