//! Strumenti di direzione aziendale (ADR-0132): calcoli deterministici per decisioni da CEO e CFO.
//! Come `expert.rs` sono solo di calcolo: nessuna rete, nessuna scrittura, nessun dato personale.
//! Ogni numero che JARVIS riporta in un'analisi aziendale deve venire da qui (o da `math.calc`), non dal modello.

use serde_json::{json, Value};

use crate::tools::ToolError;

const LIMIT: f64 = 1e12;

fn err(message: impl Into<String>) -> ToolError {
    ToolError {
        code: "biz.invalid",
        message: message.into(),
    }
}

fn round(value: f64, decimals: i32) -> f64 {
    let factor = 10f64.powi(decimals);
    (value * factor).round() / factor
}

fn r2(value: f64) -> f64 {
    round(value, 2)
}

fn bounded(value: f64, label: &str, min: f64, max: f64) -> Result<f64, ToolError> {
    if value.is_finite() && value >= min && value <= max {
        Ok(value)
    } else {
        Err(err(format!("{label} fuori intervallo ({min} – {max})")))
    }
}

fn number(args: &Value, key: &str, label: &str) -> Result<f64, ToolError> {
    args[key]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| err(format!("{label} mancante o non numerico")))
}

fn optional(args: &Value, key: &str, label: &str) -> Result<Option<f64>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| err(format!("{label} non numerico"))),
    }
}

/// Margine e ricarico da costo e prezzo, oppure il prezzo che dà un margine obiettivo.
pub fn margin(args: &Value) -> Result<Value, ToolError> {
    let cost = bounded(number(args, "cost", "Costo")?, "Costo", 0.0, LIMIT)?;
    let price = optional(args, "price", "Prezzo")?;
    let target = optional(args, "target_margin_percent", "Margine obiettivo")?;
    let price = match (price, target) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(err(
                "Indica il prezzo oppure il margine obiettivo, non entrambi",
            ))
        }
        (Some(price), None) => bounded(price, "Prezzo", 0.0, LIMIT)?,
        (None, Some(margin)) => {
            if !(0.0..100.0).contains(&margin) {
                return Err(err("Il margine obiettivo deve essere da 0 a meno di 100"));
            }
            cost / (1.0 - margin / 100.0)
        }
    };
    if price <= 0.0 {
        return Err(err("Il prezzo deve essere maggiore di zero"));
    }
    let profit = price - cost;
    Ok(json!({
        "cost": r2(cost),
        "price": r2(price),
        "profit": r2(profit),
        "margin_percent": r2(profit / price * 100.0),
        "markup_percent": (cost > 0.0).then(|| r2(profit / cost * 100.0)),
    }))
}

/// Punto di pareggio (o di profitto obiettivo) in unità e ricavi, con margine di sicurezza opzionale.
pub fn breakeven(args: &Value) -> Result<Value, ToolError> {
    let fixed = bounded(
        number(args, "fixed_costs", "Costi fissi")?,
        "Costi fissi",
        0.0,
        LIMIT,
    )?;
    let price = bounded(number(args, "price", "Prezzo")?, "Prezzo", 0.0, LIMIT)?;
    let variable = bounded(
        number(args, "variable_cost", "Costo variabile")?,
        "Costo variabile",
        0.0,
        LIMIT,
    )?;
    let target = bounded(
        optional(args, "target_profit", "Profitto obiettivo")?.unwrap_or(0.0),
        "Profitto obiettivo",
        0.0,
        LIMIT,
    )?;
    if price <= 0.0 || variable >= price {
        return Err(err(
            "Il costo variabile deve essere inferiore al prezzo: altrimenti il pareggio non si raggiunge mai",
        ));
    }
    let contribution = price - variable;
    let units = ((fixed + target) / contribution).ceil();
    let expected = optional(args, "expected_units", "Unità attese")?;
    if expected.is_some_and(|value| value <= 0.0) {
        return Err(err("Le unità attese devono essere maggiori di zero"));
    }
    Ok(json!({
        "contribution_margin": r2(contribution),
        "contribution_margin_percent": r2(contribution / price * 100.0),
        "break_even_units": units as u64,
        "break_even_revenue": r2(units * price),
        "margin_of_safety_percent": expected.map(|value| r2((value - units) / value * 100.0)),
    }))
}

