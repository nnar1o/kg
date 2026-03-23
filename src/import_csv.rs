use anyhow::{Context, Result, bail};
use csv::ReaderBuilder;
use serde::Serialize;

use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties, Note};

#[derive(Debug, Clone, Copy)]
pub enum CsvStrategy {
    PreferNew,
    PreferOld,
}

impl CsvStrategy {
    pub fn is_prefer_new(self) -> bool {
        matches!(self, CsvStrategy::PreferNew)
    }
}

#[derive(Debug, Default)]
pub struct CsvImportSummary {
    pub nodes_added: usize,
    pub nodes_updated: usize,
    pub nodes_skipped: usize,
    pub edges_added: usize,
    pub edges_updated: usize,
    pub edges_skipped: usize,
    pub notes_added: usize,
    pub notes_updated: usize,
    pub notes_skipped: usize,
    pub conflicts: Vec<String>,
}

pub struct CsvImportArgs<'a> {
    pub nodes_path: Option<&'a str>,
    pub edges_path: Option<&'a str>,
    pub notes_path: Option<&'a str>,
    pub strategy: CsvStrategy,
}

pub fn import_csv_into_graph(
    graph: &mut GraphFile,
    args: CsvImportArgs<'_>,
) -> Result<CsvImportSummary> {
    let mut summary = CsvImportSummary::default();
    if let Some(path) = args.nodes_path {
        let nodes = read_nodes_csv(path)?;
        merge_nodes(graph, nodes, args.strategy, &mut summary)?;
    }
    if let Some(path) = args.edges_path {
        let edges = read_edges_csv(path)?;
        merge_edges(graph, edges, args.strategy, &mut summary)?;
    }
    if let Some(path) = args.notes_path {
        let notes = read_notes_csv(path)?;
        merge_notes(graph, notes, args.strategy, &mut summary)?;
    }
    graph.refresh_counts();
    Ok(summary)
}

fn read_nodes_csv(path: &str) -> Result<Vec<Node>> {
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open nodes CSV: {path}"))?;
    let headers = reader.headers()?.clone();
    let mut nodes = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("failed to read nodes CSV row {}", idx + 1))?;
        let mut node = Node {
            id: field(&headers, &record, "id")?,
            r#type: field(&headers, &record, "type")?,
            name: field(&headers, &record, "name")?,
            properties: NodeProperties::default(),
            source_files: list_field(&headers, &record, "source_files"),
        };
        if node.id.is_empty() {
            bail!("nodes CSV row {} missing id", idx + 1);
        }
        if node.r#type.is_empty() {
            bail!("nodes CSV row {} missing type", idx + 1);
        }
        if node.name.is_empty() {
            bail!("nodes CSV row {} missing name", idx + 1);
        }
        node.properties.description = optional_field(&headers, &record, "description");
        node.properties.domain_area = optional_field(&headers, &record, "domain_area");
        node.properties.provenance = optional_field(&headers, &record, "provenance");
        node.properties.created_at = optional_field(&headers, &record, "created_at");
        node.properties.confidence = optional_field(&headers, &record, "confidence")
            .parse::<f64>()
            .ok();
        node.properties.key_facts = list_field(&headers, &record, "key_facts");
        node.properties.alias = list_field(&headers, &record, "alias");
        nodes.push(node);
    }
    Ok(nodes)
}

fn read_edges_csv(path: &str) -> Result<Vec<Edge>> {
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open edges CSV: {path}"))?;
    let headers = reader.headers()?.clone();
    let mut edges = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("failed to read edges CSV row {}", idx + 1))?;
        let source_id = field(&headers, &record, "source_id")?;
        let relation = field(&headers, &record, "relation")?;
        let target_id = field(&headers, &record, "target_id")?;
        if source_id.is_empty() || relation.is_empty() || target_id.is_empty() {
            bail!("edges CSV row {} missing required fields", idx + 1);
        }
        let detail = optional_field(&headers, &record, "detail");
        edges.push(Edge {
            source_id,
            relation,
            target_id,
            properties: EdgeProperties {
                detail,
                ..EdgeProperties::default()
            },
        });
    }
    Ok(edges)
}

