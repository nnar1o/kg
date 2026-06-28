use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStore {
    pub version: String,
    pub dimension: usize,
    #[serde(default)]
    pub vectors: HashMap<String, Vec<f32>>,
}

impl VectorStore {
    pub fn new(dimension: usize) -> Self {
        Self {
            version: "1.0".to_string(),
            dimension,
            vectors: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("vector store not found: {}", path.display());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read vectors: {}", path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("invalid vectors JSON: {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let raw = serde_json::to_string_pretty(self).context("failed to serialize vectors")?;
        fs::write(path, raw).with_context(|| format!("failed to write vectors: {}", path.display()))
    }

    pub fn import_jsonl(path: &Path, graph_file: &crate::graph::GraphFile) -> Result<VectorStore> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read vectors: {}", path.display()))?;

        let mut vectors: HashMap<String, Vec<f32>> = HashMap::new();
        let mut dimension: Option<usize> = None;

        for (line_num, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            #[derive(Deserialize)]
            struct VectorEntry {
                id: String,
                vector: Vec<f32>,
            }

            let entry: VectorEntry = serde_json::from_str(line)
                .with_context(|| format!("failed to parse vector at line {}", line_num + 1))?;

            if entry.vector.is_empty() {
                anyhow::bail!("empty vector for id {} at line {}", entry.id, line_num + 1);
            }

            let dim = entry.vector.len();
            if let Some(expected) = dimension {
                if dim != expected {
                    anyhow::bail!(
                        "dimension mismatch at line {}: got {} expected {}",
                        line_num + 1,
                        dim,
                        expected
                    );
                }
            } else {
                dimension = Some(dim);
            }

            if graph_file.node_by_id(&entry.id).is_none() {
                anyhow::bail!("node not found: {} (line {})", entry.id, line_num + 1);
            }

            vectors.insert(entry.id, entry.vector);
        }

        let dim = dimension.unwrap_or(0);

        let mut store = Self::new(dim);
        store.vectors = vectors;
        Ok(store)
    }

    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    pub fn search(
        &self,
        query: &[f32],
        node_ids: &[String],
        limit: usize,
        min_score: f32,
    ) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = Vec::new();

        for (id, vector) in &self.vectors {
            if !node_ids.contains(id) {
                continue;
            }
            let score = Self::cosine_similarity(query, vector);
            if score >= min_score {
                results.push((id.clone(), score));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphFile, Node, NodeProperties};

    #[test]
    fn vector_store_new_creates_empty_store() {
        let store = VectorStore::new(128);
        assert_eq!(store.dimension, 128);
        assert!(store.vectors.is_empty());
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let score = VectorStore::cosine_similarity(&a, &a);
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let score = VectorStore::cosine_similarity(&a, &b);
        assert!((score - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_zero_norm_returns_zero() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 1.0];
        assert_eq!(VectorStore::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_different_lengths_returns_zero() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(VectorStore::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn search_filters_by_node_ids_and_min_score() {
        let mut store = VectorStore::new(2);
        store.vectors.insert("n:1".to_owned(), vec![1.0, 0.0]);
        store.vectors.insert("n:2".to_owned(), vec![0.0, 1.0]);
        store.vectors.insert("n:3".to_owned(), vec![1.0, 1.0]);
        let query = vec![1.0, 0.0];
        let results = store.search(&query, &["n:1".to_owned(), "n:2".to_owned()], 10, 0.5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n:1");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("kg_test_vectors");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("vectors.json");
        let mut store = VectorStore::new(3);
        store.vectors.insert("n:1".to_owned(), vec![1.0, 2.0, 3.0]);
        store.save(&path).unwrap();
        let loaded = VectorStore::load(&path).unwrap();
        assert_eq!(loaded.dimension, 3);
        assert!(loaded.vectors.contains_key("n:1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_jsonl_adds_vectors_from_file() {
        let dir = std::env::temp_dir().join("kg_test_import_jsonl");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("data.jsonl");
        std::fs::write(&path, r#"{"id":"n:1","vector":[0.1,0.2]}"#).unwrap();
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "n:1".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Test".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        let store = VectorStore::import_jsonl(&path, &graph).unwrap();
        assert_eq!(store.dimension, 2);
        assert!(store.vectors.contains_key("n:1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_error_for_missing_file() {
        let result = VectorStore::load(Path::new("/nonexistent/vectors.json"));
        assert!(result.is_err());
    }
}