fn present_value(rate: f64, flows: &[f64]) -> f64 {
    flows
        .iter()
        .enumerate()
        .map(|(period, flow)| flow / (1.0 + rate).powi(period as i32))
        .sum()
}

/// Tasso interno di rendimento per bisezione tra -99% e +1000%; `None` se non c'è un cambio di segno.
fn internal_rate(flows: &[f64]) -> Option<f64> {
    let (mut low, mut high) = (-0.99_f64, 10.0_f64);
    let (mut f_low, f_high) = (present_value(low, flows), present_value(high, flows));
    if !f_low.is_finite() || !f_high.is_finite() || f_low * f_high > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (low + high) / 2.0;
        let f_mid = present_value(mid, flows);
        if f_mid * f_low <= 0.0 {
            high = mid;
        } else {
            low = mid;
            f_low = f_mid;
        }
    }
    Some((low + high) / 2.0)
}

/// Valore attuale netto, tasso interno di rendimento, payback e indice di profittabilità.
pub fn npv(args: &Value) -> Result<Value, ToolError> {
    let rate = bounded(
        number(args, "discount_rate_percent", "Tasso di sconto")?,
        "Tasso di sconto",
        -99.0,
        1000.0,
    )? / 100.0;
    let flows: Vec<f64> = args["cashflows"]
        .as_array()
        .filter(|items| (2..=60).contains(&items.len()))
        .ok_or_else(|| {
            err("Servono da 2 a 60 flussi di cassa (il primo è l'investimento iniziale)")
        })?
        .iter()
        .map(|item| {
            item.as_f64()
                .filter(|value| value.is_finite() && value.abs() <= LIMIT)
                .ok_or_else(|| err("Flusso di cassa non numerico o troppo grande"))
        })
        .collect::<Result<_, _>>()?;
    let value = present_value(rate, &flows);
    let mut cumulative = 0.0;
    let mut payback = None;
    for (period, flow) in flows.iter().enumerate() {
        let before = cumulative;
        cumulative += flow;
        if period > 0 && before < 0.0 && cumulative >= 0.0 && *flow > 0.0 {
            payback = Some(round((period - 1) as f64 + (-before) / flow, 2));
            break;
        }
    }
    let index = (flows[0] < 0.0).then(|| r2((value - flows[0]) / -flows[0]));
    let verdict = if value > 0.0 {
        "Il VAN è positivo: a questo tasso l'investimento crea valore."
    } else if value < 0.0 {
        "Il VAN è negativo: a questo tasso l'investimento distrugge valore."
    } else {
        "Il VAN è nullo: l'investimento rende esattamente il tasso richiesto."
    };
    Ok(json!({
        "npv": r2(value),
        "irr_percent": internal_rate(&flows).map(|irr| r2(irr * 100.0)),
        "payback_periods": payback,
        "profitability_index": index,
        "verdict": verdict,
    }))
}

/// Autonomia di cassa: mesi prima di esaurire la liquidità, con ricavi eventualmente in crescita.
pub fn runway(args: &Value) -> Result<Value, ToolError> {
    let cash = bounded(number(args, "cash", "Liquidità")?, "Liquidità", 0.0, LIMIT)?;
    let costs = bounded(
        number(args, "monthly_costs", "Costi mensili")?,
        "Costi mensili",
        0.0,
        LIMIT,
    )?;
    let revenue = bounded(
        optional(args, "monthly_revenue", "Ricavi mensili")?.unwrap_or(0.0),
        "Ricavi mensili",
        0.0,
        LIMIT,
    )?;
    let growth = bounded(
        optional(args, "revenue_growth_percent", "Crescita dei ricavi")?.unwrap_or(0.0),
        "Crescita dei ricavi",
        0.0,
        100.0,
    )? / 100.0;
    let net_burn = costs - revenue;
    let mut left = cash;
    let mut income = revenue;
    let mut months = None;
    for month in 1..=120 {
        let net = costs - income;
        let after = left - net;
        if after < 0.0 {
            months = Some((month - 1) as f64 + left / net);
            break;
        }
        left = after;
        income *= 1.0 + growth;
    }
    let status = match months {
        None => "sostenibile",
        Some(value) if value < 6.0 => "critico",
        Some(value) if value < 12.0 => "attenzione",
        Some(_) => "adeguato",
    };
    Ok(json!({
        "net_burn_monthly": r2(net_burn),
        "runway_months": months.map(|value| round(value, 1)),
        "status": status,
        "horizon_months": 120,
    }))
}

