#![allow(clippy::new_without_default)]

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use redb::{Database, ReadableTable};

use crate::graph::{GraphFile, Node};

const REDB_TERMS_TABLE: redb::TableDefinition<&str, &[u8]> =
    redb::TableDefinition::new("bm25_terms");
const REDB_META_TABLE: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("bm25_meta");

#[derive(Debug, Clone)]
pub struct Bm25Index {
    pub avg_doc_len: f32,
    pub doc_count: usize,
    pub k1: f32,
    pub b: f32,
    pub idf: HashMap<String, f32>,
    pub term_to_docs: HashMap<String, HashSet<String>>,
}

impl Bm25Index {
    pub fn new() -> Self {
        Self {
            avg_doc_len: 0.0,
            doc_count: 0,
            k1: 1.5,
            b: 0.75,
            idf: HashMap::new(),
            term_to_docs: HashMap::new(),
        }
    }

    pub fn build(graph: &GraphFile) -> Self {
        let mut index = Self::new();
        let mut doc_lengths: Vec<usize> = Vec::new();

        for node in &graph.nodes {
            let terms = extract_terms(node);
            let doc_len = terms.len();
            doc_lengths.push(doc_len);

            let mut doc_terms: HashSet<String> = HashSet::new();
            for term in terms {
                doc_terms.insert(term.clone());
                index
                    .term_to_docs
                    .entry(term)
                    .or_default()
                    .insert(node.id.clone());
            }
        }

        index.doc_count = graph.nodes.len();
        if !doc_lengths.is_empty() {
            index.avg_doc_len = doc_lengths.iter().sum::<usize>() as f32 / doc_lengths.len() as f32;
        }

        let num_docs = index.doc_count.max(1) as f32;
        for (term, docs) in &index.term_to_docs {
            let doc_freq = docs.len() as f32;
            index.idf.insert(
                term.clone(),
                ((num_docs - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln(),
            );
        }

        index
    }

    pub fn save(&self, db_path: &Path) -> Result<()> {
        let db = open_index_db(db_path)?;
        let write_txn = db
            .begin_write()
            .context("failed to start write transaction")?;
        {
            let mut terms_table = write_txn.open_table(REDB_TERMS_TABLE)?;
            for (term, doc_ids) in &self.term_to_docs {
                let doc_ids_json =
                    serde_json::to_string(doc_ids).context("failed to serialize doc ids")?;
                terms_table.insert(term.as_str(), doc_ids_json.as_bytes())?;
            }
        }
        {
            let mut meta_table = write_txn.open_table(REDB_META_TABLE)?;
            meta_table.insert("avg_doc_len", self.avg_doc_len.to_string().as_bytes())?;
            meta_table.insert("doc_count", self.doc_count.to_string().as_bytes())?;
            meta_table.insert("k1", self.k1.to_string().as_bytes())?;
            meta_table.insert("b", self.b.to_string().as_bytes())?;
            let idf_json = serde_json::to_string(&self.idf).context("failed to serialize idf")?;
            meta_table.insert("idf", idf_json.as_bytes())?;
        }
        write_txn.commit().context("failed to commit index")?;
        Ok(())
    }

    pub fn load(db_path: &Path) -> Result<Self> {
        let db = open_index_db(db_path)?;
        let read_txn = db
            .begin_read()
            .context("failed to start read transaction")?;

        let avg_doc_len = read_txn
            .open_table(REDB_META_TABLE)?
            .get("avg_doc_len")?
            .map(|v| {
                std::str::from_utf8(v.value())
                    .unwrap_or("0")
                    .parse::<f32>()
                    .unwrap_or(0.0)
            })
            .unwrap_or(0.0);

        let doc_count = read_txn
            .open_table(REDB_META_TABLE)?
            .get("doc_count")?
            .map(|v| {
                std::str::from_utf8(v.value())
                    .unwrap_or("0")
                    .parse::<usize>()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        let k1 = read_txn
            .open_table(REDB_META_TABLE)?
            .get("k1")?
            .map(|v| {
                std::str::from_utf8(v.value())
                    .unwrap_or("1.5")
                    .parse::<f32>()
                    .unwrap_or(1.5)
            })
            .unwrap_or(1.5);

        let b = read_txn
            .open_table(REDB_META_TABLE)?
            .get("b")?
            .map(|v| {
                std::str::from_utf8(v.value())
                    .unwrap_or("0.75")
                    .parse::<f32>()
                    .unwrap_or(0.75)
            })
            .unwrap_or(0.75);

        let idf_json = read_txn
            .open_table(REDB_META_TABLE)?
            .get("idf")?
            .map(|v| -> String { String::from_utf8_lossy(v.value()).into_owned() })
            .unwrap_or_else(|| "{}".to_string());
        let idf: HashMap<String, f32> = serde_json::from_str(&idf_json).unwrap_or_default();

        let mut term_to_docs: HashMap<String, HashSet<String>> = HashMap::new();
        let terms_table = read_txn.open_table(REDB_TERMS_TABLE)?;
        let entries: Vec<_> = terms_table.iter()?.collect();
        for entry in entries {
            let entry = entry?;
            let term_str = entry.0.value();
            let doc_ids_str = std::str::from_utf8(entry.1.value())?;
            let doc_ids: HashSet<String> = serde_json::from_str(doc_ids_str).unwrap_or_default();
            term_to_docs.insert(term_str.to_string(), doc_ids);
        }

        Ok(Self {
            avg_doc_len,
            doc_count,
            k1,
            b,
            idf,
            term_to_docs,
        })
    }

    pub fn search(&self, query_terms: &[String], graph: &GraphFile) -> Vec<(String, f32)> {
        if query_terms.is_empty() || self.doc_count == 0 {
            return Vec::new();
        }

        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in query_terms {
            let idf = self.idf.get(term).copied().unwrap_or(0.0);
            if idf <= 0.0 {
                continue;
            }

            if let Some(doc_ids) = self.term_to_docs.get(term) {
                for doc_id in doc_ids {
                    if let Some(node) = graph.node_by_id(doc_id) {
                        let terms = extract_terms(node);
                        let doc_len = terms.len() as f32;
                        let tf = terms.iter().filter(|t| *t == term).count() as f32;

                        let numerator = idf * tf * (self.k1 + 1.0);
                        let denominator =
                            tf + self.k1 * (1.0 - self.b + self.b * doc_len / self.avg_doc_len);

                        let score = if denominator > 0.0 {
                            numerator / denominator
                        } else {
                            0.0
                        };

                        *scores.entry(doc_id.clone()).or_insert(0.0) += score;
                    }
                }
            }
        }

        let mut results: Vec<(String, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

fn open_index_db(db_path: &Path) -> Result<Database> {
    if db_path.exists() {
        Database::open(db_path)
            .with_context(|| format!("failed to open index db: {}", db_path.display()))
    } else {
        Database::create(db_path)
            .with_context(|| format!("failed to create index db: {}", db_path.display()))
    }
}

fn extract_terms(node: &Node) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();

    for word in node.id.split(|c: char| !c.is_alphanumeric()) {
        if word.len() > 2 {
            terms.push(word.to_lowercase());
        }
    }

    for word in node.name.split(|c: char| !c.is_alphanumeric()) {
        if word.len() > 2 {
            terms.push(word.to_lowercase());
        }
    }

    for word in node
        .properties
        .description
        .split(|c: char| !c.is_alphanumeric())
    {
        if word.len() > 2 {
            terms.push(word.to_lowercase());
        }
    }

    for alias in &node.properties.alias {
        for word in alias.split(|c: char| !c.is_alphanumeric()) {
            if word.len() > 2 {
                terms.push(word.to_lowercase());
            }
        }
    }

    for fact in &node.properties.key_facts {
        for word in fact.split(|c: char| !c.is_alphanumeric()) {
            if word.len() > 2 {
                terms.push(word.to_lowercase());
            }
        }
    }

    terms.sort();
    terms.dedup();
    terms
}
