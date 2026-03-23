use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
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

const BACKUP_STALE_SECS: u64 = 60 * 60;

fn backup_graph_if_stale(path: &Path, data: &str) -> Result<()> {
    let parent = match path.parent() {
        Some(parent) => parent,
        None => return Ok(()),
    };
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => stem,
        None => return Ok(()),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("time went backwards")?
        .as_secs();
    if let Some(latest) = latest_backup_ts(parent, stem)? {
        if now.saturating_sub(latest) < BACKUP_STALE_SECS {
            return Ok(());
        }
    }

    let backup_path = parent.join(format!("{stem}.bck.{now}.gz"));
    let tmp_path = backup_path.with_extension("tmp");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_bytes())?;
    let encoded = encoder.finish()?;
    fs::write(&tmp_path, encoded)
        .with_context(|| format!("failed to write tmp: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &backup_path)
        .with_context(|| format!("failed to rename tmp to {}", backup_path.display()))?;
    Ok(())
}

fn latest_backup_ts(dir: &Path, stem: &str) -> Result<Option<u64>> {
    let prefix = format!("{stem}.bck.");
    let suffix = ".gz";
    let mut latest = None;
    for entry in fs::read_dir(dir).with_context(|| format!("read dir: {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) || !name.ends_with(suffix) {
            continue;
        }
        let ts_part = &name[prefix.len()..name.len() - suffix.len()];
        if let Ok(ts) = ts_part.parse::<u64>() {
            match latest {
                Some(current) => {
                    if ts > current {
                        latest = Some(ts);
                    }
                }
                None => latest = Some(ts),
            }
        }
    }
    Ok(latest)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFile {
    pub metadata: Metadata,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub notes: Vec<Note>,
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
    #[serde(default)]
    pub feedback_score: f64,
    #[serde(default)]
    pub feedback_count: u64,
    #[serde(default)]
    pub feedback_last_ts_ms: Option<u64>,
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
    #[serde(default)]
    pub feedback_score: f64,
    #[serde(default)]
    pub feedback_count: u64,
    #[serde(default)]
    pub feedback_last_ts_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub node_id: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub provenance: String,
    #[serde(default)]
    pub source_files: Vec<String>,
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
            notes: Vec::new(),
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
        atomic_write(path, &raw)?;
        backup_graph_if_stale(path, &raw)
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
