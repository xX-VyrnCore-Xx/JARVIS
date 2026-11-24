//! Catalogo degli strumenti (`contracts/catalog/tools`), incorporato e validato all'avvio.

use std::collections::BTreeMap;

use jarvis_contracts::{validate, Contract};
use jsonschema::Validator;
use serde_json::Value;

macro_rules! tool_file {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/catalog/tools/",
            $name,
            ".json"
        ))
    };
}

const SOURCES: &[&str] = &[
    tool_file!("code.check_python"),
    tool_file!("code.run_python"),
    tool_file!("code.validate_json"),
    tool_file!("date.calc"),
    tool_file!("fs.delete"),
    tool_file!("fs.mkdir"),
    tool_file!("fs.move"),
    tool_file!("fs.search"),
    tool_file!("math.calc"),
    tool_file!("memory.update"),
    tool_file!("mission.cancel"),
    tool_file!("mission.list"),
    tool_file!("mission.start"),
    tool_file!("notes.add"),
    tool_file!("notes.delete"),
    tool_file!("notes.list"),
    tool_file!("notes.search"),
    tool_file!("sandbox.export"),
    tool_file!("sandbox.import"),
    tool_file!("sandbox.prepare"),
    tool_file!("sandbox.reset"),
    tool_file!("sandbox.run"),
    tool_file!("sandbox.status"),
    tool_file!("system.disks"),
    tool_file!("text.count"),
    tool_file!("todo.add"),
    tool_file!("todo.done"),
    tool_file!("todo.list"),
    tool_file!("todo.review"),
    tool_file!("units.convert"),
    tool_file!("weather.forecast"),
    tool_file!("agent.delegate"),
    tool_file!("fs.list"),
    tool_file!("fs.read"),
    tool_file!("fs.write"),
    tool_file!("memory.forget"),
    tool_file!("memory.list"),
    tool_file!("memory.remember"),
    tool_file!("memory.search"),
    tool_file!("n8n.create"),
    tool_file!("process.list"),
    tool_file!("pc.open"),
    tool_file!("security.status"),
    tool_file!("shell.run"),
    tool_file!("system.info"),
    tool_file!("time.now"),
    tool_file!("web.browse"),
    tool_file!("web.fetch"),
    tool_file!("web.form"),
    tool_file!("web.search"),
    tool_file!("workflow.create"),
    tool_file!("workflow.delete"),
    tool_file!("workflow.list"),
    tool_file!("workflow.run"),
    tool_file!("workflow.update"),
];

pub struct Tool {
    pub descriptor: Value,
    pub input: Validator,
}

impl Tool {
    pub fn name(&self) -> &str {
        self.descriptor["name"].as_str().unwrap_or_default()
    }
    pub fn version(&self) -> &str {
        self.descriptor["version"].as_str().unwrap_or_default()
    }
    pub fn risk(&self) -> &str {
        self.descriptor["risk"].as_str().unwrap_or("high")
    }
    pub fn needs_confirmation(&self) -> bool {
        self.descriptor["confirmation"] == "required"
    }
}

pub struct Catalog {
    tools: BTreeMap<String, Tool>,
}

impl Catalog {
    pub fn load() -> Result<Self, String> {
        let mut tools = BTreeMap::new();
        for src in SOURCES {
            let descriptor: Value =
                serde_json::from_str(src).map_err(|e| format!("catalogo non JSON: {e}"))?;
            let report = validate(Contract::ToolDescriptor, &descriptor);
            if !report.is_valid() {
                return Err(format!("descrittore non conforme: {report:?}"));
            }
            let input = jsonschema::validator_for(&descriptor["input_schema"])
                .map_err(|e| format!("input_schema non valido: {e}"))?;
            let name = descriptor["name"].as_str().unwrap_or_default().to_string();
            tools.insert(name, Tool { descriptor, input });
        }
        Ok(Self { tools })
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tool> {
        self.tools.values()
    }

    /// Registra uno strumento dinamico (connettori RF-14). Solo il prefisso `connector.` è ammesso,
    /// così un manifest non può sostituire uno strumento statico.
    pub fn insert_dynamic(&mut self, descriptor: Value) -> Result<(), String> {
        let name = descriptor["name"].as_str().unwrap_or_default().to_string();
        if !name.starts_with("connector.") && !name.starts_with("node.") {
            return Err(format!(
                "strumento dinamico fuori dallo spazio dei connettori: {name}"
            ));
        }
        let report = validate(Contract::ToolDescriptor, &descriptor);
        if !report.is_valid() {
            return Err(format!(
                "descrittore non conforme: {}",
                report.schema.join("; ")
            ));
        }
        let input = jsonschema::validator_for(&descriptor["input_schema"])
            .map_err(|e| format!("input_schema non valido: {e}"))?;
        self.tools.insert(name, Tool { descriptor, input });
        Ok(())
    }

    /// Rimuove gli strumenti di un connettore; restituisce quanti ne ha tolti.
    pub fn remove_connector(&mut self, id: &str) -> usize {
        self.remove_prefix(&format!("connector.{id}."))
    }

    /// Rimuove gli strumenti dinamici con il prefisso indicato (connettori o nodi).
    pub fn remove_prefix(&mut self, prefix: &str) -> usize {
        let before = self.tools.len();
        self.tools.retain(|name, _| !name.starts_with(prefix));
        before - self.tools.len()
    }
}
