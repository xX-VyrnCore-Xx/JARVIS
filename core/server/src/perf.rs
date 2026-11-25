//! Profili di risorse (ADR-0186): Eco, Bilanciato, Prestazioni e Massimo. Ogni profilo regola solo limiti che il
//! Core applica davvero: passi dell'agente per richiesta, agenti temporanei per richiesta e frequenza delle
//! istantanee delle metriche. Nessun profilo allenta permessi, conferme, audit o controlli di sicurezza: le
//! differenze riguardano soltanto quanto lavoro e quanta attività di fondo JARVIS consuma.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

const FILE: &str = "performance.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Eco,
    Balanced,
    Performance,
    Maximum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_steps: usize,
    pub max_subagents: usize,
    pub metrics_every: Duration,
}

impl Profile {
    pub const ALL: [Profile; 4] = [
        Profile::Eco,
        Profile::Balanced,
        Profile::Performance,
        Profile::Maximum,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Profile::Eco => "eco",
            Profile::Balanced => "balanced",
            Profile::Performance => "performance",
            Profile::Maximum => "maximum",
        }
    }

    pub fn parse(text: &str) -> Option<Profile> {
        Profile::ALL
            .into_iter()
            .find(|p| p.id() == text.trim().to_lowercase())
    }

    pub fn label(self) -> &'static str {
        match self {
            Profile::Eco => "Eco",
            Profile::Balanced => "Bilanciato",
            Profile::Performance => "Prestazioni",
            Profile::Maximum => "Massimo",
        }
    }

    pub fn limits(self) -> Limits {
        let (max_steps, max_subagents, secs) = match self {
            Profile::Eco => (3, 1, 900),
            Profile::Balanced => (4, 2, 300),
            Profile::Performance => (6, 3, 120),
            Profile::Maximum => (8, 4, 60),
        };
        Limits {
            max_steps,
            max_subagents,
            metrics_every: Duration::from_secs(secs),
        }
    }
}

/// Profilo scelto dal proprietario; se il file manca o è illeggibile vale «Bilanciato» (i limiti storici).
pub fn current(dir: &Path) -> Profile {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["profile"].as_str().and_then(Profile::parse))
        .unwrap_or(Profile::Balanced)
}

pub fn set(dir: &Path, id: &str) -> Result<Profile, String> {
    let profile = Profile::parse(id)
        .ok_or("Profilo sconosciuto: scegli eco, balanced, performance o maximum")?;
    std::fs::write(dir.join(FILE), json!({"profile": profile.id()}).to_string())
        .map_err(|e| e.to_string())?;
    Ok(profile)
}

pub fn describe(dir: &Path) -> Value {
    let active = current(dir);
    let profiles: Vec<Value> = Profile::ALL
        .into_iter()
        .map(|p| {
            let l = p.limits();
            json!({"id": p.id(), "label": p.label(), "active": p == active,
                "max_steps": l.max_steps, "max_subagents": l.max_subagents,
                "metrics_every_s": l.metrics_every.as_secs()})
        })
        .collect();
    json!({"active": active.id(), "profiles": profiles,
        "note": "Il profilo cambia solo quanto lavoro svolge JARVIS; permessi, conferme e audit restano identici."})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_keeps_the_historical_limits_and_profiles_scale_monotonically() {
        let b = Profile::Balanced.limits();
        assert_eq!((b.max_steps, b.max_subagents), (4, 2));
        assert_eq!(b.metrics_every, Duration::from_secs(300));
        let l: Vec<Limits> = Profile::ALL.iter().map(|p| p.limits()).collect();
        assert!(l.windows(2).all(|w| w[0].max_steps < w[1].max_steps
            && w[0].max_subagents < w[1].max_subagents
            && w[0].metrics_every > w[1].metrics_every));
    }

    #[test]
    fn the_choice_is_persisted_and_unknown_values_are_refused() {
        let d = std::env::current_dir().unwrap().join(format!(
            "target/jarvis-perf-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(current(&d), Profile::Balanced);
        assert_eq!(set(&d, "ECO").unwrap(), Profile::Eco);
        assert_eq!(current(&d), Profile::Eco);
        assert!(set(&d, "turbo").is_err());
        assert_eq!(current(&d), Profile::Eco);
        assert_eq!(describe(&d)["profiles"][0]["active"], true);
        std::fs::write(d.join(FILE), "non json").unwrap();
        assert_eq!(current(&d), Profile::Balanced);
        let _ = std::fs::remove_dir_all(&d);
    }
}
