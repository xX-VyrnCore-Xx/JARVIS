//! Memoria del proprietario (MEMORY_DATA_SYNC.md, ADR-0022).
//!
//! Tre tipi di elemento, tutti in SQLite con provenienza e scadenza facoltativa:
//! - **semantica**: fatti e conoscenze («ricorda che …»);
//! - **procedurale**: preferenze e modi di lavorare («preferisco …», «d'ora in poi …»), sempre nel contesto;
//! - **episodica**: riassunto di ogni attività conclusa, legato al task, con scadenza (30 giorni predefiniti).
//!
//! Indici derivati, ricostruibili in ogni momento dalla tabella `memory` (`reindex`):
//! - `memory_fts`: indice testuale FTS5 (BM25);
//! - `memory_vectors`: vettore locale da 256 dimensioni (parole normalizzate e trigrammi con hashing), senza
//!   modelli esterni: la ricerca resta offline e deterministica;
//! - `memory_edges`: knowledge graph di triple (soggetto, relazione, oggetto) estratte dagli elementi semantici.
//!
//! Correzione e cancellazione aggiornano subito tutti i derivati.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

pub const KINDS: [&str; 3] = ["semantic", "procedural", "episodic"];

/// Embedding da un modello (ADR-0028): riconosce i sinonimi che il vettore locale non vede.
pub trait Embedder: Send + Sync {
    /// Nome del modello, salvato con ogni vettore: cambiare modello rende i vecchi vettori da ricalcolare.
    fn model(&self) -> &str;
    /// `None` se il servizio non risponde: la memoria continua con l'indice locale.
    fn embed(&self, text: &str) -> Option<Vec<f32>>;
}

/// Embedding con Ollama (`POST /api/embed`), con pausa di 60 secondi dopo un errore per non rallentare la chat.
pub struct OllamaEmbedder {
    url: String,
    model: String,
    down_until: std::sync::Mutex<Option<std::time::Instant>>,
}

impl OllamaEmbedder {
    pub fn new(url: &str, model: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            down_until: std::sync::Mutex::new(None),
        }
    }
}

impl Embedder for OllamaEmbedder {
    fn model(&self) -> &str {
        &self.model
    }

    fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let mut down = self.down_until.lock().unwrap_or_else(|e| e.into_inner());
        if down.is_some_and(|t| std::time::Instant::now() < t) {
            return None;
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(8))
            .redirects(0)
            .build();
        let result = agent
            .post(&format!("{}/api/embed", self.url))
            .set("Content-Type", "application/json")
            .send_string(
                &json!({"model": self.model, "input": text.chars().take(2000).collect::<String>()})
                    .to_string(),
            )
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|b| serde_json::from_str::<Value>(&b).ok())
            .and_then(|v| {
                v["embeddings"][0].as_array().map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_f64().map(|f| f as f32))
                        .collect::<Vec<f32>>()
                })
            })
            .filter(|v| !v.is_empty());
        *down = if result.is_some() {
            None
        } else {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(60))
        };
        result.map(|mut v| {
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                v.iter_mut().for_each(|x| *x /= norm);
            }
            v
        })
    }
}
pub const DIM: usize = 256;
/// Punteggio minimo perché un elemento sia considerato pertinente.
pub const MIN_SCORE: f32 = 0.12;

const STOPWORDS: &[&str] = &[
    "il", "lo", "la", "i", "gli", "le", "un", "uno", "una", "di", "da", "del", "della", "dei",
    "delle", "che", "e", "ed", "o", "a", "al", "alla", "ai", "in", "nel", "nella", "su", "per",
    "con", "tra", "fra", "mi", "ti", "si", "ci", "vi", "me", "te", "non", "come", "cosa", "quale",
    "quali", "chi", "sono", "sei", "the", "a", "an", "of", "to", "is", "are", "and", "or", "my",
    "what", "who",
];

/// Minuscole senza accenti, solo lettere e cifre.
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect()
}

/// Parole significative: niente parole vuote, radice approssimata alle prime 6 lettere (cane/cani, lavoro/lavora).
pub fn terms(text: &str) -> Vec<String> {
    normalize(text)
        .split_whitespace()
        .filter(|w| w.chars().count() > 1 && !STOPWORDS.contains(w))
        .map(|w| {
            let n = w.chars().count();
            let keep = if n > 4 { (n - 1).min(6) } else { n };
            w.chars().take(keep).collect()
        })
        .collect()
}

