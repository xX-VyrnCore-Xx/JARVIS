//! Permessi del proprietario (ADR-0116): profili e regole per strumento che **restringono** la policy del catalogo.
//!
//! Il proprietario può solo stringere: un profilo o una regola possono rifiutare uno strumento o chiedere la
//! conferma per ogni uso, mai togliere una conferma richiesta dal catalogo. Le regole stanno in
//! `<dati>/permissions.json`; si modificano dal solo computer del Core (`PATCH /api/permissions`), mai da chat,
//! così un testo ostile non può allargare né ridurre i permessi.

use std::path::Path;

use serde_json::{json, Value};

const FILE: &str = "permissions.json";
const MAX_RULES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Comportamento del catalogo, senza restrizioni aggiuntive.
    Standard,
    /// Ogni strumento che modifica qualcosa chiede la conferma del proprietario.
    Prudent,
    /// Solo lettura: ogni strumento che modifica o esegue è negato.
    ReadOnly,
}

impl Profile {
    pub fn code(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Prudent => "prudent",
            Self::ReadOnly => "read_only",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "standard" => Some(Self::Standard),
            "prudent" => Some(Self::Prudent),
            "read_only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Chiede la conferma a ogni uso.
    Ask,
    /// Rifiuta sempre.
    Deny,
}

#[derive(Clone, Debug)]
pub struct Rules {
    profile: Profile,
    rules: Vec<(String, Mode)>,
}

/// Nome di uno strumento o prefisso con asterisco finale (`connector.*`).
pub fn valid_pattern(pattern: &str) -> bool {
    let body = pattern.strip_suffix(".*").unwrap_or(pattern);
    !pattern.is_empty()
        && pattern.len() <= 64
        && !body.is_empty()
        && body
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'_')
}

fn matches(pattern: &str, tool: &str) -> bool {
    match pattern.strip_suffix(".*") {
        Some(prefix) => tool.starts_with(&format!("{prefix}.")),
        None => pattern == tool,
    }
}

impl Rules {
    pub fn standard() -> Self {
        Self {
            profile: Profile::Standard,
            rules: Vec::new(),
        }
    }

