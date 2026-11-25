//! Registro universale degli strumenti (ADR-0151): metadati normalizzati, salute e cronologia di esecuzione.
//!
//! I descrittori del catalogo (`contracts/catalog/tools`) restano la fonte autorevole di nome, schema, rischio,
//! timeout, retry, permessi e conferma. Questo modulo ne ricava i campi del registro che i descrittori non
//! dichiarano: categoria, piattaforme, disponibilità offline, reversibilità, dipendenze e requisiti. Un descrittore
//! può sovrascrivere ciascun campo scrivendolo esplicitamente.

use serde_json::{json, Value};

/// Categoria dalla famiglia dello strumento.
pub fn category(name: &str) -> &'static str {
    let family = name.split('.').next().unwrap_or(name);
    match family {
        "fs" => "files",
        "git" | "deps" | "log" => "development",
        "table" => "analysis",
        "shell" | "process" | "system" | "pc" | "app" | "screen" => "system",
        "code" | "sandbox" => "development",
        "web" | "weather" => "web",
        "notes" | "todo" | "time" | "date" => "productivity",
        "memory" => "memory",
        "mission" | "workflow" | "n8n" | "agent" => "automation",
        "security" | "it" => "security",
        "biz" | "math" | "units" | "data" => "analysis",
        "text" => "text",
        _ => "general",
    }
}

/// Sistemi su cui lo strumento è disponibile: il Core gira su Windows, macOS e Linux; il controllo del desktop
/// e dello schermo esiste solo dove c'è un'interfaccia grafica supportata.
pub fn platforms(name: &str) -> Vec<&'static str> {
    if name.starts_with("app.") || name == "pc.open" || name.starts_with("screen.") {
        vec!["windows", "macos"]
    } else {
        vec!["windows", "macos", "linux"]
    }
}

/// Funziona senza rete.
pub fn offline(name: &str) -> bool {
    !(name.starts_with("web.")
        || name.starts_with("weather.")
        || name.starts_with("connector.")
        || name == "n8n.create")
}

/// Reversibilità: `yes` (esiste un'operazione inversa), `no` (irreversibile) o `n/a` (nessun effetto).
pub fn reversible(name: &str, effects: &str) -> &'static str {
    if matches!(effects, "none" | "read") {
        return "n/a";
    }
    match name {
        "fs.write" | "fs.mkdir" | "fs.move" | "notes.add" | "todo.add" | "todo.done"
        | "memory.remember" | "memory.update" | "mission.start" | "mission.cancel"
        | "workflow.create" | "workflow.update" | "sandbox.import" | "sandbox.reset" => "yes",
        _ => "no",
    }
}

/// Componenti esterni necessari.
pub fn dependencies(name: &str) -> Vec<&'static str> {
    match name {
        n if n.starts_with("sandbox.") || n == "code.run_python" || n == "code.check_python" => {
            vec!["docker"]
        }
        "web.browse" | "web.form" => vec!["browser"],
        "n8n.create" => vec!["n8n"],
        n if n.starts_with("screen.") => vec!["screen-capture"],
        n if n.starts_with("app.") || n.starts_with("clipboard.") => vec!["desktop-session"],
        _ => vec![],
    }
}

/// Requisiti hardware approssimativi.
pub fn hardware(name: &str) -> Value {
    match name {
        "sandbox.run" => json!({"memory_mb": 512, "cpus": 1}),
        "code.run_python" | "code.check_python" => json!({"memory_mb": 128, "cpus": 1}),
        "screen.read" => json!({"display": true}),
        _ => json!({}),
    }
}