fn hash(s: &str) -> u64 {
    // FNV-1a: stabile fra versioni e piattaforme, indispensabile per un indice persistente.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Vettore normalizzato: radici delle parole (peso 1) e trigrammi di carattere (peso 0,5).
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    let mut add = |feature: &str, weight: f32| {
        let h = hash(feature);
        let sign = if h & 1 == 0 { 1.0 } else { -1.0 };
        v[(h >> 1) as usize % DIM] += sign * weight;
    };
    for t in terms(text) {
        add(&format!("w:{t}"), 1.0);
        let padded: Vec<char> = format!(" {t} ").chars().collect();
        for g in padded.windows(3) {
            add(&format!("g:{}", g.iter().collect::<String>()), 0.5);
        }
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Triple estratte da un fatto semantico. Il proprietario è il nodo «io».
pub fn extract_triples(text: &str) -> Vec<(String, String, String)> {
    let t = text.trim().trim_end_matches(['.', '!', ';']).trim();
    let lower = t.to_lowercase();
    // Le posizioni si calcolano sulla forma minuscola: se cambia la lunghezza in byte si usa quella.
    let t: &str = if lower.len() == t.len() { t } else { &lower };
    let clean = |s: &str| s.trim().trim_matches(['"', '«', '»']).trim().to_string();
    let first_person: [(&str, &str); 9] = [
        ("mi chiamo ", "si chiama"),
        ("abito a ", "abita a"),
        ("abito in ", "abita in"),
        ("vivo a ", "vive a"),
        ("lavoro come ", "lavora come"),
        ("lavoro per ", "lavora per"),
        ("lavoro da ", "lavora da"),
        ("lavoro a ", "lavora a"),
        ("sono nato a ", "è nato a"),
    ];
    let body = lower.strip_prefix("io ").unwrap_or(&lower);
    let offset = lower.len() - body.len();
    for (prefix, relation) in first_person {
        if let Some(rest) = body.strip_prefix(prefix) {
            let obj = &t[offset + prefix.len()..offset + prefix.len() + rest.len()];
            return vec![("io".into(), relation.into(), clean(obj))];
        }
    }
    // «il mio cane si chiama Fido», «mia sorella è Anna», «i miei colori preferiti sono blu e verde»
    let possessive = [
        "il mio ", "la mia ", "lo mio ", "i miei ", "le mie ", "mio ", "mia ", "miei ", "mie ",
    ];
    for p in possessive {
        if let Some(rest) = lower.strip_prefix(p) {
            for verb in [
                " si chiama ",
                " si chiamano ",
                " è ",
                " sono ",
                " vive a ",
                " abita a ",
            ] {
                if let Some(pos) = rest.find(verb) {
                    let noun = &t[p.len()..p.len() + pos];
                    let obj = &t[p.len() + pos + verb.len()..];
                    let relation = match verb.trim() {
                        "si chiama" | "si chiamano" | "è" | "sono" => clean(noun),
                        other => format!("{} {}", clean(noun), other),
                    };
                    return vec![("io".into(), relation, clean(obj))];
                }
            }
        }
    }
    for verb in [
        " si chiama ",
        " abita a ",
        " vive a ",
        " lavora per ",
        " lavora a ",
        " lavora come ",
        " è ",
        " sono ",
    ] {
        if let Some(pos) = lower.find(verb) {
            let subj = clean(&t[..pos]);
            let obj = clean(&t[pos + verb.len()..]);
            if !subj.is_empty() && !obj.is_empty() && subj.split_whitespace().count() <= 5 {
                return vec![(subj, verb.trim().into(), obj)];
            }
        }
    }
    vec![]
}

/// Tipo dedotto dal testo quando non è indicato: preferenze e regole sono procedurali.
pub fn infer_kind(text: &str) -> &'static str {
    let l = normalize(text);
    let starts = |p: &str| l.trim_start().starts_with(p);
    if starts("preferisco")
        || starts("d ora in poi")
        || starts("da ora in poi")
        || starts("sempre ")
        || starts("non ")
        || starts("quando ")
        || starts("di solito")
        || l.contains("rispondimi")
        || l.contains("chiamami")
    {
        "procedural"
    } else {
        "semantic"
    }
}

pub struct NewItem<'a> {
    pub id: &'a str,
    pub text: &'a str,
    pub kind: &'a str,
    pub source: Value,
    pub expires_at: Option<String>,
}

