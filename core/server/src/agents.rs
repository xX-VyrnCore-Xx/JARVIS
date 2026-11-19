//! Specializzazioni degli agenti (RF-03, ADR-0038). Il Core sceglie la specializzazione dalla richiesta e
//! concede solo gli strumenti del suo ambito; una richiesta che tocca più ambiti, o nessuno, va all'agente
//! generalista con tutti gli strumenti. Uno strumento fuori dall'ambito concesso viene negato dal Core.

/// Specializzazione: nome del contratto di delega, ruolo per il prompt, parole chiave e strumenti concessi.
pub struct Agent {
    pub name: &'static str,
    pub role: &'static str,
    keywords: &'static [&'static str],
    tools: &'static [&'static str],
}

/// Strumenti concessi a ogni specializzazione oltre al proprio ambito: ora e ricerca nella memoria.
const COMMON: [&str; 2] = ["time.now", "memory.search"];

pub const SPECIALISTS: [Agent; 5] = [
    Agent {
        name: "research",
        role: "Ricerca: trova informazioni sul Web, confronta le fonti e cita sempre gli URL.",
        keywords: &["cerca sul web", "cerca online", "cerca in rete", "notizi", "wikipedia", "http://", "https://", "fonti", "fai una ricerca", "naviga"],
        tools: &["web.search", "web.fetch", "web.browse", "web.form"],
    },
    Agent {
        name: "system",
        role: "Sistema: controlla processi, risorse e terminale del computer; spiega l'effetto di ogni comando prima di proporlo.",
        keywords: &["processi", "processo", "terminale", "cpu", " ram", "task manager", "rallent", "che sistema", "info sistema", "informazioni di sistema", "informazioni sul sistema"],
        tools: &["system.info", "system.disks", "process.list", "shell.run", "fs.list", "fs.read", "fs.search"],
    },
    Agent {
        name: "automation",
        role: "Automazioni: crea, modifica ed esegue automazioni pianificate; descrivi chiaramente quando e cosa eseguiranno.",
        keywords: &["automazion", "pianific", "ogni giorno", "ogni ora", "ogni settimana", "tra 1", "tra 2", "tra 3", "tra 5", "tra 10", "tra un", "tra mezz"],
        tools: &["workflow.create", "workflow.list", "workflow.run", "workflow.update", "workflow.delete", "system.info", "fs.write", "todo.add", "todo.list", "todo.done"],
    },
    Agent {
        name: "memory",
        role: "Memoria: salva, cerca, corregge e dimentica ciò che il proprietario ti chiede; frasi brevi e autonome.",
        keywords: &["ricorda che", "ricordati", "cosa ricordi", "dimentica", "nella memoria", "preferisco"],
        tools: &["memory.remember", "memory.list", "memory.forget", "memory.update", "notes.add", "notes.list", "notes.search", "notes.delete"],
    },
    Agent {
        name: "coding",
        role: "Codice: scrivi, leggi ed esegui script e programmi nella workspace; proponi comandi verificabili.",
        keywords: &["codice", "script", "programma in", "python", "javascript", "compila", "git ", "npm "],
        tools: &["fs.list", "fs.read", "fs.write", "fs.search", "fs.mkdir", "fs.move", "fs.delete", "shell.run", "math.calc"],
    },
];

pub const GENERAL: &str = "general_assistant";

fn fold(text: &str) -> String {
    text.to_lowercase()
        .replace(['à'], "a")
        .replace(['è', 'é'], "e")
        .replace(['ì'], "i")
        .replace(['ò'], "o")
        .replace(['ù'], "u")
}

/// Specializzazione per la richiesta: esattamente un ambito riconosciuto, altrimenti `None` (generalista).
pub fn route(message: &str) -> Option<&'static Agent> {
    let text = format!(" {} ", fold(message));
    let mut hits = SPECIALISTS
        .iter()
        .filter(|a| a.keywords.iter().any(|k| text.contains(k)));
    match (hits.next(), hits.next()) {
        (Some(a), None) => Some(a),
        _ => None,
    }
}

/// Lo strumento rientra nell'ambito concesso alla specializzazione.
pub fn allows(agent: &Agent, tool: &str) -> bool {
    COMMON.contains(&tool) || agent.tools.contains(&tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(m: &str) -> &'static str {
        route(m).map_or(GENERAL, |a| a.name)
    }

    #[test]
    fn requests_go_to_one_specialist_or_to_the_generalist() {
        assert_eq!(name("cerca sul web le notizie su Rust"), "research");
        assert_eq!(name("quali processi usano la CPU?"), "system");
        assert_eq!(name("terminale: ls -la"), "system");
        assert_eq!(name("tra 2 minuti che ore sono"), "automation");
        assert_eq!(name("ricorda che il cane si chiama Fido"), "memory");
        assert_eq!(
            name("scrivi uno script python che conta le righe"),
            "coding"
        );
        // Nessun ambito o più ambiti: generalista.
        assert_eq!(name("ciao, come stai?"), GENERAL);
        assert_eq!(name("ogni ora info sistema"), GENERAL);
        assert_eq!(name("leggi note.txt"), GENERAL);
    }

    #[test]
    fn specialists_only_get_their_tools() {
        let research = route("cerca sul web Rust").unwrap();
        assert!(allows(research, "web.fetch") && allows(research, "time.now"));
        assert!(!allows(research, "shell.run") && !allows(research, "fs.write"));
        let memory = route("cosa ricordi?").unwrap();
        assert!(!allows(memory, "web.search"));
    }
}
