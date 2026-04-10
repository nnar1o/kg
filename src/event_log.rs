use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::graph::GraphFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub ts_ms: u64,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub graph: GraphFile,
}

impl EventLogEntry {
    pub fn new(action: &str, detail: Option<String>, graph: GraphFile) -> Result<Self> {
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("time went backwards")?
            .as_millis() as u64;
        Ok(Self {
            ts_ms,
            action: action.to_owned(),
            detail,
            graph,
        })
    }
}

pub fn event_log_path(graph_path: &Path) -> PathBuf {
    let stem = graph_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    let ext = graph_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("json");
    crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.{ext}.event.log"))
}

fn legacy_event_log_path(graph_path: &Path) -> PathBuf {
    let mut path = graph_path.to_path_buf();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("graph");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
    path.set_file_name(format!("{stem}.{ext}.event.log"));
    path
}

fn first_existing_event_log_path(graph_path: &Path) -> PathBuf {
    let preferred = event_log_path(graph_path);
    if preferred.exists() {
        return preferred;
    }
    let legacy = legacy_event_log_path(graph_path);
    if legacy.exists() {
        return legacy;
    }
    preferred
}

pub fn has_log(graph_path: &Path) -> bool {
    event_log_path(graph_path).exists() || legacy_event_log_path(graph_path).exists()
}

pub fn append_snapshot(
    graph_path: &Path,
    action: &str,
    detail: Option<String>,
    graph: &GraphFile,
) -> Result<()> {
    let mut snapshot = graph.clone();
    snapshot.refresh_counts();
    let entry = EventLogEntry::new(action, detail, snapshot)?;
    let log_path = event_log_path(graph_path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let line = serde_json::to_string(&entry).context("failed to serialize event log entry")?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn read_log(graph_path: &Path) -> Result<Vec<EventLogEntry>> {
    let log_path = first_existing_event_log_path(graph_path);
    if !log_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&log_path)?;
    let mut entries = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: EventLogEntry = serde_json::from_str(line)
            .with_context(|| format!("invalid event log entry at line {}", idx + 1))?;
        entries.push(entry);
    }
    Ok(entries)
}
