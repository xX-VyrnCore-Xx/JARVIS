//! Nodi esecutori (RF-11, ADR-0017): computer associati che eseguono strumenti assegnati dal Core.
//!
//! Il nodo si associa con un codice monouso (permesso `node`), poi chiede lavoro al Core con richieste
//! in uscita (`GET /api/nodes/next`, attesa lunga) e restituisce gli esiti (`POST /api/nodes/result/{id}`).
//! Non apre porte. Il Core decide policy, conferme e audit come per gli strumenti locali; il nodo esegue
//! solo gli strumenti dichiarati all'associazione, dentro la propria workspace.
//!
//! Il Core attende l'esito tenendo il proprio blocco: per questo l'hub ha un blocco separato e un elenco
//! proprio delle impronte dei token, così le richieste dei nodi non passano mai dal blocco del Core.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::Digest;

/// Strumenti che un nodo può offrire: lettura e scrittura nella propria workspace, informazioni di sistema,
/// processi e ora. Niente memoria, Web, automazioni o terminale, che restano al Core (ADR-0038).
pub const NODE_TOOLS: [&str; 6] = [
    "system.info",
    "time.now",
    "fs.list",
    "fs.read",
    "fs.write",
    "process.list",
];

pub fn token_hash(token: &str) -> String {
    hex::encode(sha2::Sha256::digest(token.as_bytes()))
}

#[derive(Clone, Debug)]
pub struct Assignment {
    pub id: String,
    pub tool: String,
    pub arguments: Value,
}

impl Assignment {
    pub fn to_json(&self) -> Value {
        json!({"assignment_id": self.id, "tool": self.tool, "arguments": self.arguments})
    }
}

/// Esito di un'assegnazione: output dello strumento oppure (codice, messaggio).
pub type Outcome = Result<Value, (String, String)>;

struct Pending {
    node_id: String,
    delivered: bool,
}

#[derive(Default)]
struct State {
    /// impronta del token -> id del nodo
    tokens: HashMap<String, String>,
    queues: HashMap<String, VecDeque<Assignment>>,
    /// assegnazioni consegnate o in coda -> nodo che deve rispondere
    pending: HashMap<String, Pending>,
    results: HashMap<String, Outcome>,
    last_seen: HashMap<String, Instant>,
    revoked: HashSet<String>,
    /// Risorse dichiarate dal nodo all'ultima richiesta di lavoro (ADR-0031).
    resources: HashMap<String, Resources>,
    /// Impronta del certificato TLS del nodo (ADR-0041): con l'mTLS il token da solo non basta.
    certs: HashMap<String, String>,
}

/// Risorse dichiarate da un nodo: CPU disponibili, carico medio dell'ultimo minuto e memoria libera.
#[derive(Clone, Copy, Debug, Default)]
pub struct Resources {
    pub cpus: u32,
    pub load: Option<f64>,
    pub mem_free_mb: Option<u64>,
}

#[derive(Default)]
pub struct NodeHub {
    state: Mutex<State>,
    changed: Condvar,
}

impl NodeHub {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn register(&self, node_id: &str, hash: &str) {
        let mut s = self.lock();
        s.tokens.insert(hash.to_string(), node_id.to_string());
        s.revoked.remove(node_id);
    }

    /// Lega il nodo al certificato TLS con questa impronta (ADR-0041).
    pub fn bind_cert(&self, node_id: &str, fingerprint: &str) {
        self.lock()
            .certs
            .insert(node_id.to_string(), fingerprint.to_string());
    }

    /// Vero se l'impronta appartiene a un nodo associato in questa sessione (verifica dell'handshake TLS).
    /// Un nodo revocato completa l'handshake e riceve 401, così si ferma invece di riprovare all'infinito.
    pub fn cert_allowed(&self, fingerprint: &str) -> bool {
        self.lock().certs.values().any(|fp| fp == fingerprint)
    }

    /// Autenticazione completa: token valido e, se il nodo è legato a un certificato, connessione TLS con
    /// quel certificato. Un token rubato non basta senza la chiave privata del nodo.
    pub fn authenticate_peer(&self, token: &str, peer: Option<&str>) -> Option<String> {
        let id = self.authenticate(token)?;
        match self.lock().certs.get(&id) {
            Some(fp) if peer != Some(fp.as_str()) => None,
            _ => Some(id),
        }
    }

    /// Revoca: il token smette di valere, la coda viene svuotata e le assegnazioni in attesa falliscono.
    pub fn revoke(&self, node_id: &str) {
        let mut s = self.lock();
        s.tokens.retain(|_, id| id != node_id);
        s.queues.remove(node_id);
        s.revoked.insert(node_id.to_string());
        let lost: Vec<String> = s
            .pending
            .iter()
            .filter(|(_, assignment)| assignment.node_id == node_id)
            .map(|(a, _)| a.clone())
            .collect();
        for a in lost {
            s.pending.remove(&a);
            s.results.insert(
                a,
                Err(("node.revoked".into(), "il nodo è stato revocato".into())),
            );
        }
        drop(s);
        self.changed.notify_all();
    }