fn read_notes_csv(path: &str) -> Result<Vec<Note>> {
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open notes CSV: {path}"))?;
    let headers = reader.headers()?.clone();
    let mut notes = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("failed to read notes CSV row {}", idx + 1))?;
        let id = field(&headers, &record, "id")?;
        let node_id = field(&headers, &record, "node_id")?;
        if id.is_empty() || node_id.is_empty() {
            bail!("notes CSV row {} missing id or node_id", idx + 1);
        }
        let note = Note {
            id,
            node_id,
            body: optional_field(&headers, &record, "body"),
            tags: list_field(&headers, &record, "tags"),
            author: optional_field(&headers, &record, "author"),
            created_at: optional_field(&headers, &record, "created_at"),
            provenance: optional_field(&headers, &record, "provenance"),
            source_files: list_field(&headers, &record, "source_files"),
        };
        notes.push(note);
    }
    Ok(notes)
}

fn merge_nodes(
    graph: &mut GraphFile,
    nodes: Vec<Node>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    let prefer_new = strategy.is_prefer_new();
    for node in nodes {
        if let Some(existing) = graph.node_by_id_mut(&node.id) {
            if prefer_new {
                *existing = node;
                summary.nodes_updated += 1;
            } else {
                if differs(existing, &node) {
                    summary.conflicts.push(format!("node {}", node.id));
                }
                summary.nodes_skipped += 1;
            }
        } else {
            graph.nodes.push(node);
            summary.nodes_added += 1;
        }
    }
    Ok(())
}

fn merge_edges(
    graph: &mut GraphFile,
    edges: Vec<Edge>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    let prefer_new = strategy.is_prefer_new();
    for edge in edges {
        let key = format!("{} {} {}", edge.source_id, edge.relation, edge.target_id);
        let pos = graph
            .edges
            .iter()
            .position(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id) == key);
        if let Some(idx) = pos {
            if prefer_new {
                graph.edges[idx] = edge;
                summary.edges_updated += 1;
            } else {
                if differs(&graph.edges[idx], &edge) {
                    summary.conflicts.push(format!("edge {key}"));
                }
                summary.edges_skipped += 1;
            }
        } else {
            if graph.node_by_id(&edge.source_id).is_none()
                || graph.node_by_id(&edge.target_id).is_none()
            {
                summary.edges_skipped += 1;
                continue;
            }
            graph.edges.push(edge);
            summary.edges_added += 1;
        }
    }
    Ok(())
}

fn merge_notes(
    graph: &mut GraphFile,
    notes: Vec<Note>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    let prefer_new = strategy.is_prefer_new();
    for note in notes {
        let pos = graph.notes.iter().position(|n| n.id == note.id);
        if let Some(idx) = pos {
            if prefer_new {
                graph.notes[idx] = note;
                summary.notes_updated += 1;
            } else {
                if differs(&graph.notes[idx], &note) {
                    summary.conflicts.push(format!("note {}", note.id));
                }
                summary.notes_skipped += 1;
            }
        } else {
            if graph.node_by_id(&note.node_id).is_none() {
                summary.notes_skipped += 1;
                continue;
            }
            graph.notes.push(note);
            summary.notes_added += 1;
        }
    }
    Ok(())
}

fn field(headers: &csv::StringRecord, record: &csv::StringRecord, name: &str) -> Result<String> {
    let idx = headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| anyhow::anyhow!("missing column: {name}"))?;
    Ok(record.get(idx).unwrap_or_default().trim().to_owned())
}

fn optional_field(headers: &csv::StringRecord, record: &csv::StringRecord, name: &str) -> String {
    let Some(idx) = headers.iter().position(|h| h == name) else {
        return String::new();
    };
    record.get(idx).unwrap_or_default().trim().to_owned()
}

fn list_field(headers: &csv::StringRecord, record: &csv::StringRecord, name: &str) -> Vec<String> {
    let raw = optional_field(headers, record, name);
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split('|')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_owned())
        .collect()
}

pub fn merge_summary_lines(summary: &CsvImportSummary) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "nodes: +{} ~{} !{}",
        summary.nodes_added, summary.nodes_updated, summary.nodes_skipped
    ));
    lines.push(format!(
        "edges: +{} ~{} !{}",
        summary.edges_added, summary.edges_updated, summary.edges_skipped
    ));
    lines.push(format!(
        "notes: +{} ~{} !{}",
        summary.notes_added, summary.notes_updated, summary.notes_skipped
    ));
    if !summary.conflicts.is_empty() {
        lines.push(format!("conflicts: {}", summary.conflicts.len()));
        for conflict in summary.conflicts.iter().take(10) {
            lines.push(format!("! {conflict}"));
        }
        if summary.conflicts.len() > 10 {
            lines.push("! ...".to_owned());
        }
        lines.push("suggestion: re-run with --strategy prefer-new or resolve manually".to_owned());
    }
    lines
}

fn differs<T: Serialize>(left: &T, right: &T) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(l), Ok(r)) => l != r,
        _ => true,
    }
}