/// LTV, rapporto LTV/CAC e payback del costo di acquisizione di un cliente.
pub fn unit_economics(args: &Value) -> Result<Value, ToolError> {
    let arpu = bounded(
        number(args, "arpu_monthly", "Ricavo mensile per cliente")?,
        "Ricavo mensile per cliente",
        0.0,
        LIMIT,
    )?;
    let gross = bounded(
        number(args, "gross_margin_percent", "Margine lordo")?,
        "Margine lordo",
        0.0,
        100.0,
    )? / 100.0;
    let churn = bounded(
        number(args, "monthly_churn_percent", "Abbandono mensile")?,
        "Abbandono mensile",
        0.0,
        100.0,
    )?;
    let cac = bounded(number(args, "cac", "CAC")?, "CAC", 0.0, LIMIT)?;
    if arpu <= 0.0 || gross <= 0.0 || churn <= 0.0 {
        return Err(err(
            "Ricavo per cliente, margine lordo e abbandono devono essere maggiori di zero",
        ));
    }
    let monthly_margin = arpu * gross;
    let lifetime = 100.0 / churn;
    let ltv = monthly_margin * lifetime;
    let ratio = (cac > 0.0).then(|| ltv / cac);
    let verdict = match ratio {
        None => "Senza costo di acquisizione il rapporto non è definito.",
        Some(value) if value >= 3.0 => "LTV/CAC pari o superiore a 3: modello sostenibile.",
        Some(value) if value >= 1.0 => {
            "LTV/CAC tra 1 e 3: i clienti rendono più di quanto costano, ma con poco margine."
        }
        Some(_) => "LTV/CAC sotto 1: ogni cliente acquisito fa perdere denaro.",
    };
    Ok(json!({
        "customer_lifetime_months": round(lifetime, 1),
        "ltv": r2(ltv),
        "ltv_cac_ratio": ratio.map(r2),
        "cac_payback_months": (cac > 0.0).then(|| round(cac / monthly_margin, 1)),
        "verdict": verdict,
    }))
}

