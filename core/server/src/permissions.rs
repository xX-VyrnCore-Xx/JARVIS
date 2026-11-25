//! Permessi del proprietario (ADR-0116): profili e regole per strumento che **restringono** la policy del catalogo.
//!
//! Il proprietario può solo stringere: un profilo o una regola possono rifiutare uno strumento o chiedere la
//! conferma per ogni uso, mai togliere una conferma richiesta dal catalogo. Le regole stanno in
//! `<dati>/permissions.json`; si modificano dal solo computer del Core (`PATCH /api/permissions`), mai da chat,
//! così un testo ostile non può allargare né ridurre i permessi.

use std::path::Path;

use serde_json::{json, Value};

const FILE: &str = "permissions.json";
const EMERGENCY_FILE: &str = "emergency.json";
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
    /// Livelli di autonomia (ADR-0174): una restrizione in più, mai un permesso in più.
    autonomy: Autonomy,
    /// Arresto di emergenza attivo (ADR-0151): tutto ciò che modifica o esegue è negato.
    emergency: bool,
}

/// Vero se l'arresto di emergenza è attivo.
pub fn emergency_active(dir: &Path) -> bool {
    std::fs::read(dir.join(EMERGENCY_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|doc| doc["active"] == true)
}

/// Attiva o disattiva l'arresto di emergenza e ne restituisce lo stato.
pub fn set_emergency(dir: &Path, active: bool) -> Result<Value, String> {
    let doc = json!({"active": active, "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)});
    std::fs::write(dir.join(EMERGENCY_FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(doc)
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
            emergency: false,
            rules: Vec::new(),
            autonomy: Autonomy::standard(),
        }
    }

    /// Legge le regole. File assente: standard. File presente ma illeggibile: profilo prudente (fallire chiuso).
    pub fn load(dir: &Path) -> Self {
        let mut rules = Self::load_saved(dir);
        rules.autonomy = Autonomy::load(dir);
        if emergency_active(dir) {
            rules.emergency = true;
            rules.profile = Profile::ReadOnly;
        }
        rules
    }

    pub fn emergency(&self) -> bool {
        self.emergency
    }

    fn load_saved(dir: &Path) -> Self {
        let Ok(bytes) = std::fs::read(dir.join(FILE)) else {
            return Self::standard();
        };
        let Some(doc) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
        else {
            return Self {
                profile: Profile::Prudent,
                emergency: false,
                rules: Vec::new(),
                autonomy: Autonomy::standard(),
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
        Self {
            profile,
            rules,
            emergency: false,
            autonomy: Autonomy::standard(),
        }
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// Restrizione applicabile a uno strumento; la più severa vince. `None`: nessuna restrizione.
    pub fn verdict(&self, tool: &str, effects: &str) -> Option<Mode> {
        self.verdict_for(tool, effects, "low", "")
    }

    /// Come `verdict`, con il rischio dello strumento e l'agente che lo propone (livelli di autonomia, ADR-0174).
    pub fn verdict_for(&self, tool: &str, effects: &str, risk: &str, agent: &str) -> Option<Mode> {
        let base = self.profile_verdict(tool, effects);
        let level = self.autonomy.effective(tool, risk, agent);
        let by_level = level_restriction(level, effects);
        match (base, by_level) {
            (Some(Mode::Deny), _) | (_, Some(Mode::Deny)) => Some(Mode::Deny),
            (Some(Mode::Ask), _) | (_, Some(Mode::Ask)) => Some(Mode::Ask),
            _ => None,
        }
    }

    fn profile_verdict(&self, tool: &str, effects: &str) -> Option<Mode> {
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
               "profiles": ["standard", "prudent", "read_only"], "autonomy": self.autonomy.to_json()})
    }
}

const AUTONOMY_FILE: &str = "autonomy.json";
/// Livello predefinito: il comportamento del catalogo con le attività in background consentite (invariato).
pub const DEFAULT_LEVEL: u8 = 4;
pub const MAX_LEVEL: u8 = 5;

/// Livelli di autonomia (ADR-0174), configurabili per agente, strumento e categoria di rischio.
/// 0 conversazione (nessuno strumento), 1 consultivo (solo lettura), 2 esecuzione assistita (ogni modifica con
/// conferma), 3 autonomia delegata (comportamento del catalogo; niente attività in background), 4 proattiva
/// (come 3, con missioni, iniziative e osservazioni di file) e 5 avanzata supervisionata: **riservata**, oggi
/// equivale a 4. Nessun livello toglie conferme richieste dal catalogo, dalla policy o dai profili: il livello
/// effettivo è il **minimo** fra quelli applicabili, quindi un livello più alto non allarga nulla.
#[derive(Clone, Debug)]
pub struct Autonomy {
    default: u8,
    agents: std::collections::BTreeMap<String, u8>,
    tools: Vec<(String, u8)>,
    risks: std::collections::BTreeMap<String, u8>,
}

fn level_restriction(level: u8, effects: &str) -> Option<Mode> {
    let changes_something = !matches!(effects, "read" | "none");
    match level {
        0 => Some(Mode::Deny),
        1 if changes_something => Some(Mode::Deny),
        2 if changes_something => Some(Mode::Ask),
        _ => None,
    }
}

impl Autonomy {
    pub fn standard() -> Self {
        Self {
            default: DEFAULT_LEVEL,
            agents: Default::default(),
            tools: Vec::new(),
            risks: Default::default(),
        }
    }

    /// File assente: livelli predefiniti. File illeggibile: livello 2 (esecuzione assistita), cioè fallire chiuso.
    pub fn load(dir: &Path) -> Self {
        let Ok(bytes) = std::fs::read(dir.join(AUTONOMY_FILE)) else {
            return Self::standard();
        };
        let Some(doc) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
        else {
            return Self {
                default: 2,
                ..Self::standard()
            };
        };
        let level = |v: &Value| {
            v.as_u64()
                .filter(|n| *n <= MAX_LEVEL as u64)
                .map(|n| n as u8)
        };
        let map = |key: &str| -> std::collections::BTreeMap<String, u8> {
            doc[key]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| level(v).map(|l| (k.clone(), l)))
                .take(MAX_RULES)
                .collect()
        };
        Self {
            default: level(&doc["default"]).unwrap_or(2),
            agents: map("agents"),
            tools: map("tools")
                .into_iter()
                .filter(|(pattern, _)| valid_pattern(pattern))
                .collect(),
            risks: map("risks"),
        }
    }

    /// Livello effettivo: il minimo di quelli che si applicano allo strumento, al suo rischio e all'agente.
    pub fn effective(&self, tool: &str, risk: &str, agent: &str) -> u8 {
        let mut level = self.default;
        if let Some(l) = self.agents.get(agent) {
            level = level.min(*l);
        }
        if let Some(l) = self.risks.get(risk) {
            level = level.min(*l);
        }
        for (pattern, l) in &self.tools {
            if matches(pattern, tool) {
                level = level.min(*l);
            }
        }
        level
    }

    /// Le attività che partono da sole (missioni, iniziative, osservazioni di file) richiedono almeno il livello 4.
    pub fn background_allowed(&self) -> bool {
        self.default >= 4
    }

    pub fn to_json(&self) -> Value {
        json!({
            "default": self.default,
            "agents": self.agents, "tools": self.tools.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
            "risks": self.risks,
            "levels": ["conversazione", "consultivo", "esecuzione assistita", "autonomia delegata",
                       "proattiva", "avanzata supervisionata (riservata: oggi come 4)"],
            "background_allowed": self.background_allowed(),
        })
    }
}

/// Vero se le attività in background (missioni, iniziative proattive, osservazioni) sono consentite.
pub fn background_allowed(dir: &Path) -> bool {
    Autonomy::load(dir).background_allowed()
}

/// Modifica i livelli: `default`, `agents`, `tools`, `risks` (`null` elimina una voce). Valida tutto prima di scrivere.
pub fn update_autonomy(dir: &Path, patch: &Value, agents: &[&str]) -> Result<Value, String> {
    let current = Autonomy::load(dir);
    let mut default = current.default;
    let mut agents_map = current.agents;
    let mut tools: std::collections::BTreeMap<String, u8> = current.tools.into_iter().collect();
    let mut risks = current.risks;
    let level = |v: &Value| -> Result<u8, String> {
        v.as_u64()
            .filter(|n| *n <= MAX_LEVEL as u64)
            .map(|n| n as u8)
            .ok_or_else(|| format!("il livello deve essere un numero da 0 a {MAX_LEVEL}"))
    };
    if let Some(v) = patch.get("default") {
        default = level(v)?;
    }
    let edit = |map: &mut std::collections::BTreeMap<String, u8>,
                key: &str,
                allowed: &dyn Fn(&str) -> bool|
     -> Result<(), String> {
        let Some(changes) = patch.get(key) else {
            return Ok(());
        };
        let changes = changes
            .as_object()
            .ok_or_else(|| format!("«{key}» deve essere un oggetto"))?;
        for (name, value) in changes {
            if !allowed(name) {
                return Err(format!("{key}: nome non valido ({name})"));
            }
            if value.is_null() {
                map.remove(name);
            } else {
                map.insert(name.clone(), level(value)?);
            }
        }
        Ok(())
    };
    edit(&mut agents_map, "agents", &|n| agents.contains(&n))?;
    edit(&mut tools, "tools", &|n| valid_pattern(n))?;
    edit(&mut risks, "risks", &|n| {
        matches!(n, "low" | "medium" | "high" | "critical")
    })?;
    if agents_map.len() + tools.len() + risks.len() > MAX_RULES {
        return Err(format!("al massimo {MAX_RULES} voci"));
    }
    let doc = json!({"default": default, "agents": agents_map, "tools": tools, "risks": risks});
    std::fs::write(dir.join(AUTONOMY_FILE), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(Autonomy::load(dir).to_json())
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

    #[test]
    fn autonomy_levels_only_restrict_and_the_lowest_applicable_one_wins() {
        let d = dir();
        // Predefinito (4): nessuna restrizione in più.
        assert_eq!(
            Rules::load(&d).verdict_for("fs.write", "write_local", "high", "coding"),
            None
        );
        let names = ["coding", "system", "general_assistant"];
        update_autonomy(
            &d,
            &json!({"default": 3, "agents": {"coding": 2}, "tools": {"fs.*": 1}, "risks": {"critical": 0}}),
            &names,
        )
        .unwrap();
        let rules = Rules::load(&d);
        // 3: catalogo invariato per uno strumento qualsiasi.
        assert_eq!(
            rules.verdict_for("notes.add", "write_local", "low", "system"),
            None
        );
        // L'agente di codice è a 2: ogni modifica chiede conferma, la lettura no.
        assert_eq!(
            rules.verdict_for("notes.add", "write_local", "low", "coding"),
            Some(Mode::Ask)
        );
        assert_eq!(rules.verdict_for("fs.read", "read", "low", "coding"), None);
        // I file sono consultivi (1): una scrittura è negata, anche per il generalista.
        assert_eq!(
            rules.verdict_for("fs.write", "write_local", "high", "general_assistant"),
            Some(Mode::Deny)
        );
        // Livello 0 per il rischio critico: nessuno strumento critico, nemmeno in lettura.
        assert_eq!(
            rules.verdict_for("shell.run", "read", "critical", "system"),
            Some(Mode::Deny)
        );
        assert!(!Autonomy::load(&d).background_allowed());
    }

    #[test]
    fn a_higher_autonomy_level_never_lifts_a_profile_or_rule_restriction() {
        let d = dir();
        update(
            &d,
            &json!({"profile": "read_only", "rules": {"web.fetch": "ask"}}),
        )
        .unwrap();
        update_autonomy(&d, &json!({"default": 5}), &[]).unwrap();
        let rules = Rules::load(&d);
        assert_eq!(
            rules.verdict_for("fs.write", "write_local", "high", "coding"),
            Some(Mode::Deny)
        );
        assert_eq!(
            rules.verdict_for("web.fetch", "read", "medium", "research"),
            Some(Mode::Ask)
        );
    }

    #[test]
    fn autonomy_input_is_validated_and_a_corrupt_file_fails_closed() {
        let d = dir();
        let names = ["coding"];
        for bad in [
            json!({"default": 6}),
            json!({"default": "alto"}),
            json!({"agents": {"sconosciuto": 1}}),
            json!({"tools": {"../x": 1}}),
            json!({"risks": {"enorme": 1}}),
            json!({"agents": []}),
        ] {
            assert!(update_autonomy(&d, &bad, &names).is_err(), "{bad}");
        }
        std::fs::write(d.join(AUTONOMY_FILE), b"{ non json").unwrap();
        let rules = Rules::load(&d);
        assert_eq!(
            rules.verdict_for("notes.add", "write_local", "low", "system"),
            Some(Mode::Ask)
        );
        assert!(!Autonomy::load(&d).background_allowed());
    }
}