impl Store {
    /// Colonne e indici derivati della memoria; idempotente.
    pub(crate) fn memory_migrate(&self) -> StoreResult<()> {
        let has_kind = self.conn.prepare("SELECT kind FROM memory LIMIT 0").is_ok();
        if !has_kind {
            self.conn.execute_batch(
                "ALTER TABLE memory ADD COLUMN kind TEXT NOT NULL DEFAULT 'semantic';
                 ALTER TABLE memory ADD COLUMN source TEXT;
                 ALTER TABLE memory ADD COLUMN updated_at TEXT;
                 ALTER TABLE memory ADD COLUMN expires_at TEXT;",
            )?;
        }
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS memory_by_kind ON memory (kind, created_at);
             CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5 (id UNINDEXED, text, tokenize = 'unicode61 remove_diacritics 2');
             CREATE TABLE IF NOT EXISTS memory_vectors (id TEXT PRIMARY KEY, vec BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS memory_embeddings (id TEXT PRIMARY KEY, model TEXT NOT NULL, vec BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS memory_edges (memory_id TEXT NOT NULL, subject TEXT NOT NULL,
                 relation TEXT NOT NULL, object TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS memory_edges_by_memory ON memory_edges (memory_id);",
        )?;
        // Database precedenti: indici derivati mancanti per elementi già presenti.
        let missing: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM memory WHERE id NOT IN (SELECT id FROM memory_vectors)",
            [],
            |r| r.get(0),
        )?;
        if missing > 0 {
            self.memory_reindex()?;
        }
        Ok(())
    }

    fn memory_index(&self, id: &str, text: &str, kind: &str) -> StoreResult<()> {
        self.memory_unindex(id)?;
        self.conn.execute(
            "INSERT INTO memory_fts (id, text) VALUES (?1, ?2)",
            params![id, text],
        )?;
        self.conn.execute(
            "INSERT INTO memory_vectors (id, vec) VALUES (?1, ?2)",
            params![id, to_blob(&embed(text))],
        )?;
        if let Some(e) = &self.embedder {
            if let Some(v) = e.embed(text) {
                self.conn.execute(
                    "INSERT INTO memory_embeddings (id, model, vec) VALUES (?1, ?2, ?3)",
                    params![id, e.model(), to_blob(&v)],
                )?;
            }
        }
        if kind == "semantic" {
            for (s, r, o) in extract_triples(text) {
                self.conn.execute(
                    "INSERT INTO memory_edges (memory_id, subject, relation, object) VALUES (?1, ?2, ?3, ?4)",
                    params![id, s, r, o],
                )?;
            }
        }
        Ok(())
    }

    fn memory_unindex(&self, id: &str) -> StoreResult<()> {
        self.conn
            .execute("DELETE FROM memory_fts WHERE id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM memory_vectors WHERE id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM memory_embeddings WHERE id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM memory_edges WHERE memory_id = ?1", [id])?;
        Ok(())
    }

    /// Ricostruisce tutti gli indici derivati dalla sola tabella `memory`. Restituisce il numero di elementi.
    pub fn memory_reindex(&self) -> StoreResult<usize> {
        self.conn.execute_batch(
            "DELETE FROM memory_fts; DELETE FROM memory_vectors; DELETE FROM memory_edges; DELETE FROM memory_embeddings;",
        )?;
        let rows: Vec<(String, String, String)> = {
            let mut stmt = self.conn.prepare("SELECT id, text, kind FROM memory")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<StoreResult<_>>()?
        };
        for (id, text, kind) in &rows {
            self.memory_index(id, text, kind)?;
        }
        Ok(rows.len())
    }

    /// Calcola gli embedding mancanti per il modello attivo; si ferma al primo errore del servizio.
    pub fn memory_embed_missing(&self) -> StoreResult<usize> {
        let Some(e) = self.embedder.clone() else {
            return Ok(0);
        };
        let rows: Vec<(String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, text FROM memory WHERE id NOT IN (SELECT id FROM memory_embeddings WHERE model = ?1)",
            )?;
            let rows = stmt.query_map([e.model()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<StoreResult<_>>()?
        };
        let mut done = 0;
        for (id, text) in rows {
            let Some(v) = e.embed(&text) else { break };
            self.conn.execute(
                "INSERT OR REPLACE INTO memory_embeddings (id, model, vec) VALUES (?1, ?2, ?3)",
                params![id, e.model(), to_blob(&v)],
            )?;
            done += 1;
        }
        Ok(done)
    }

    /// Elimina gli elementi scaduti con i loro derivati; restituisce quanti.
    pub fn memory_purge_expired(&self, now: &str) -> StoreResult<usize> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM memory WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            )?;
            let rows = stmt.query_map([now], |r| r.get(0))?;
            rows.collect::<StoreResult<_>>()?
        };
        for id in &ids {
            self.memory_delete(id)?;
        }
        Ok(ids.len())
    }

    pub fn memory_insert(&self, item: &NewItem<'_>, at: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO memory (id, text, created_at, kind, source, updated_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?3, ?6)",
            params![
                item.id,
                item.text,
                at,
                item.kind,
                item.source.to_string(),
                item.expires_at
            ],
        )?;
        self.memory_index(item.id, item.text, item.kind)
    }

    pub fn memory_add(&self, id: &str, text: &str, at: &str) -> StoreResult<()> {
        self.memory_insert(
            &NewItem {
                id,
                text,
                kind: infer_kind(text),
                source: json!({"kind": "owner"}),
                expires_at: None,
            },
            at,
        )
    }

    /// Correzione del testo: aggiorna l'elemento e tutti i derivati. Falso se l'elemento non esiste.
    pub fn memory_update(&self, id: &str, text: &str, at: &str) -> StoreResult<bool> {
        let Some(kind) = self
            .conn
            .query_row("SELECT kind FROM memory WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        else {
            return Ok(false);
        };
        self.conn.execute(
            "UPDATE memory SET text = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, text, at],
        )?;
        self.memory_index(id, text, &kind)?;
        Ok(true)
    }

    fn memory_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
        let source: Option<String> = r.get(4)?;
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "text": r.get::<_, String>(1)?,
            "created_at": r.get::<_, String>(2)?,
            "kind": r.get::<_, String>(3)?,
            "source": source.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null),
            "updated_at": r.get::<_, Option<String>>(5)?,
            "expires_at": r.get::<_, Option<String>>(6)?,
        }))
    }

    const COLUMNS: &'static str = "id, text, created_at, kind, source, updated_at, expires_at";

    pub fn memory_list(&self) -> StoreResult<Vec<Value>> {
        self.memory_list_kind(None)
    }

    pub fn memory_list_kind(&self, kind: Option<&str>) -> StoreResult<Vec<Value>> {
        let sql = format!(
            "SELECT {} FROM memory WHERE (?1 IS NULL OR kind = ?1) ORDER BY created_at, id",
            Self::COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([kind], Self::memory_row)?;
        rows.collect()
    }

    pub fn memory_get(&self, id: &str) -> StoreResult<Option<String>> {
        self.conn
            .query_row("SELECT text FROM memory WHERE id = ?1", [id], |r| r.get(0))
            .optional()
    }

    pub fn memory_delete(&self, id: &str) -> StoreResult<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM memory WHERE id = ?1", [id])?
            > 0;
        self.memory_unindex(id)?;
        Ok(deleted)
    }

    /// Ricerca ibrida: similarità del vettore locale (60%) e posizione BM25 nell'indice testuale (40%).
    /// Con un modello di embedding (ADR-0028) il punteggio diventa 30% locale, 25% BM25 e fino a 60% dal
    /// modello, che riconosce i sinonimi.
    pub fn memory_search(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
    ) -> StoreResult<Vec<Value>> {
        let q = embed(query);
        let semantic: Option<(String, Vec<f32>)> = self
            .embedder
            .as_ref()
            .and_then(|e| e.embed(query).map(|v| (e.model().to_string(), v)));
        let (local_w, text_w) = if semantic.is_some() {
            (0.3, 0.25)
        } else {
            (0.6, 0.4)
        };
        let mut scores: BTreeMap<String, f32> = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT v.id, v.vec FROM memory_vectors v JOIN memory m ON m.id = v.id
                 WHERE (?1 IS NULL OR m.kind = ?1)",
            )?;
            let rows = stmt.query_map([kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })?;
            for row in rows {
                let (id, blob) = row?;
                scores.insert(id, local_w * cosine(&q, &from_blob(&blob)).max(0.0));
            }
        }
        if let Some((model, qv)) = &semantic {
            let mut stmt = self.conn.prepare(
                "SELECT e.id, e.vec FROM memory_embeddings e JOIN memory m ON m.id = e.id
                 WHERE e.model = ?1 AND (?2 IS NULL OR m.kind = ?2)",
            )?;
            let rows = stmt.query_map(params![model, kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })?;
            for row in rows {
                let (id, blob) = row?;
                let v = from_blob(&blob);
                if v.len() == qv.len() {
                    // I modelli danno similarità alte anche fra testi lontani: conta solo la parte sopra 0,4.
                    let sim = ((cosine(qv, &v) - 0.4) / 0.6).clamp(0.0, 1.0);
                    *scores.entry(id).or_default() += 0.6 * sim;
                }
            }
        }
        let fts_terms: Vec<String> = normalize(query)
            .split_whitespace()
            .filter(|w| w.chars().count() > 1 && !STOPWORDS.contains(w))
            .map(|w| format!("\"{}\"*", w.chars().take(6).collect::<String>()))
            .collect();
        if !fts_terms.is_empty() {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM memory_fts WHERE memory_fts MATCH ?1 ORDER BY bm25(memory_fts) LIMIT 50",
            )?;
            let rows = stmt.query_map([fts_terms.join(" OR ")], |r| r.get::<_, String>(0))?;
            for (rank, id) in rows.enumerate() {
                if let Some(s) = scores.get_mut(&id?) {
                    *s += text_w / (1.0 + rank as f32 * 0.25);
                }
            }
        }
        let mut ranked: Vec<(String, f32)> = scores
            .into_iter()
            .filter(|(_, s)| *s >= MIN_SCORE)
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let sql = format!("SELECT {} FROM memory WHERE id = ?1", Self::COLUMNS);
        let mut out = vec![];
        for (id, score) in ranked.into_iter().take(limit) {
            if let Some(mut item) = self
                .conn
                .query_row(&sql, [&id], Self::memory_row)
                .optional()?
            {
                item["score"] = json!((score * 1000.0).round() / 1000.0);
                out.push(item);
            }
        }
        Ok(out)
    }

    /// Knowledge graph: nodi unici e archi con l'elemento di memoria che li giustifica.
    pub fn memory_graph(&self) -> StoreResult<Value> {
        let mut stmt = self.conn.prepare(
            "SELECT e.memory_id, e.subject, e.relation, e.object FROM memory_edges e
             JOIN memory m ON m.id = e.memory_id ORDER BY m.created_at, e.rowid",
        )?;
        let edges: Vec<(String, String, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<StoreResult<_>>()?;
        let mut seen = HashSet::new();
        let mut nodes = vec![];
        for (_, s, _, o) in &edges {
            for n in [s, o] {
                if seen.insert(n.to_lowercase()) {
                    nodes.push(json!({"id": n.to_lowercase(), "label": if n == "io" { "Tu" } else { n.as_str() }}));
                }
            }
        }
        let edges: Vec<Value> = edges
            .into_iter()
            .map(|(m, s, r, o)| json!({"memory_id": m, "from": s.to_lowercase(), "relation": r, "to": o.to_lowercase()}))
            .collect();
        Ok(json!({"nodes": nodes, "edges": edges}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddings_tolerate_accents_and_inflections() {
        let a = embed("Il mio cane si chiama Fido");
        let b = embed("come si chiamano i cani?");
        let c = embed("la riunione è alle nove");
        assert!(cosine(&a, &b) > cosine(&a, &c), "parole affini più vicine");
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-4);
        assert_eq!(normalize("Perché è così"), "perche e cosi");
    }

    #[test]
    fn triples_from_common_italian_phrasings() {
        assert_eq!(
            extract_triples("Il mio cane si chiama Fido."),
            vec![("io".into(), "cane".into(), "Fido".into())]
        );
        assert_eq!(
            extract_triples("abito a Torino"),
            vec![("io".into(), "abita a".into(), "Torino".into())]
        );
        assert_eq!(
            extract_triples("Marco è il mio collega"),
            vec![("Marco".into(), "è".into(), "il mio collega".into())]
        );
        assert!(extract_triples("compra il latte").is_empty());
    }

    #[test]
    fn kinds_are_inferred_from_phrasing() {
        assert_eq!(infer_kind("preferisco risposte brevi"), "procedural");
        assert_eq!(
            infer_kind("D'ora in poi rispondimi in inglese"),
            "procedural"
        );
        assert_eq!(infer_kind("il mio cane si chiama Fido"), "semantic");
    }

    #[test]
    fn old_databases_are_migrated_and_indexed() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-mem-mig-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jarvis.sqlite3");
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE memory (id TEXT PRIMARY KEY, text TEXT NOT NULL, created_at TEXT NOT NULL);
                 INSERT INTO memory VALUES ('a1', 'mia sorella si chiama Anna', '2026-01-01T00:00:00Z');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let items = store.memory_list().unwrap();
        assert_eq!(items[0]["kind"], "semantic");
        assert_eq!(store.memory_search("Anna", None, 5).unwrap()[0]["id"], "a1");
        assert!(store
            .memory_graph()
            .unwrap()
            .to_string()
            .contains("sorella"));
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Embedder finto: «auto», «macchina» e «vettura» hanno lo stesso significato.
    struct Synonyms;
    impl Embedder for Synonyms {
        fn model(&self) -> &str {
            "finto"
        }
        fn embed(&self, text: &str) -> Option<Vec<f32>> {
            let t = normalize(text);
            let car = ["auto", "macchina", "vettura"]
                .iter()
                .any(|w| t.split_whitespace().any(|x| x.starts_with(w)));
            let food = ["pizza", "pasta"].iter().any(|w| t.contains(w));
            let v = vec![
                if car { 1.0 } else { 0.0 },
                if food { 1.0 } else { 0.0 },
                0.1,
            ];
            let n = v.iter().map(|x: &f32| x * x).sum::<f32>().sqrt();
            Some(v.into_iter().map(|x| x / n).collect())
        }
    }

    #[test]
    fn model_embeddings_find_synonyms_and_are_rebuilt() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-mem-syn-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(&dir.join("jarvis.sqlite3")).unwrap();
        store
            .memory_add(
                "m1",
                "ho comprato una macchina rossa",
                "2026-01-01T00:00:00Z",
            )
            .unwrap();
        store
            .memory_add(
                "m2",
                "la mia pizza preferita è la margherita",
                "2026-01-01T00:00:01Z",
            )
            .unwrap();
        assert!(
            store
                .memory_search("parcheggio dell'auto", None, 5)
                .unwrap()
                .iter()
                .all(|i| i["id"] != "m1"),
            "senza modello il sinonimo non si trova"
        );
        // Attivando il modello si calcolano gli embedding degli elementi già presenti.
        assert_eq!(
            store
                .set_embedder(Some(std::sync::Arc::new(Synonyms)))
                .unwrap(),
            2
        );
        let found = store
            .memory_search("parcheggio dell'auto", None, 5)
            .unwrap();
        assert_eq!(found[0]["id"], "m1", "{found:?}");
        assert!(found.iter().all(|i| i["id"] != "m2"));
        // Correzione e cancellazione aggiornano anche gli embedding.
        store
            .memory_update("m1", "ho venduto la vettura", "2026-01-02T00:00:00Z")
            .unwrap();
        assert_eq!(store.memory_search("auto", None, 5).unwrap()[0]["id"], "m1");
        store.memory_delete("m1").unwrap();
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.memory_reindex().unwrap(), 1);
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "ricostruiti dalla tabella sorgente");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ollama_embedder_reads_api_and_pauses_after_errors() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (i, stream) in listener.incoming().enumerate() {
                let mut stream = stream.unwrap();
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut l = String::new();
                    r.read_line(&mut l).unwrap();
                    if l == "\r\n" || l.is_empty() {
                        break;
                    }
                    if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut b = vec![0u8; len];
                r.read_exact(&mut b).unwrap();
                let req: Value = serde_json::from_slice(&b).unwrap();
                assert_eq!(req["model"], "nomic-embed-text");
                let (status, body) = if i == 0 {
                    ("200 OK", json!({"embeddings": [[3.0, 4.0]]}).to_string())
                } else {
                    ("500 X", "{}".into())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let e = OllamaEmbedder::new(&url, "nomic-embed-text");
        assert_eq!(
            e.embed("ciao").unwrap(),
            vec![0.6, 0.8],
            "vettore normalizzato"
        );
        assert!(e.embed("ciao").is_none(), "errore del servizio");
        assert!(
            e.embed("ciao").is_none(),
            "pausa dopo l'errore senza nuove richieste"
        );
    }
}
