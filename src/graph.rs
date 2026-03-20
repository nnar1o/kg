use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Write `data` to `dest` atomically:
/// 1. Write to `dest.tmp`
/// 2. If `dest` already exists, copy it to `dest.bak`
/// 3. Rename `dest.tmp` -> `dest`
fn atomic_write(dest: &Path, data: &str) -> Result<()> {
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, data).with_context(|| format!("failed to write tmp: {}", tmp.display()))?;
    if dest.exists() {
        let bak = dest.with_extension("bak");
        fs::copy(dest, &bak)
            .with_context(|| format!("failed to create backup: {}", bak.display()))?;
    }
    fs::rename(&tmp, dest).with_context(|| format!("failed to rename tmp to {}", dest.display()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFile {
    pub metadata: Metadata,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub name: String,
    #[serde(default)]
    pub properties: NodeProperties,
    #[serde(default)]
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeProperties {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub domain_area: String,
    #[serde(default)]
    pub provenance: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub alias: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
    #[serde(default)]
    pub properties: EdgeProperties,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgeProperties {
    #[serde(default)]
    pub detail: String,
}

impl GraphFile {
    pub fn new(name: &str) -> Self {
        Self {
            metadata: Metadata {
                name: name.to_owned(),
                version: "1.0".to_owned(),
                description: format!("Knowledge graph: {name}"),
                node_count: 0,
                edge_count: 0,
            },
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read graph: {}", path.display()))?;
        let mut graph: GraphFile = serde_json::from_str(&raw)
            .with_context(|| format!("invalid JSON: {}", path.display()))?;
        graph.refresh_counts();
        Ok(graph)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut graph = self.clone();
        graph.refresh_counts();
        let raw = serde_json::to_string_pretty(&graph).context("failed to serialize graph")?;
        atomic_write(path, &raw)
    }

    pub fn refresh_counts(&mut self) {
        self.metadata.node_count = self.nodes.len();
        self.metadata.edge_count = self.edges.len();
    }

    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_by_id_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn has_edge(&self, source_id: &str, relation: &str, target_id: &str) -> bool {
        self.edges.iter().any(|edge| {
            edge.source_id == source_id && edge.relation == relation && edge.target_id == target_id
        })
    }
}
