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

/// Strumenti che un nodo può offrire nel prototipo: lettura e scrittura nella propria workspace,
/// informazioni di sistema e ora. Niente memoria, Web o automazioni, che restano al Core.
pub const NODE_TOOLS: [&str; 5] = ["system.info", "time.now", "fs.list", "fs.read", "fs.write"];

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

#[derive(Default)]
struct State {
    /// impronta del token -> id del nodo
    tokens: HashMap<String, String>,
    queues: HashMap<String, VecDeque<Assignment>>,
    /// assegnazioni consegnate o in coda -> nodo che deve rispondere
    pending: HashMap<String, String>,
    results: HashMap<String, Outcome>,
    last_seen: HashMap<String, Instant>,
    revoked: HashSet<String>,
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

    /// Revoca: il token smette di valere, la coda viene svuotata e le assegnazioni in attesa falliscono.
    pub fn revoke(&self, node_id: &str) {
        let mut s = self.lock();
        s.tokens.retain(|_, id| id != node_id);
        s.queues.remove(node_id);
        s.revoked.insert(node_id.to_string());
        let lost: Vec<String> = s
            .pending
            .iter()
            .filter(|(_, n)| *n == node_id)
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

    pub fn enqueue(&self, node_id: &str, tool: &str, arguments: &Value) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut s = self.lock();
        s.pending.insert(id.clone(), node_id.to_string());
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
        id
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
                if let Some(node) = s.pending.remove(assignment_id) {
                    if let Some(q) = s.queues.get_mut(&node) {
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

    /// Registra l'esito inviato dal nodo. Rifiuta assegnazioni sconosciute o di un altro nodo.
    pub fn complete(&self, node_id: &str, assignment_id: &str, outcome: Outcome) -> bool {
        let mut s = self.lock();
        if s.pending.get(assignment_id).map(String::as_str) != Some(node_id) {
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
    }

    #[test]
    fn assignment_round_trip_and_revocation() {
        let hub = std::sync::Arc::new(NodeHub::default());
        hub.register("n1", &token_hash(&"a".repeat(64)));
        assert_eq!(hub.authenticate(&"a".repeat(64)).as_deref(), Some("n1"));
        let id = hub.enqueue("n1", "time.now", &json!({}));
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
            })
        };
        assert_eq!(
            hub.wait_result(&id, Duration::from_secs(5)).unwrap()["ok"],
            1
        );
        worker.join().unwrap();
        assert!(hub.online("n1", Duration::from_secs(5)));

        let id = hub.enqueue("n1", "time.now", &json!({}));
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

        hub.register("n3", &token_hash(&"b".repeat(64)));
        let id = hub.enqueue("n3", "time.now", &json!({}));
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
}