/// Classifica iniziative con il metodo RICE: portata × impatto × fiducia ÷ impegno.
pub fn priorities(args: &Value) -> Result<Value, ToolError> {
    let items = args["items"]
        .as_array()
        .filter(|items| (1..=30).contains(&items.len()))
        .ok_or_else(|| err("Servono da 1 a 30 iniziative"))?;
    let mut scored = Vec::new();
    for item in items {
        let name = item["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty() && name.chars().count() <= 80)
            .ok_or_else(|| err("Ogni iniziativa ha un nome di 1-80 caratteri"))?;
        let reach = bounded(number(item, "reach", "Portata")?, "Portata", 0.0, LIMIT)?;
        let impact = bounded(number(item, "impact", "Impatto")?, "Impatto", 0.0, 1000.0)?;
        let confidence = bounded(
            number(item, "confidence_percent", "Fiducia")?,
            "Fiducia",
            0.0,
            100.0,
        )?;
        let effort = number(item, "effort", "Impegno")?;
        if !(effort > 0.0 && effort <= LIMIT) {
            return Err(err("L'impegno deve essere maggiore di zero"));
        }
        scored.push((
            name.to_string(),
            reach * impact * (confidence / 100.0) / effort,
        ));
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let ranking: Vec<Value> = scored
        .iter()
        .enumerate()
        .map(|(index, (name, score))| json!({"rank": index + 1, "name": name, "score": r2(*score)}))
        .collect();
    Ok(json!({"method": "RICE", "ranking": ranking}))
}

/// Tasso di crescita annuo composto tra due valori, con proiezione opzionale.
pub fn cagr(args: &Value) -> Result<Value, ToolError> {
    let start = number(args, "start", "Valore iniziale")?;
    let end = number(args, "end", "Valore finale")?;
    let years = bounded(number(args, "years", "Anni")?, "Anni", 0.0, 200.0)?;
    if start <= 0.0 || end <= 0.0 || years <= 0.0 || start > LIMIT || end > LIMIT {
        return Err(err(
            "Valore iniziale, finale e anni devono essere maggiori di zero",
        ));
    }
    let rate = (end / start).powf(1.0 / years) - 1.0;
    let ahead = optional(args, "project_years", "Anni di proiezione")?
        .map(|value| bounded(value, "Anni di proiezione", 0.0, 100.0))
        .transpose()?;
    Ok(json!({
        "cagr_percent": round(rate * 100.0, 2),
        "total_growth_percent": round((end / start - 1.0) * 100.0, 2),
        "projected_value": ahead.map(|value| r2(end * (1.0 + rate).powf(value))),
        "projection_years": ahead,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(result: Result<Value, ToolError>) -> Value {
        result.unwrap_or_else(|e| panic!("{}", e.message))
    }

    #[test]
    fn margin_and_markup_are_computed_from_cost_and_price_or_from_a_target() {
        let out = value(margin(&json!({"cost": 60, "price": 100})));
        assert_eq!(out["profit"], 40.0);
        assert_eq!(out["margin_percent"], 40.0);
        assert_eq!(out["markup_percent"], 66.67);
        let out = value(margin(&json!({"cost": 60, "target_margin_percent": 40})));
        assert_eq!(out["price"], 100.0);
        assert!(margin(&json!({"cost": 60})).is_err());
        assert!(margin(&json!({"cost": 60, "price": 100, "target_margin_percent": 10})).is_err());
        assert!(margin(&json!({"cost": 60, "target_margin_percent": 100})).is_err());
        assert!(margin(&json!({"cost": -1, "price": 100})).is_err());
    }

    #[test]
    fn breakeven_rounds_units_up_and_reports_the_safety_margin() {
        let out = value(breakeven(
            &json!({"fixed_costs": 10000, "price": 50, "variable_cost": 30, "expected_units": 800}),
        ));
        assert_eq!(out["break_even_units"], 500);
        assert_eq!(out["break_even_revenue"], 25000.0);
        assert_eq!(out["contribution_margin_percent"], 40.0);
        assert_eq!(out["margin_of_safety_percent"], 37.5);
        let out = value(breakeven(
            &json!({"fixed_costs": 10000, "price": 50, "variable_cost": 30, "target_profit": 5}),
        ));
        assert_eq!(out["break_even_units"], 501);
        assert!(breakeven(&json!({"fixed_costs": 1, "price": 30, "variable_cost": 30})).is_err());
    }

    #[test]
    fn npv_irr_and_payback_match_a_worked_example() {
        let out = value(npv(
            &json!({"discount_rate_percent": 10, "cashflows": [-1000, 500, 500, 500]}),
        ));
        assert_eq!(out["npv"], 243.43);
        assert_eq!(out["irr_percent"], 23.38);
        assert_eq!(out["payback_periods"], 2.0);
        assert_eq!(out["profitability_index"], 1.24);
        // Senza cambio di segno non esiste un TIR.
        let out = value(npv(
            &json!({"discount_rate_percent": 5, "cashflows": [100, 100]}),
        ));
        assert!(out["irr_percent"].is_null());
        assert!(npv(&json!({"discount_rate_percent": 5, "cashflows": [100]})).is_err());
        assert!(npv(&json!({"discount_rate_percent": -100, "cashflows": [1, 2]})).is_err());
    }

    #[test]
    fn runway_counts_months_and_flags_a_sustainable_business() {
        let out = value(runway(
            &json!({"cash": 120000, "monthly_costs": 30000, "monthly_revenue": 10000}),
        ));
        assert_eq!(out["net_burn_monthly"], 20000.0);
        assert_eq!(out["runway_months"], 6.0);
        assert_eq!(out["status"], "attenzione");
        let out = value(runway(&json!({"cash": 50000, "monthly_costs": 30000})));
        assert_eq!(out["runway_months"], 1.7);
        assert_eq!(out["status"], "critico");
        let out = value(runway(
            &json!({"cash": 1000, "monthly_costs": 10000, "monthly_revenue": 12000}),
        ));
        assert_eq!(out["status"], "sostenibile");
        assert!(out["runway_months"].is_null());
        // La crescita dei ricavi allunga l'autonomia.
        let flat = value(runway(
            &json!({"cash": 100000, "monthly_costs": 20000, "monthly_revenue": 5000}),
        ));
        let growing = value(runway(&json!({"cash": 100000, "monthly_costs": 20000,
            "monthly_revenue": 5000, "revenue_growth_percent": 10})));
        assert!(growing["runway_months"].as_f64() > flat["runway_months"].as_f64());
    }

    #[test]
    fn unit_economics_gives_ltv_ratio_and_payback() {
        let out = value(unit_economics(
            &json!({"arpu_monthly": 100, "gross_margin_percent": 70,
            "monthly_churn_percent": 3, "cac": 500}),
        ));
        assert_eq!(out["customer_lifetime_months"], 33.3);
        assert_eq!(out["ltv"], 2333.33);
        assert_eq!(out["ltv_cac_ratio"], 4.67);
        assert_eq!(out["cac_payback_months"], 7.1);
        let out = value(unit_economics(
            &json!({"arpu_monthly": 10, "gross_margin_percent": 50,
            "monthly_churn_percent": 10, "cac": 100}),
        ));
        assert!(out["verdict"].as_str().unwrap().contains("sotto 1"));
        assert!(
            unit_economics(&json!({"arpu_monthly": 10, "gross_margin_percent": 50,
            "monthly_churn_percent": 0, "cac": 1}))
            .is_err()
        );
    }

    #[test]
    fn priorities_rank_by_rice_score_and_validate_the_input() {
        let out = value(priorities(&json!({"items": [
            {"name": "A", "reach": 1000, "impact": 2, "confidence_percent": 80, "effort": 4},
            {"name": "B", "reach": 500, "impact": 3, "confidence_percent": 100, "effort": 1},
            {"name": "C", "reach": 100, "impact": 1, "confidence_percent": 50, "effort": 10},
        ]})));
        let names: Vec<_> = out["ranking"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["B", "A", "C"]);
        assert_eq!(out["ranking"][0]["score"], 1500.0);
        assert_eq!(out["ranking"][1]["score"], 400.0);
        assert!(priorities(&json!({"items": []})).is_err());
        assert!(priorities(&json!({"items": [
            {"name": "X", "reach": 1, "impact": 1, "confidence_percent": 50, "effort": 0}]}))
        .is_err());
        assert!(priorities(&json!({"items": [
            {"name": "", "reach": 1, "impact": 1, "confidence_percent": 50, "effort": 1}]}))
        .is_err());
    }

    #[test]
    fn cagr_and_projection() {
        let out = value(cagr(
            &json!({"start": 100, "end": 200, "years": 5, "project_years": 2}),
        ));
        assert_eq!(out["cagr_percent"], 14.87);
        assert_eq!(out["total_growth_percent"], 100.0);
        assert_eq!(out["projected_value"], 263.9);
        assert!(cagr(&json!({"start": 0, "end": 200, "years": 5})).is_err());
        assert!(cagr(&json!({"start": 100, "end": 200, "years": 0})).is_err());
    }
}