    pub fn authenticate(&self, token: &str) -> Option<String> {
        if token.len() != 64 {
            return None;
        }
        self.lock().tokens.get(&token_hash(token)).cloned()
    }

    /// Vero se il nodo ha chiesto lavoro negli ultimi `within`.
    pub fn online(&self, node_id: &str, within: Duration) -> bool {
        self.lock()
            .last_seen
            .get(node_id)
            .is_some_and(|t| t.elapsed() <= within)
    }

    /// Aggiorna le risorse dichiarate dal nodo.
    pub fn report(&self, node_id: &str, res: Resources) {
        self.lock().resources.insert(node_id.to_string(), res);
    }

    /// Scelta automatica (ADR-0031): fra i candidati in linea, quello con meno lavori in corso, poi carico
    /// per CPU più basso, poi più memoria libera. `None` se nessuno è in linea.
    pub fn pick(&self, candidates: &[String], within: Duration) -> Option<String> {
        let s = self.lock();
        let mut best: Option<(usize, f64, i64, String)> = None;
        for id in candidates {
            if s.revoked.contains(id) || s.last_seen.get(id).is_none_or(|t| t.elapsed() > within) {
                continue;
            }
            let busy = s.pending.values().filter(|a| &a.node_id == id).count();
            let r = s.resources.get(id).copied().unwrap_or_default();
            let load = r.load.unwrap_or(0.5) / f64::from(r.cpus.max(1));
            let mem = -(r.mem_free_mb.unwrap_or(0) as i64);
            let key = (busy, load, mem, id.clone());
            if best
                .as_ref()
                .is_none_or(|b| (key.0, key.1, key.2) < (b.0, b.1, b.2))
            {
                best = Some(key);
            }
        }
        best.map(|b| b.3)
    }

