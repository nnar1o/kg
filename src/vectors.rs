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
