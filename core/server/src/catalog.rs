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

const SOURCES: [&str; 15] = [
    tool_file!("fs.list"),
    tool_file!("fs.read"),
    tool_file!("fs.write"),
    tool_file!("memory.forget"),
    tool_file!("memory.list"),
    tool_file!("memory.remember"),
    tool_file!("system.info"),
    tool_file!("time.now"),
    tool_file!("web.fetch"),
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
        if !name.starts_with("connector.") {
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
        let prefix = format!("connector.{id}.");
        let before = self.tools.len();
        self.tools.retain(|name, _| !name.starts_with(&prefix));
        before - self.tools.len()
    }
}