    /// Legge le regole. File assente: standard. File presente ma illeggibile: profilo prudente (fallire chiuso).
    pub fn load(dir: &Path) -> Self {
        let Ok(bytes) = std::fs::read(dir.join(FILE)) else {
            return Self::standard();
        };
        let Some(doc) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
        else {
            return Self {
                profile: Profile::Prudent,
                rules: Vec::new(),
            };
        };
        let profile = doc["profile"]
            .as_str()
            .and_then(Profile::parse)
            .unwrap_or(Profile::Prudent);
        let mut rules: Vec<(String, Mode)> = doc["rules"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(pattern, _)| valid_pattern(pattern))
            .filter_map(|(pattern, mode)| match mode.as_str() {
                Some("ask") => Some((pattern.clone(), Mode::Ask)),
                Some("deny") => Some((pattern.clone(), Mode::Deny)),
                _ => None,
            })
            .take(MAX_RULES)
            .collect();
        rules.sort_by(|a, b| a.0.cmp(&b.0));
        Self { profile, rules }
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// Restrizione applicabile a uno strumento; la più severa vince. `None`: nessuna restrizione.
    pub fn verdict(&self, tool: &str, effects: &str) -> Option<Mode> {
        let mut found = self
            .rules
            .iter()
            .filter(|(pattern, _)| matches(pattern, tool))
            .map(|(_, mode)| *mode)
            .fold(None, |acc: Option<Mode>, mode| match (acc, mode) {
                (Some(Mode::Deny), _) | (_, Mode::Deny) => Some(Mode::Deny),
                _ => Some(Mode::Ask),
            });
        let changes_something = !matches!(effects, "read" | "none");
        if changes_something {
            match self.profile {
                Profile::ReadOnly => found = Some(Mode::Deny),
                Profile::Prudent if found.is_none() => found = Some(Mode::Ask),
                _ => {}
            }
        }
        found
    }

    pub fn to_json(&self) -> Value {
        let rules: serde_json::Map<String, Value> = self
            .rules
            .iter()
            .map(|(pattern, mode)| {
                (
                    pattern.clone(),
                    json!(match mode {
                        Mode::Ask => "ask",
                        Mode::Deny => "deny",
                    }),
                )
            })
            .collect();
        json!({"profile": self.profile.code(), "rules": rules,
               "profiles": ["standard", "prudent", "read_only"]})
    }
}

/// Applica una modifica: `profile` e/o `rules` (`null` elimina una regola). Valida tutto prima di scrivere.
pub fn update(dir: &Path, patch: &Value) -> Result<Value, String> {
    let current = Rules::load(dir);
    let mut profile = current.profile;
    if let Some(value) = patch.get("profile") {
        profile = value
            .as_str()
            .and_then(Profile::parse)
            .ok_or("profilo sconosciuto (standard, prudent, read_only)")?;
    }
    let mut rules: std::collections::BTreeMap<String, &'static str> = current
        .rules
        .iter()
        .map(|(p, m)| {
            (
                p.clone(),
                match m {
                    Mode::Ask => "ask",
                    Mode::Deny => "deny",
                },
            )
        })
        .collect();
    if let Some(changes) = patch.get("rules") {
        let changes = changes
            .as_object()
            .ok_or("«rules» deve essere un oggetto")?;
        for (pattern, mode) in changes {
            if !valid_pattern(pattern) {
                return Err(format!("nome di strumento non valido: {pattern}"));
            }
            match mode.as_str() {
                Some("ask") => rules.insert(pattern.clone(), "ask"),
                Some("deny") => rules.insert(pattern.clone(), "deny"),
                None if mode.is_null() => rules.remove(pattern),
                _ => {
                    return Err(format!(
                        "modalità non valida per {pattern} (ask, deny o null)"
                    ))
                }
            };
        }
    }
    if rules.len() > MAX_RULES {
        return Err(format!("al massimo {MAX_RULES} regole"));
    }
    let doc = json!({"profile": profile.code(), "rules": rules});
    std::fs::write(dir.join(FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(Rules::load(dir).to_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jarvis-perm-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_default_adds_no_restriction() {
        let rules = Rules::load(&dir());
        assert_eq!(rules.profile(), Profile::Standard);
        assert_eq!(rules.verdict("fs.write", "write_local"), None);
    }

    #[test]
    fn profiles_tighten_only_tools_that_change_something() {
        let d = dir();
        update(&d, &json!({"profile": "prudent"})).unwrap();
        let rules = Rules::load(&d);
        assert_eq!(rules.verdict("notes.add", "write_local"), Some(Mode::Ask));
        assert_eq!(rules.verdict("time.now", "none"), None);
        assert_eq!(rules.verdict("fs.read", "read"), None);
        update(&d, &json!({"profile": "read_only"})).unwrap();
        let rules = Rules::load(&d);
        assert_eq!(
            rules.verdict("sandbox.run", "write_local"),
            Some(Mode::Deny)
        );
        assert_eq!(rules.verdict("fs.list", "read"), None);
    }

    #[test]
    fn rules_match_names_and_prefixes_and_deny_wins() {
        let d = dir();
        update(&d, &json!({"rules": {"connector.*": "ask", "connector.gmail.send": "deny", "web.fetch": "ask"}})).unwrap();
        let rules = Rules::load(&d);
        assert_eq!(
            rules.verdict("connector.calendar.list", "read"),
            Some(Mode::Ask)
        );
        assert_eq!(
            rules.verdict("connector.gmail.send", "write_external"),
            Some(Mode::Deny)
        );
        assert_eq!(rules.verdict("web.fetch", "read"), Some(Mode::Ask));
        assert_eq!(rules.verdict("web.fetcher", "read"), None);
        // Eliminare una regola.
        update(&d, &json!({"rules": {"web.fetch": null}})).unwrap();
        assert_eq!(Rules::load(&d).verdict("web.fetch", "read"), None);
    }

    #[test]
    fn invalid_input_is_rejected_and_a_corrupt_file_fails_closed() {
        let d = dir();
        for bad in [
            json!({"profile": "libero"}),
            json!({"rules": {"../etc": "deny"}}),
            json!({"rules": {"*": "deny"}}),
            json!({"rules": {"fs.write": "allow"}}),
            json!({"rules": []}),
        ] {
            assert!(update(&d, &bad).is_err(), "{bad}");
        }
        std::fs::write(d.join(FILE), b"{ non json").unwrap();
        let rules = Rules::load(&d);
        assert_eq!(rules.profile(), Profile::Prudent);
        assert_eq!(rules.verdict("fs.write", "write_local"), Some(Mode::Ask));
    }
}