    pub fn enqueue(&self, node_id: &str, tool: &str, arguments: &Value) -> Option<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut s = self.lock();
        if s.revoked.contains(node_id) {
            return None;
        }
        s.pending.insert(
            id.clone(),
            Pending {
                node_id: node_id.to_string(),
                delivered: false,
            },
        );
        s.queues
            .entry(node_id.to_string())
            .or_default()
            .push_back(Assignment {
                id: id.clone(),
                tool: tool.to_string(),
                arguments: arguments.clone(),
            });
        drop(s);
        self.changed.notify_all();
        Some(id)
    }

    /// Attende l'esito; allo scadere l'assegnazione viene ritirata (se ancora in coda) e restituisce
    /// `node.timeout`. Un esito arrivato dopo la scadenza viene ignorato.
    pub fn wait_result(&self, assignment_id: &str, timeout: Duration) -> Outcome {
        let deadline = Instant::now() + timeout;
        let mut s = self.lock();
        loop {
            if let Some(r) = s.results.remove(assignment_id) {
                return r;
            }
            let now = Instant::now();
            if now >= deadline {
                if let Some(assignment) = s.pending.remove(assignment_id) {
                    if let Some(q) = s.queues.get_mut(&assignment.node_id) {
                        q.retain(|a| a.id != assignment_id);
                    }
                }
                return Err((
                    "node.timeout".into(),
                    "il nodo non ha risposto in tempo".into(),
                ));
            }
            s = self
                .changed
                .wait_timeout(s, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Prossima assegnazione per il nodo, attendendo al massimo `wait`. Registra la presenza del nodo.
    pub fn next(&self, node_id: &str, wait: Duration) -> Option<Assignment> {
        let deadline = Instant::now() + wait;
        let mut s = self.lock();
        loop {
            s.last_seen.insert(node_id.to_string(), Instant::now());
            if s.revoked.contains(node_id) {
                return None;
            }
            if let Some(a) = s.queues.get_mut(node_id).and_then(VecDeque::pop_front) {
                if let Some(pending) = s.pending.get_mut(&a.id) {
                    pending.delivered = true;
                }
                return Some(a);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            s = self
                .changed
                .wait_timeout(s, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Accetta il primo esito di un'assegnazione consegnata a quel nodo.
    pub fn complete(&self, node_id: &str, assignment_id: &str, outcome: Outcome) -> bool {
        let mut s = self.lock();
        if !s
            .pending
            .get(assignment_id)
            .is_some_and(|a| a.delivered && a.node_id == node_id)
        {
            return false;
        }
        s.pending.remove(assignment_id);
        s.results.insert(assignment_id.to_string(), outcome);
        drop(s);
        self.changed.notify_all();
        true
    }
}

/// Nome breve del nodo usato negli strumenti (`node.<slug>.<strumento>`): minuscole, cifre e trattini.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out
        .trim_end_matches('-')
        .chars()
        .take(24)
        .collect::<String>();
    let out = out.trim_end_matches('-').to_string();
    // «auto» è riservato alla scelta automatica del nodo (ADR-0031).
    if out == "auto" {
        return "nodo-auto".into();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        out
    } else {
        format!(
            "nodo{}",
            if out.is_empty() {
                String::new()
            } else {
                format!("-{out}")
            }
        )
    }
}

/// Descrittore dello strumento sul nodo, derivato da quello del catalogo statico: stesso schema, rischio
/// e conferma, ambito `node`.
pub fn tool_descriptor(base: &Value, slug: &str, node_name: &str) -> Value {
    let mut d = base.clone();
    let tool = base["name"].as_str().unwrap_or_default();
    d["name"] = format!("node.{slug}.{tool}").into();
    d["summary"] = format!(
        "[nodo {node_name}] {}",
        base["summary"].as_str().unwrap_or_default()
    )
    .chars()
    .take(500)
    .collect::<String>()
    .into();
    d["scope"] = "node".into();
    let mut errors: Vec<Value> = base["errors"].as_array().cloned().unwrap_or_default();
    for e in ["node.timeout", "node.revoked", "node.offline"] {
        if !errors.iter().any(|x| x == e) {
            errors.push(e.into());
        }
    }
    d["errors"] = errors.into();
    d["prerequisites"] = json!([format!("Nodo {node_name} associato e in linea")]);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_valid_identifiers() {
        assert_eq!(slug("Portatile di Marco"), "portatile-di-marco");
        assert_eq!(slug("  PC #2 (studio) "), "pc-2-studio");
        assert_eq!(slug("42"), "nodo-42");
        assert_eq!(slug("ç"), "nodo");
        assert_eq!(slug("Auto"), "nodo-auto", "nome riservato");
    }

    #[test]
    fn assignment_round_trip_and_revocation() {
        let hub = std::sync::Arc::new(NodeHub::default());
        hub.register("n1", &token_hash(&"a".repeat(64)));
        assert_eq!(hub.authenticate(&"a".repeat(64)).as_deref(), Some("n1"));
        let id = hub.enqueue("n1", "time.now", &json!({})).unwrap();
        assert!(!hub.complete("n1", &id, Ok(json!({"precoce": true}))));
        let worker = {
            let hub = hub.clone();
            std::thread::spawn(move || {
                let a = hub
                    .next("n1", Duration::from_secs(2))
                    .expect("assegnazione");
                assert!(
                    !hub.complete("n2", &a.id, Ok(json!({}))),
                    "un altro nodo non può rispondere"
                );
                assert!(hub.complete("n1", &a.id, Ok(json!({"ok": 1}))));
                assert!(!hub.complete("n1", &a.id, Ok(json!({"duplicato": true}))));
            })
        };
        assert_eq!(
            hub.wait_result(&id, Duration::from_secs(5)).unwrap()["ok"],
            1
        );
        worker.join().unwrap();
        assert!(hub.online("n1", Duration::from_secs(5)));

        let id = hub.enqueue("n1", "time.now", &json!({})).unwrap();
        let h = hub.clone();
        let revoker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            h.revoke("n1");
        });
        assert_eq!(
            hub.wait_result(&id, Duration::from_secs(5)).unwrap_err().0,
            "node.revoked"
        );
        revoker.join().unwrap();
        assert!(hub.authenticate(&"a".repeat(64)).is_none());
        assert!(hub.next("n1", Duration::from_millis(10)).is_none());
        assert!(hub.enqueue("n1", "time.now", &json!({})).is_none());

        hub.register("n3", &token_hash(&"b".repeat(64)));
        let id = hub.enqueue("n3", "time.now", &json!({})).unwrap();
        assert_eq!(
            hub.wait_result(&id, Duration::from_millis(50))
                .unwrap_err()
                .0,
            "node.timeout"
        );
        assert!(
            hub.next("n3", Duration::from_millis(10)).is_none(),
            "assegnazione scaduta ritirata dalla coda"
        );
    }

    #[test]
    fn automatic_choice_prefers_idle_and_less_loaded_nodes() {
        let hub = NodeHub::default();
        for id in ["a", "b", "c"] {
            hub.register(id, &token_hash(&id.repeat(64)));
            let _ = hub.next(id, Duration::from_millis(1));
        }
        hub.report(
            "a",
            Resources {
                cpus: 2,
                load: Some(1.8),
                mem_free_mb: Some(1000),
            },
        );
        hub.report(
            "b",
            Resources {
                cpus: 8,
                load: Some(1.0),
                mem_free_mb: Some(8000),
            },
        );
        hub.report(
            "c",
            Resources {
                cpus: 4,
                load: Some(0.2),
                mem_free_mb: Some(500),
            },
        );
        let all: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            hub.pick(&all, Duration::from_secs(60)).as_deref(),
            Some("c"),
            "carico per CPU più basso"
        );
        hub.enqueue("c", "time.now", &json!({})).unwrap();
        assert_eq!(
            hub.pick(&all, Duration::from_secs(60)).as_deref(),
            Some("b"),
            "nodo occupato evitato"
        );
        hub.revoke("b");
        assert_eq!(
            hub.pick(&all, Duration::from_secs(60)).as_deref(),
            Some("a")
        );
        assert_eq!(
            hub.pick(&["x".into()], Duration::from_secs(60)),
            None,
            "nessuno in linea"
        );
    }
}