/// Voce del registro per un descrittore: campi autorevoli del catalogo più quelli ricavati.
pub fn entry(descriptor: &Value) -> Value {
    let name = descriptor["name"].as_str().unwrap_or_default();
    let effects = descriptor["effects"].as_str().unwrap_or("write_local");
    let pick = |key: &str, derived: Value| {
        descriptor
            .get(key)
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(derived)
    };
    json!({
        "name": name,
        "version": descriptor["version"],
        "summary": descriptor["summary"],
        "category": pick("category", json!(category(name))),
        "risk": descriptor["risk"],
        "effects": effects,
        "confirmation": descriptor["confirmation"],
        "permissions": descriptor["permissions"],
        "timeout_ms": descriptor["timeout_ms"],
        "retry": descriptor["retry"],
        "errors": descriptor["errors"],
        "platforms": pick("platforms", json!(platforms(name))),
        "offline": pick("offline", json!(offline(name))),
        "reversible": pick("reversible", json!(reversible(name, effects))),
        "dependencies": pick("dependencies", json!(dependencies(name))),
        "hardware": pick("hardware", hardware(name)),
        "input_schema": descriptor["input_schema"],
        "output_schema": descriptor["output_schema"],
    })
}

/// Strumenti che non toccano il database né lo stato del Core: calcoli, lettura di file nella workspace e rete.
/// Solo questi possono girare in parallelo dentro un piano (ogni esecuzione usa un archivio privato e vuoto).
pub fn parallel_safe(name: &str) -> bool {
    name.starts_with("biz.")
        || matches!(
            name,
            "time.now"
                | "time.wait"
                | "math.calc"
                | "units.convert"
                | "date.calc"
                | "text.count"
                | "text.correct"
                | "text.diff"
                | "text.extract"
                | "data.stats"
                | "code.validate_json"
                | "security.cvss"
                | "it.subnet"
                | "system.disks"
                | "process.list"
                | "fs.read"
                | "fs.list"
                | "fs.search"
                | "git.status"
                | "git.log"
                | "git.diff"
                | "git.branch"
                | "log.analyze"
                | "deps.list"
                | "table.summary"
                | "pdf.extract"
                | "sheet.read"
                | "web.search"
                | "web.fetch"
                | "weather.forecast"
        )
}

/// Stato di salute da conteggi: poche esecuzioni non bastano per giudicare.
pub fn health_state(runs: u64, failures: u64) -> &'static str {
    if runs < 3 {
        "unknown"
    } else {
        let rate = failures as f64 / runs as f64;
        if rate < 0.10 {
            "healthy"
        } else if rate < 0.40 {
            "degraded"
        } else {
            "failing"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_fields_follow_the_tool_family() {
        assert_eq!(category("fs.write"), "files");
        assert_eq!(category("biz.npv"), "analysis");
        assert_eq!(category("sandbox.run"), "development");
        assert_eq!(platforms("app.open"), ["windows", "macos"]);
        assert!(platforms("math.calc").contains(&"linux"));
        assert!(!offline("web.search") && offline("math.calc"));
        assert_eq!(reversible("fs.write", "write_local"), "yes");
        assert_eq!(reversible("fs.delete", "write_local"), "no");
        assert_eq!(reversible("time.now", "none"), "n/a");
        assert_eq!(dependencies("sandbox.run"), ["docker"]);
        assert!(dependencies("math.calc").is_empty());
    }

    #[test]
    fn a_descriptor_can_override_a_derived_field() {
        let descriptor = json!({"name": "fs.write", "version": "1.0.0", "effects": "write_local",
            "category": "storage", "offline": false});
        let out = entry(&descriptor);
        assert_eq!(out["category"], "storage");
        assert_eq!(out["offline"], false);
        assert_eq!(out["reversible"], "yes");
    }

    #[test]
    fn only_stateless_tools_run_in_parallel() {
        assert!(
            parallel_safe("math.calc") && parallel_safe("biz.npv") && parallel_safe("web.fetch")
        );
        for sequential in [
            "fs.write",
            "notes.add",
            "memory.search",
            "sandbox.run",
            "workflow.run",
            "shell.run",
        ] {
            assert!(!parallel_safe(sequential), "{sequential}");
        }
    }

    #[test]
    fn health_needs_enough_runs_and_follows_the_failure_rate() {
        assert_eq!(health_state(2, 2), "unknown");
        assert_eq!(health_state(20, 0), "healthy");
        assert_eq!(health_state(10, 2), "degraded");
        assert_eq!(health_state(10, 6), "failing");
    }
}
