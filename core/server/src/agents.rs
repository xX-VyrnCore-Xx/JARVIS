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

pub const SPECIALISTS: [Agent; 7] = [
    Agent {
        name: "monitoring",
        role: "Sicurezza difensiva: controlla postura, audit e configurazioni locali. Riferisci le evidenze raccolte, la gravità e le verifiche mancanti. Per la bonifica indica priorità e verifica del ripristino.",
        keywords: &["cvss", "vulnerabilit", "ransomware", "phishing", "incidente di sicurezza", "violazione dei dati", "data breach", "integrita del file", "cybersecurity", "postura di sicurezza", "centro sicurezza", "audit di sicurezza", "hardening", "controllo di sicurezza", "bonifica", "risposta a incidenti", "incident response", "audit dei permessi", "postura it"],
        tools: &["security.status", "security.cvss", "security.hash", "system.info", "system.disks", "process.list", "fs.read", "fs.list", "fs.search", "notes.add", "todo.add"],
    },
    Agent {
        name: "research",
        role: "Ricerca: trova informazioni sul Web, confronta le fonti e cita sempre gli URL.",
        keywords: &["cerca sul web", "cerca online", "cerca in rete", "notizi", "wikipedia", "http://", "https://", "fonti", "fai una ricerca", "naviga"],
        tools: &["web.search", "web.fetch", "web.browse", "web.form"],
    },
    Agent {
        name: "system",
        role: "Sistemi IT: inizia da diagnosi in sola lettura; distingue osservazioni e ipotesi. Prima di una modifica indica impatto, verifica e ripristino.",
        keywords: &["sottorete", "subnet", "cidr", "runbook", "processi", "processo", "terminale", "cpu", " ram", "task manager", "rallent", "che sistema", "info sistema", "informazioni di sistema", "informazioni sul sistema"],
        tools: &["system.info", "system.disks", "process.list", "it.subnet", "shell.run", "fs.list", "fs.read", "fs.search"],
    },
    Agent {
        name: "automation",
        role: "Automazioni: crea, modifica ed esegue automazioni pianificate; descrivi chiaramente quando e cosa eseguiranno.",
        keywords: &["n8n", "automazion", "pianific", "ogni giorno", "ogni ora", "ogni settimana", "tra 1", "tra 2", "tra 3", "tra 5", "tra 10", "tra un", "tra mezz"],
        tools: &["n8n.create", "workflow.create", "workflow.list", "workflow.run", "workflow.update", "workflow.delete", "system.info", "fs.write", "todo.add", "todo.list", "todo.review", "todo.done"],
    },
    Agent {
        name: "memory",
        role: "Memoria: salva, cerca, corregge e dimentica ciò che il proprietario ti chiede; frasi brevi e autonome.",
        keywords: &["ricorda che", "ricordati", "cosa ricordi", "dimentica", "nella memoria", "preferisco"],
        tools: &["memory.remember", "memory.list", "memory.forget", "memory.update", "notes.add", "notes.list", "notes.search", "notes.delete"],
    },
    Agent {
        name: "business",
        role: "Direzione aziendale (CEO/CFO): parti dalla decisione, calcola con gli strumenti, confronta opzioni con numeri e rischi, chiudi con raccomandazione e scadenze. Niente pareri legali vincolanti.",
        keywords: &[" iva ", "l'iva", "mutuo", "finanziamento", "rata del", "business plan", "analisi aziendale", "piano aziendale", "margine", "kpi", "preventivo", "bilancio", "procedura aziendale", "fornitori", "break even", "break-even", "pareggio", "markup", "ricarico", " roi ", " van ", " tir ", "payback", "runway", "burn rate", "unit economics", " cac ", " ltv ", "churn", "cagr", "cash flow", "flusso di cassa", "ebitda", "fatturato", "ricavi", "budget", "pricing", "prezzo di vendita", "strategia aziendale", "piano strategico", "analisi swot", " okr", "investitor", "consiglio di amministrazione", "verbale", "ordine del giorno", "piano commerciale", "forecast", "scenario aziendale"],
        tools: &["biz.vat", "biz.loan", "biz.margin", "biz.breakeven", "biz.npv", "biz.runway", "biz.unit_economics", "biz.priorities", "biz.cagr", "math.calc", "date.calc", "notes.add", "notes.list", "notes.search", "todo.add", "todo.list", "fs.read", "fs.list", "fs.search", "web.search"],
    },
    Agent {
        name: "coding",
        role: "Coding: leggi il codice prima di modificarlo, riproduci il problema con un test mirato, preserva i contratti, verifica errori e regressioni. Distingui i test eseguiti da quelli proposti.",
        keywords: &["codice", "script", "programma in", "python", "javascript", "compila", "git ", "npm ", "regressione", "code review", "refactoring", "test unitari", "codice rust", "codice kotlin", "codice typescript", "debug", "valida il json", "valida json", "verifica il json", "verifica json"],
        tools: &["fs.list", "fs.read", "fs.write", "fs.search", "fs.mkdir", "fs.move", "fs.delete", "shell.run", "math.calc", "code.check_python", "code.validate_json", "code.run_python", "sandbox.run", "sandbox.import", "sandbox.export", "sandbox.status", "agent.delegate"],
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
    if text.trim_start().starts_with("n8n ") {
        return SPECIALISTS.iter().find(|agent| agent.name == "automation");
    }
    if [
        "valida json:",
        "valida il json:",
        "verifica json:",
        "verifica il json:",
    ]
    .iter()
    .any(|prefix| text.trim_start().starts_with(prefix))
    {
        return SPECIALISTS.iter().find(|agent| agent.name == "coding");
    }
    let security = &SPECIALISTS[0];
    if security.keywords.iter().any(|key| text.contains(key)) {
        return Some(security);
    }
    let mut hits = SPECIALISTS
        .iter()
        .filter(|a| a.keywords.iter().any(|k| text.contains(k)));
    match (hits.next(), hits.next()) {
        (Some(a), None) => Some(a),
        _ => None,
    }
}

/// Registro degli agenti (ADR-0153): competenze, ambito di strumenti, rischio massimo e risorse. `risk_of` dà il
/// rischio di uno strumento del catalogo.
pub fn registry(risk_of: &dyn Fn(&str) -> Option<String>) -> Vec<serde_json::Value> {
    let rank = |risk: &str| match risk {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        _ => 3,
    };
    let mut out: Vec<serde_json::Value> = SPECIALISTS
        .iter()
        .map(|agent| {
            let tools: Vec<&str> = agent.tools.iter().chain(COMMON.iter()).copied().collect();
            let mut categories: Vec<&str> =
                tools.iter().map(|t| crate::toolsys::category(t)).collect();
            categories.sort_unstable();
            categories.dedup();
            let max_risk = tools
                .iter()
                .filter_map(|t| risk_of(t))
                .max_by_key(|r| rank(r))
                .unwrap_or_else(|| "low".into());
            serde_json::json!({
                "name": agent.name, "role": agent.role, "kind": "specialist",
                "capabilities": categories, "tools": tools, "keywords": agent.keywords.len(),
                "max_risk": max_risk, "context": "isolated",
                "resources": {"max_steps": 6, "tool_scope": "only_granted"},
            })
        })
        .collect();
    out.push(serde_json::json!({
        "name": GENERAL, "role": "Generalista: riceve le richieste che toccano più ambiti o nessuno, con tutti gli strumenti entro policy e conferme.",
        "kind": "generalist", "capabilities": ["all"], "tools": "all", "keywords": 0, "max_risk": "critical",
        "context": "shared", "resources": {"max_steps": 8, "tool_scope": "catalog"},
    }));
    out
}

/// Perché una richiesta va a un agente: specializzazione scelta e parole chiave che l'hanno attivata.
pub fn explain(message: &str) -> serde_json::Value {
    let text = format!(" {} ", fold(message));
    let hits: Vec<serde_json::Value> = SPECIALISTS
        .iter()
        .map(|agent| {
            let matched: Vec<&str> = agent
                .keywords
                .iter()
                .copied()
                .filter(|k| text.contains(k))
                .collect();
            serde_json::json!({"agent": agent.name, "matched": matched})
        })
        .filter(|v| v["matched"].as_array().is_some_and(|m| !m.is_empty()))
        .collect();
    let chosen = route(message).map_or(GENERAL, |a| a.name);
    let reason = match (chosen == GENERAL, hits.len()) {
        (true, 0) => "nessuna parola chiave di una specializzazione: va al generalista",
        (true, _) => "la richiesta tocca più ambiti: va al generalista con tutti gli strumenti",
        (false, _) => {
            "un solo ambito riconosciuto: va alla specializzazione con i soli strumenti concessi"
        }
    };
    serde_json::json!({"agent": chosen, "reason": reason, "candidates": hits})
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
    fn every_role_fits_the_delegation_contract() {
        for agent in SPECIALISTS.iter() {
            assert!(
                agent.role.chars().count() <= 200,
                "{}: {}",
                agent.name,
                agent.role.chars().count()
            );
        }
    }

    #[test]
    fn company_it_and_security_requests_reach_their_specialists() {
        assert_eq!(name("calcola l'iva al 22% su 100 euro"), "business");
        assert_eq!(
            name("quanto è la rata del mutuo di 100000 euro"),
            "business"
        );
        assert_eq!(name("che gravità ha questo vettore cvss"), "monitoring");
        assert_eq!(name("ho ricevuto un phishing, cosa faccio"), "monitoring");
        assert_eq!(name("calcola la sottorete 10.0.0.0/8"), "system");
        let business = route("analisi aziendale dei costi").unwrap();
        assert!(
            allows(business, "biz.vat")
                && !allows(business, "shell.run")
                && !allows(business, "fs.write")
        );
        let security = route("cvss 9.8").unwrap();
        assert!(allows(security, "security.hash") && !allows(security, "shell.run"));
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
