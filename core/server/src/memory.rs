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

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::store::{Store, StoreResult};

pub const KINDS: [&str; 3] = ["semantic", "procedural", "episodic"];

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
             DROP TABLE IF EXISTS memory_embeddings;
             CREATE TABLE IF NOT EXISTS memory_edges (memory_id TEXT NOT NULL, subject TEXT NOT NULL,
                 relation TEXT NOT NULL, object TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS memory_edges_by_memory ON memory_edges (memory_id);",
        )?;
        // Database precedenti o indici interrotti da un arresto brusco: si ricostruiscono dalla sola tabella `memory`.
        if !self.memory_consistent()? {
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
            .execute("DELETE FROM memory_edges WHERE memory_id = ?1", [id])?;
        Ok(())
    }

    /// Vero se gli indici derivati (ricerca testuale e vettori) hanno esattamente gli stessi elementi della memoria.
    pub fn memory_consistent(&self) -> StoreResult<bool> {
        let count = |sql: &str| -> StoreResult<i64> { self.conn.query_row(sql, [], |r| r.get(0)) };
        let memory = count("SELECT COUNT(*) FROM memory")?;
        let fts = count("SELECT COUNT(*) FROM memory_fts")?;
        let vectors = count("SELECT COUNT(*) FROM memory_vectors")?;
        let orphans =
            count("SELECT COUNT(*) FROM memory_vectors WHERE id NOT IN (SELECT id FROM memory)")?;
        let missing =
            count("SELECT COUNT(*) FROM memory WHERE id NOT IN (SELECT id FROM memory_vectors)")?;
        Ok(memory == fts && memory == vectors && orphans == 0 && missing == 0)
    }

    /// Riepilogo della memoria: elementi per tipo, in scadenza entro 7 giorni, più vecchio e più recente.
    pub fn memory_stats(&self, now: &str) -> StoreResult<Value> {
        let mut by_kind = serde_json::Map::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT kind, COUNT(*) FROM memory GROUP BY kind ORDER BY kind")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (kind, n) = row?;
                by_kind.insert(kind, json!(n));
            }
        }
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memory", [], |r| r.get(0))?;
        let (oldest, newest): (Option<String>, Option<String>) = self.conn.query_row(
            "SELECT MIN(created_at), MAX(COALESCE(updated_at, created_at)) FROM memory",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let soon = chrono::DateTime::parse_from_rfc3339(now)
            .map(|t| (t + chrono::Duration::days(7)).to_rfc3339())
            .unwrap_or_else(|_| now.to_string());
        let expiring: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM memory WHERE expires_at IS NOT NULL AND expires_at > ?1 AND expires_at <= ?2",
            params![now, soon],
            |r| r.get(0),
        )?;
        let edges: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memory_edges", [], |r| r.get(0))?;
        Ok(json!({
            "total": total, "by_kind": by_kind, "expiring_within_7_days": expiring,
            "relations": edges, "oldest": oldest, "newest": newest,
            "indexes_consistent": self.memory_consistent()?,
        }))
    }

    /// Coppie di ricordi molto simili (stesso tipo, coseno almeno `threshold`), per decidere a mano quale dimenticare.
    /// Non elimina nulla. Al massimo `limit` coppie, le più simili per prime.
    pub fn memory_duplicates(&self, threshold: f32, limit: usize) -> StoreResult<Vec<Value>> {
        let rows: Vec<(String, String, String, Vec<f32>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT m.id, m.text, m.kind, v.vec FROM memory m JOIN memory_vectors v ON v.id = m.id
                 ORDER BY m.kind, m.id LIMIT 4000",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    from_blob(&r.get::<_, Vec<u8>>(3)?),
                ))
            })?;
            rows.collect::<StoreResult<_>>()?
        };
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for i in 0..rows.len() {
            for j in (i + 1)..rows.len() {
                if rows[i].2 != rows[j].2 {
                    break;
                }
                let similarity = cosine(&rows[i].3, &rows[j].3);
                if similarity >= threshold {
                    pairs.push((similarity, i, j));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(pairs
            .into_iter()
            .take(limit)
            .map(|(s, i, j)| {
                json!({"similarity": (s * 1000.0).round() / 1000.0, "kind": rows[i].2,
                       "a": {"id": rows[i].0, "text": rows[i].1}, "b": {"id": rows[j].0, "text": rows[j].1}})
            })
            .collect())
    }

    /// Ricostruisce tutti gli indici derivati dalla sola tabella `memory`. Restituisce il numero di elementi.
    pub fn memory_reindex(&self) -> StoreResult<usize> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "DELETE FROM memory_fts; DELETE FROM memory_vectors; DELETE FROM memory_edges;",
        )?;
        let rows: Vec<(String, String, String)> = {
            let mut stmt = self.conn.prepare("SELECT id, text, kind FROM memory")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<StoreResult<_>>()?
        };
        for (id, text, kind) in &rows {
            self.memory_index(id, text, kind)?;
        }
        tx.commit()?;
        Ok(rows.len())
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

    /// Elemento con lo stesso testo normalizzato (maiuscole, accenti e punteggiatura ignorati), se esiste:
    /// `memory.remember` lo riusa invece di creare un duplicato (ADR-0037).
    pub fn memory_find_same(&self, text: &str) -> StoreResult<Option<(String, String)>> {
        let key = normalize(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if key.is_empty() {
            return Ok(None);
        }
        let mut stmt = self.conn.prepare("SELECT id, text, kind FROM memory")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, existing, kind) = row?;
            if normalize(&existing)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                == key
            {
                return Ok(Some((id, kind)));
            }
        }
        Ok(None)
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

    /// Fissa o libera un ricordo (ADR-0145): un ricordo fissato è sempre nel contesto e non scade.
    pub fn memory_pin(&self, id: &str, pinned: bool) -> StoreResult<bool> {
        let Some(source) = self
            .conn
            .query_row("SELECT source FROM memory WHERE id = ?1", [id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()?
        else {
            return Ok(false);
        };
        let mut doc = source
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        doc["pinned"] = json!(pinned);
        self.conn.execute(
            "UPDATE memory SET source = ?2, expires_at = CASE WHEN ?3 THEN NULL ELSE expires_at END WHERE id = ?1",
            params![id, doc.to_string(), pinned],
        )?;
        Ok(true)
    }

    /// Ricordi fissati, dal più recente.
    pub fn memory_pinned(&self, limit: usize) -> StoreResult<Vec<Value>> {
        let sql = format!(
            "SELECT {} FROM memory WHERE source LIKE '%\"pinned\":true%' ORDER BY created_at DESC LIMIT ?1",
            Self::COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([limit as i64], Self::memory_row)?;
        rows.collect()
    }

    fn memory_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
        let source: Option<String> = r.get(4)?;
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "text": r.get::<_, String>(1)?,
            "created_at": r.get::<_, String>(2)?,
            "kind": r.get::<_, String>(3)?,
            "pinned": source
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|s| s["pinned"] == true),
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
    /// Combina similarità locale, corrispondenze testuali e termini equivalenti.
    pub fn memory_search(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
    ) -> StoreResult<Vec<Value>> {
        let normalized = normalize(query);
        let words: Vec<_> = normalized
            .split_whitespace()
            .filter(|word| word.chars().count() > 1 && !STOPWORDS.contains(word))
            .take(24)
            .collect();
        if words.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let query_terms: HashSet<_> = terms(query).into_iter().collect();
        let equivalents = [
            &["auto", "automobile", "macchina", "vettura"][..],
            &["riunione", "meeting"][..],
            &["cartella", "directory"][..],
            &["promemoria", "reminder"][..],
        ];
        let expanded: HashSet<_> = words
            .iter()
            .copied()
            .chain(
                equivalents
                    .iter()
                    .filter(|group| words.iter().any(|word| group.contains(word)))
                    .flat_map(|group| group.iter().copied()),
            )
            .collect();
        let mut search: Vec<_> = expanded.iter().map(|word| format!(r#""{word}""#)).collect();
        search.sort_unstable();
        let fts = search.join(" OR ");
        let q = embed(query);
        let now = chrono::Utc::now().to_rfc3339();
        // Coseno di tutti i ricordi (veloce, solo vettori); testo e frasi si valutano solo sui candidati:
        // i 300 più vicini e i risultati della ricerca testuale. Così il costo non cresce con il testo di ogni ricordo.
        let mut near: Vec<(String, f32)> = {
            let mut stmt = self.conn.prepare(
                "SELECT m.id, v.vec FROM memory m JOIN memory_vectors v ON v.id = m.id
                 WHERE (?1 IS NULL OR m.kind = ?1) AND (m.expires_at IS NULL OR m.expires_at > ?2)",
            )?;
            let rows = stmt.query_map(params![kind, now], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })?;
            let mut all = Vec::new();
            for row in rows {
                let (id, blob) = row?;
                all.push((id, cosine(&q, &from_blob(&blob)).max(0.0)));
            }
            all
        };
        let cosines: HashMap<String, f32> = near.iter().cloned().collect();
        near.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        near.truncate(300);
        let fts_ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT memory_fts.id FROM memory_fts JOIN memory m ON m.id = memory_fts.id
                 WHERE memory_fts MATCH ?1 AND (?2 IS NULL OR m.kind = ?2)
                 AND (m.expires_at IS NULL OR m.expires_at > ?3)
                 ORDER BY bm25(memory_fts), m.updated_at DESC, m.id LIMIT 80",
            )?;
            let rows = stmt.query_map(params![fts, kind, now], |r| r.get::<_, String>(0))?;
            rows.collect::<StoreResult<_>>()?
        };
        let mut candidates: Vec<String> = near.into_iter().map(|(id, _)| id).collect();
        for id in &fts_ids {
            if !candidates.contains(id) {
                candidates.push(id.clone());
            }
        }
        let mut scores = BTreeMap::new();
        let mut recency: HashMap<String, f32> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT text, COALESCE(updated_at, created_at) FROM memory WHERE id = ?1",
            )?;
            let today = chrono::Utc::now();
            for id in candidates {
                let Some((text, stamp)) = stmt
                    .query_row([&id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .optional()?
                else {
                    continue;
                };
                let text = normalize(&text);
                let document_terms: HashSet<_> = terms(&text).into_iter().collect();
                let coverage = query_terms
                    .iter()
                    .filter(|term| document_terms.contains(*term))
                    .count() as f32
                    / query_terms.len().max(1) as f32;
                let phrase = words.len() > 1
                    && text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(words.len())
                        .any(|part| part == words);
                // Ricordi più recenti passano avanti a parità di pertinenza (solo ordinamento, mai la soglia).
                let age_days = chrono::DateTime::parse_from_rfc3339(&stamp)
                    .map(|t| (today - t.with_timezone(&chrono::Utc)).num_days().max(0) as f32)
                    .unwrap_or(0.0);
                recency.insert(id.clone(), 0.04 * 0.5f32.powf(age_days / 365.0));
                scores.insert(
                    id.clone(),
                    0.5 * cosines.get(&id).copied().unwrap_or(0.0)
                        + 0.3 * coverage
                        + if phrase { 0.2 } else { 0.0 },
                );
            }
        }
        for (rank, id) in fts_ids.iter().enumerate() {
            if let Some(score) = scores.get_mut(id) {
                *score += 0.3 / (1.0 + rank as f32 * 0.08);
            }
        }
        let mut ranked: Vec<_> = scores
            .into_iter()
            .filter(|(_, score)| *score >= MIN_SCORE)
            .collect();
        ranked.sort_by(|a, b| {
            let boost = |id: &String| recency.get(id).copied().unwrap_or(0.0);
            (b.1 + boost(&b.0))
                .total_cmp(&(a.1 + boost(&a.0)))
                .then_with(|| a.0.cmp(&b.0))
        });
        let sql = format!("SELECT {} FROM memory WHERE id = ?1", Self::COLUMNS);
        let mut out = Vec::new();
        for (id, score) in ranked.into_iter().take(limit) {
            if let Some(mut item) = self
                .conn
                .query_row(&sql, [&id], Self::memory_row)
                .optional()?
            {
                item["score"] = json!((score.min(1.0) * 1000.0).round() / 1000.0);
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

    #[test]
    fn search_prioritizes_phrases_and_finds_equivalent_words() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let at = "2026-01-01T00:00:00Z";
        store.memory_add("a", "la vettura è in garage", at).unwrap();
        store
            .memory_add("b", "sala riunioni secondo piano", at)
            .unwrap();
        store
            .memory_add("c", "secondo incontro al piano terra", at)
            .unwrap();
        assert_eq!(store.memory_search("auto", None, 5).unwrap()[0]["id"], "a");
        assert_eq!(
            store.memory_search("secondo piano", None, 5).unwrap()[0]["id"],
            "b"
        );
        assert!(store.memory_search("il e", None, 5).unwrap().is_empty());
    }

    #[test]
    fn search_matches_inflected_terms_and_keeps_exact_phrase_ranking() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let at = "2026-01-01T00:00:00Z";
        store
            .memory_add("meeting", "la riunione del progetto è importante", at)
            .unwrap();
        store
            .memory_add("unrelated", "il progetto viene archiviato", at)
            .unwrap();

        let results = store
            .memory_search("le riunioni del progetto", None, 5)
            .unwrap();
        assert_eq!(
            results.first().map(|item| item["id"].as_str()),
            Some(Some("meeting"))
        );
        assert!(results.iter().any(|item| item["id"] == "unrelated"));
    }

    #[test]
    fn broken_indexes_are_detected_and_rebuilt() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let at = "2026-01-01T00:00:00Z";
        store
            .memory_add("a", "il mio cane si chiama Fido", at)
            .unwrap();
        store.memory_add("b", "abito a Torino", at).unwrap();
        assert!(store.memory_consistent().unwrap());
        // Un arresto brusco può lasciare un indice senza l'elemento: lo si ricostruisce dalla sola tabella.
        store
            .conn
            .execute("DELETE FROM memory_fts WHERE id = 'b'", [])
            .unwrap();
        assert!(!store.memory_consistent().unwrap());
        store.memory_migrate().unwrap();
        assert!(store.memory_consistent().unwrap());
        assert_eq!(
            store.memory_search("Torino", None, 3).unwrap()[0]["id"],
            "b"
        );
        store
            .conn
            .execute(
                "INSERT INTO memory_vectors (id, vec) VALUES ('orfano', x'00')",
                [],
            )
            .unwrap();
        assert!(!store.memory_consistent().unwrap());
    }

    #[test]
    fn stats_and_duplicates_describe_the_memory_without_changing_it() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let at = "2026-01-01T00:00:00Z";
        store
            .memory_add("a", "il mio cane si chiama Fido", at)
            .unwrap();
        store
            .memory_add("b", "il mio cane si chiama Fido ed è un labrador", at)
            .unwrap();
        store
            .memory_add("c", "la riunione di progetto è alle nove", at)
            .unwrap();
        let stats = store.memory_stats("2026-01-10T00:00:00Z").unwrap();
        assert_eq!(stats["total"], 3);
        assert_eq!(stats["indexes_consistent"], true);
        assert_eq!(stats["by_kind"]["semantic"], 3);
        let pairs = store.memory_duplicates(0.8, 10).unwrap();
        assert_eq!(pairs.len(), 1, "{pairs:?}");
        let ids = [
            pairs[0]["a"]["id"].as_str().unwrap(),
            pairs[0]["b"]["id"].as_str().unwrap(),
        ];
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert_eq!(
            store.memory_list().unwrap().len(),
            3,
            "nessuna eliminazione"
        );
    }

    #[test]
    fn search_stays_correct_and_fast_with_thousands_of_memories() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let at = "2026-01-01T00:00:00Z";
        let tx = store.conn.unchecked_transaction().unwrap();
        for n in 0..3000 {
            store
                .memory_add(
                    &format!("m{n}"),
                    &format!("appunto numero {n} su argomento{} e cose varie", n % 97),
                    at,
                )
                .unwrap();
        }
        store
            .memory_add(
                "target",
                "la password del wifi di casa è nel cassetto della scrivania",
                at,
            )
            .unwrap();
        tx.commit().unwrap();
        let started = std::time::Instant::now();
        let found = store
            .memory_search("wifi di casa cassetto", None, 5)
            .unwrap();
        assert_eq!(found[0]["id"], "target");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn newer_memories_win_ties_but_never_beat_relevance() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store
            .memory_add(
                "old",
                "il codice dell'armadio è in cantina",
                "2020-01-01T00:00:00Z",
            )
            .unwrap();
        store
            .memory_add(
                "new",
                "il codice dell'armadio è in cantina",
                &chrono::Utc::now().to_rfc3339(),
            )
            .unwrap();
        store
            .memory_add(
                "other",
                "il garage ha un portone rosso",
                &chrono::Utc::now().to_rfc3339(),
            )
            .unwrap();
        let found = store
            .memory_search("codice armadio cantina", None, 5)
            .unwrap();
        assert_eq!(found[0]["id"], "new");
        assert!(found.iter().all(|m| m["id"] != "other"));
    }
}
