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

fn read_csv_records(
    path: &str,
    entity: &str,
) -> Result<(csv::StringRecord, Vec<(usize, csv::StringRecord)>)> {
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open {} CSV: {path}", entity))?;
    let headers = reader.headers()?.clone();
    let records = reader
        .records()
        .enumerate()
        .map(|(idx, r)| {
            let record =
                r.with_context(|| format!("failed to read {} CSV row {}", entity, idx + 1))?;
            Ok((idx, record))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((headers, records))
}

fn read_nodes_csv(path: &str) -> Result<Vec<Node>> {
    let (headers, records) = read_csv_records(path, "nodes")?;
    let mut nodes = Vec::new();
    for (idx, record) in records {
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
    let (headers, records) = read_csv_records(path, "edges")?;
    let mut edges = Vec::new();
    for (idx, record) in records {
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
    let (headers, records) = read_csv_records(path, "notes")?;
    let mut notes = Vec::new();
    for (idx, record) in records {
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

macro_rules! merge_items {
    ($graph:expr, $items:expr, $strategy:expr, $summary:expr,
     $storage:ident,
     $added:ident, $updated:ident, $skipped:ident,
     $entity:expr,
     $find:expr,
     $key_of:expr,
     $validate:expr) =>
    {
        let prefer_new = $strategy.is_prefer_new();
        for item in $items {
            let key = $key_of(&item);
            let idx = $find(&item);
            if let Some(idx) = idx {
                if prefer_new {
                    $graph.$storage[idx] = item;
                    $summary.$updated += 1;
                } else {
                    if differs(&$graph.$storage[idx], &item) {
                        $summary.conflicts.push(format!("{} {}",
                            $entity, key));
                    }
                    $summary.$skipped += 1;
                }
            } else if $validate(&item) {
                $graph.$storage.push(item);
                $summary.$added += 1;
            } else {
                $summary.$skipped += 1;
            }
        }
    };
}

fn merge_nodes(
    graph: &mut GraphFile,
    nodes: Vec<Node>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    merge_items!(graph, nodes, strategy, summary,
        nodes, nodes_added, nodes_updated, nodes_skipped, "node",
        |item: &Node| graph.nodes.iter().position(|n| n.id == item.id),
        |item: &Node| item.id.clone(),
        |_: &Node| true);
    Ok(())
}

fn merge_edges(
    graph: &mut GraphFile,
    edges: Vec<Edge>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    merge_items!(graph, edges, strategy, summary,
        edges, edges_added, edges_updated, edges_skipped, "edge",
        |item: &Edge| graph.edges.iter().position(|e| {
            e.source_id == item.source_id
                && e.relation == item.relation
                && e.target_id == item.target_id
        }),
        |item: &Edge| format!("{} {} {}", item.source_id, item.relation, item.target_id),
        |item: &Edge| graph.node_by_id(&item.source_id).is_some()
            && graph.node_by_id(&item.target_id).is_some());
    Ok(())
}

fn merge_notes(
    graph: &mut GraphFile,
    notes: Vec<Note>,
    strategy: CsvStrategy,
    summary: &mut CsvImportSummary,
) -> Result<()> {
    merge_items!(graph, notes, strategy, summary,
        notes, notes_added, notes_updated, notes_skipped, "note",
        |item: &Note| graph.notes.iter().position(|n| n.id == item.id),
        |item: &Note| item.id.clone(),
        |item: &Note| graph.node_by_id(&item.node_id).is_some());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn import_nodes_csv_adds_nodes() {
        let dir = std::env::temp_dir().join("kg_test_csv_nodes");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("nodes.csv");
        fs::write(&path, "id,type,name,description\nn:1,Concept,Test,desc\n").unwrap();
        let mut graph = GraphFile::new("test");
        let args = CsvImportArgs {
            nodes_path: Some(path.to_str().unwrap()),
            edges_path: None,
            notes_path: None,
            strategy: CsvStrategy::PreferNew,
        };
        let summary = import_csv_into_graph(&mut graph, args).unwrap();
        assert_eq!(summary.nodes_added, 1);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "n:1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_edges_csv_adds_edges() {
        let dir = std::env::temp_dir().join("kg_test_csv_edges");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("edges.csv");
        fs::write(&path, "source_id,relation,target_id\nn:a,GRELATES,n:b\n").unwrap();
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "n:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "A".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.nodes.push(Node {
            id: "n:b".to_owned(),
            r#type: "Concept".to_owned(),
            name: "B".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        let args = CsvImportArgs {
            nodes_path: None,
            edges_path: Some(path.to_str().unwrap()),
            notes_path: None,
            strategy: CsvStrategy::PreferNew,
        };
        let summary = import_csv_into_graph(&mut graph, args).unwrap();
        assert_eq!(summary.edges_added, 1);
        assert_eq!(graph.edges.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_notes_csv_adds_notes() {
        let dir = std::env::temp_dir().join("kg_test_csv_notes");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("notes.csv");
        fs::write(&path, "id,node_id,body\nnote:1,n:a,hello\n").unwrap();
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "n:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "A".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        let args = CsvImportArgs {
            nodes_path: None,
            edges_path: None,
            notes_path: Some(path.to_str().unwrap()),
            strategy: CsvStrategy::PreferNew,
        };
        let summary = import_csv_into_graph(&mut graph, args).unwrap();
        assert_eq!(summary.notes_added, 1);
        assert_eq!(graph.notes.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_csv_skips_edges_with_missing_nodes() {
        let dir = std::env::temp_dir().join("kg_test_csv_skip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("edges.csv");
        fs::write(&path, "source_id,relation,target_id\nn:x,GRELATES,n:y\n").unwrap();
        let mut graph = GraphFile::new("test");
        let args = CsvImportArgs {
            nodes_path: None,
            edges_path: Some(path.to_str().unwrap()),
            notes_path: None,
            strategy: CsvStrategy::PreferNew,
        };
        let summary = import_csv_into_graph(&mut graph, args).unwrap();
        assert_eq!(summary.edges_added, 0);
        assert_eq!(summary.edges_skipped, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_summary_lines_formats_correctly() {
        let summary = CsvImportSummary {
            nodes_added: 2,
            nodes_updated: 1,
            nodes_skipped: 0,
            ..Default::default()
        };
        let lines = merge_summary_lines(&summary);
        assert!(lines[0].contains("+2 ~1 !0"));
    }

    #[test]
    fn import_csv_rejects_missing_columns() {
        let dir = std::env::temp_dir().join("kg_test_csv_bad");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("nodes.csv");
        fs::write(&path, "bad_column,other\nn:1,val\n").unwrap();
        let mut graph = GraphFile::new("test");
        let args = CsvImportArgs {
            nodes_path: Some(path.to_str().unwrap()),
            edges_path: None,
            notes_path: None,
            strategy: CsvStrategy::PreferNew,
        };
        let result = import_csv_into_graph(&mut graph, args);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_nodes_csv_requires_non_empty_id() {
        let dir = std::env::temp_dir().join("kg_test_csv_empty_id");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("nodes.csv");
        fs::write(&path, "id,type,name\n,Concept,Test\n").unwrap();
        let mut graph = GraphFile::new("test");
        let args = CsvImportArgs {
            nodes_path: Some(path.to_str().unwrap()),
            edges_path: None,
            notes_path: None,
            strategy: CsvStrategy::PreferNew,
        };
        let result = import_csv_into_graph(&mut graph, args);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn differs_detects_different_values() {
        assert!(differs(&"hello", &"world"));
        assert!(!differs(&42u32, &42u32));
    }
}
