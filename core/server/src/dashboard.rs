//! Dati della Plancia (ADR-0181): una sola chiamata con tutto ciò che le schede mostrano — specialisti attivi, piani in
//! corso, cose da fare, notifiche, contatori di attività — e il meteo, che è **facoltativo e acconsentito**: il nome
//! della località esce dal computer (Open-Meteo), quindi la Plancia lo cerca solo se il proprietario ha scelto una
//! località e non ne ha nessuna di predefinita. Nessun dato personale è scritto nel codice o nel repository.

use std::path::Path;

use serde_json::{json, Value};

const FILE: &str = "dashboard.json";
/// Il meteo si rilegge al massimo ogni mezz'ora.
pub const WEATHER_TTL_S: u64 = 1800;

/// Località scelta dal proprietario per il meteo, se c'è.
pub fn weather_place(dir: &Path) -> Option<String> {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["weather_place"].as_str().map(str::to_string))
        .filter(|p| !p.trim().is_empty())
}

/// Imposta o toglie (`null`, stringa vuota) la località del meteo.
pub fn set_weather_place(dir: &Path, patch: &Value) -> Result<Value, String> {
    let Some(value) = patch.get("weather_place") else {
        return Err("manca «weather_place»".into());
    };
    let place = match value {
        Value::Null => None,
        Value::String(s) if s.trim().is_empty() => None,
        Value::String(s) => {
            let s = s.trim();
            if s.chars().count() > 80 || s.chars().any(char::is_control) {
                return Err("località troppo lunga o con caratteri non validi".into());
            }
            if crate::cloud::contains_credentials(s) {
                return Err("la località sembra contenere una credenziale".into());
            }
            Some(s.to_string())
        }
        _ => return Err("«weather_place» deve essere un testo o null".into()),
    };
    let doc = json!({"weather_place": place});
    std::fs::write(dir.join(FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(
        json!({"weather_place": place, "note": "il nome della località viene inviato a Open-Meteo per il meteo"}),
    )
}

/// Percentuale di avanzamento di un piano.
pub fn percent(done: usize, steps: usize) -> u32 {
    if steps == 0 {
        0
    } else {
        ((done as f64 / steps as f64) * 100.0).round() as u32
    }
}

/// Vista breve del meteo per la scheda, dal risultato dello strumento `weather.forecast`.
pub fn weather_view(raw: &Value) -> Value {
    json!({
        "configured": true,
        "place": raw["place"], "temperature_c": raw["current"]["temperature_c"],
        "sky": raw["current"]["sky"], "min_c": raw["daily"][0]["min_c"], "max_c": raw["daily"][0]["max_c"],
        "retrieved_at": raw["retrieved_at"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jarvis-dash-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_weather_place_is_opt_in_validated_and_removable() {
        let d = dir();
        assert!(weather_place(&d).is_none(), "nessuna località predefinita");
        assert!(set_weather_place(&d, &json!({"weather_place": 5})).is_err());
        assert!(set_weather_place(&d, &json!({"weather_place": "x".repeat(81)})).is_err());
        assert!(set_weather_place(
            &d,
            &json!({"weather_place": "chiave sk-abcdefghijklmnopqrstuvwxyz0123456789"})
        )
        .is_err());
        assert!(set_weather_place(&d, &json!({})).is_err());
        set_weather_place(&d, &json!({"weather_place": "  Torino "})).unwrap();
        assert_eq!(weather_place(&d).as_deref(), Some("Torino"));
        set_weather_place(&d, &json!({"weather_place": null})).unwrap();
        assert!(weather_place(&d).is_none());
    }

    #[test]
    fn progress_is_a_rounded_percentage_and_never_divides_by_zero() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(3, 4), 75);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(4, 4), 100);
    }

    #[test]
    fn the_weather_view_keeps_only_what_the_card_shows() {
        let raw = json!({"place": "Roma", "current": {"temperature_c": 12.4, "sky": "nuvoloso", "humidity": 80},
            "daily": [{"min_c": 6, "max_c": 14}], "retrieved_at": "2026-01-01T00:00:00Z", "country": "Italia"});
        let v = weather_view(&raw);
        assert_eq!(v["place"], "Roma");
        assert_eq!(v["max_c"], 14);
        assert!(v.get("country").is_none() && v.get("humidity").is_none());
    }
}
