use crate::graph::{Edge, GraphFile, Node, Note};
use crate::storage::GraphStore;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

pub(crate) fn render_graph_diff(
    store: &dyn GraphStore,
    left: &str,
    right: &str,
) -> Result<String> {
    let left_path = store.resolve_graph_path(left)?;
    let right_path = store.resolve_graph_path(right)?;
    let left_graph = store.load_graph(&left_path)?;
    let right_graph = store.load_graph(&right_path)?;
    Ok(render_graph_diff_from_files(
        left,
        right,
        &left_graph,
        &right_graph,
    ))
}

pub(crate) fn render_graph_diff_json(
    store: &dyn GraphStore,
    left: &str,
    right: &str,
) -> Result<String> {
    let left_path = store.resolve_graph_path(left)?;
    let right_path = store.resolve_graph_path(right)?;
    let left_graph = store.load_graph(&left_path)?;
    let right_graph = store.load_graph(&right_path)?;
    Ok(render_graph_diff_json_from_files(
        left,
        right,
        &left_graph,
        &right_graph,
    ))
}

#[derive(Debug, Serialize)]
pub(crate) struct DiffEntry {
    path: String,
    left: Value,
    right: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct EntityDiff {
    id: String,
    diffs: Vec<DiffEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphDiffResponse {
    left: String,
    right: String,
    added_nodes: Vec<String>,
    removed_nodes: Vec<String>,
    changed_nodes: Vec<EntityDiff>,
    added_edges: Vec<String>,
    removed_edges: Vec<String>,
    changed_edges: Vec<EntityDiff>,
    added_notes: Vec<String>,
    removed_notes: Vec<String>,
    changed_notes: Vec<EntityDiff>,
}

pub(crate) fn render_graph_diff_json_from_files(
    left: &str,
    right: &str,
    left_graph: &GraphFile,
    right_graph: &GraphFile,
) -> String {
    use std::collections::{HashMap, HashSet};

    let left_nodes: HashSet<String> = left_graph.nodes.iter().map(|n| n.id.clone()).collect();
    let right_nodes: HashSet<String> = right_graph.nodes.iter().map(|n| n.id.clone()).collect();

    let left_node_map: HashMap<String, &Node> =
        left_graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_node_map: HashMap<String, &Node> = right_graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let left_edges: HashSet<String> = left_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();
    let right_edges: HashSet<String> = right_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();

    let left_edge_map: HashMap<String, &Edge> = left_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();
    let right_edge_map: HashMap<String, &Edge> = right_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();

    let left_notes: HashSet<String> = left_graph.notes.iter().map(|n| n.id.clone()).collect();
    let right_notes: HashSet<String> = right_graph.notes.iter().map(|n| n.id.clone()).collect();

    let left_note_map: HashMap<String, &Note> =
        left_graph.notes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_note_map: HashMap<String, &Note> = right_graph
        .notes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let mut added_nodes: Vec<String> = right_nodes.difference(&left_nodes).cloned().collect();
    let mut removed_nodes: Vec<String> = left_nodes.difference(&right_nodes).cloned().collect();
    let mut added_edges: Vec<String> = right_edges.difference(&left_edges).cloned().collect();
    let mut removed_edges: Vec<String> = left_edges.difference(&right_edges).cloned().collect();
    let mut added_notes: Vec<String> = right_notes.difference(&left_notes).cloned().collect();
    let mut removed_notes: Vec<String> = left_notes.difference(&right_notes).cloned().collect();

    let mut changed_nodes: Vec<String> = left_nodes
        .intersection(&right_nodes)
        .filter_map(|id| {
            let left_node = left_node_map.get(id.as_str())?;
            let right_node = right_node_map.get(id.as_str())?;
            if eq_serialized(*left_node, *right_node) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();
    let mut changed_edges: Vec<String> = left_edges
        .intersection(&right_edges)
        .filter_map(|key| {
            let left_edge = left_edge_map.get(key.as_str())?;
            let right_edge = right_edge_map.get(key.as_str())?;
            if eq_serialized(*left_edge, *right_edge) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect();
    let mut changed_notes: Vec<String> = left_notes
        .intersection(&right_notes)
        .filter_map(|id| {
            let left_note = left_note_map.get(id.as_str())?;
            let right_note = right_note_map.get(id.as_str())?;
            if eq_serialized(*left_note, *right_note) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    added_nodes.sort();
    removed_nodes.sort();
    added_edges.sort();
    removed_edges.sort();
    added_notes.sort();
    removed_notes.sort();
    changed_nodes.sort();
    changed_edges.sort();
    changed_notes.sort();

    let changed_nodes = changed_nodes
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_node_map
                .get(id.as_str())
                .zip(right_node_map.get(id.as_str()))
                .map(|(left_node, right_node)| diff_serialized_values_json(*left_node, *right_node))
                .unwrap_or_default(),
            id,
        })
        .collect();
    let changed_edges = changed_edges
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_edge_map
                .get(id.as_str())
                .zip(right_edge_map.get(id.as_str()))
                .map(|(left_edge, right_edge)| diff_serialized_values_json(*left_edge, *right_edge))
                .unwrap_or_default(),
            id,
        })
        .collect();
    let changed_notes = changed_notes
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_note_map
                .get(id.as_str())
                .zip(right_note_map.get(id.as_str()))
                .map(|(left_note, right_note)| diff_serialized_values_json(*left_note, *right_note))
                .unwrap_or_default(),
            id,
        })
        .collect();

    let payload = GraphDiffResponse {
        left: left.to_owned(),
        right: right.to_owned(),
        added_nodes,
        removed_nodes,
        changed_nodes,
        added_edges,
        removed_edges,
        changed_edges,
        added_notes,
        removed_notes,
        changed_notes,
    };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

pub(crate) fn render_graph_diff_from_files(
    left: &str,
    right: &str,
    left_graph: &GraphFile,
    right_graph: &GraphFile,
) -> String {
    use std::collections::{HashMap, HashSet};

    let left_nodes: HashSet<String> = left_graph.nodes.iter().map(|n| n.id.clone()).collect();
    let right_nodes: HashSet<String> = right_graph.nodes.iter().map(|n| n.id.clone()).collect();

    let left_node_map: HashMap<String, &Node> =
        left_graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_node_map: HashMap<String, &Node> = right_graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let left_edges: HashSet<String> = left_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();
    let right_edges: HashSet<String> = right_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();

    let left_edge_map: HashMap<String, &Edge> = left_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();
    let right_edge_map: HashMap<String, &Edge> = right_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();

    let left_notes: HashSet<String> = left_graph.notes.iter().map(|n| n.id.clone()).collect();
    let right_notes: HashSet<String> = right_graph.notes.iter().map(|n| n.id.clone()).collect();

    let left_note_map: HashMap<String, &Note> =
        left_graph.notes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_note_map: HashMap<String, &Note> = right_graph
        .notes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let mut added_nodes: Vec<String> = right_nodes.difference(&left_nodes).cloned().collect();
    let mut removed_nodes: Vec<String> = left_nodes.difference(&right_nodes).cloned().collect();
    let mut added_edges: Vec<String> = right_edges.difference(&left_edges).cloned().collect();
    let mut removed_edges: Vec<String> = left_edges.difference(&right_edges).cloned().collect();
    let mut added_notes: Vec<String> = right_notes.difference(&left_notes).cloned().collect();
    let mut removed_notes: Vec<String> = left_notes.difference(&right_notes).cloned().collect();

    let mut changed_nodes: Vec<String> = left_nodes
        .intersection(&right_nodes)
        .filter_map(|id| {
            let left_node = left_node_map.get(id.as_str())?;
            let right_node = right_node_map.get(id.as_str())?;
            if eq_serialized(*left_node, *right_node) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    let mut changed_edges: Vec<String> = left_edges
        .intersection(&right_edges)
        .filter_map(|key| {
            let left_edge = left_edge_map.get(key.as_str())?;
            let right_edge = right_edge_map.get(key.as_str())?;
            if eq_serialized(*left_edge, *right_edge) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect();

    let mut changed_notes: Vec<String> = left_notes
        .intersection(&right_notes)
        .filter_map(|id| {
            let left_note = left_note_map.get(id.as_str())?;
            let right_note = right_note_map.get(id.as_str())?;
            if eq_serialized(*left_note, *right_note) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    added_nodes.sort();
    removed_nodes.sort();
    added_edges.sort();
    removed_edges.sort();
    added_notes.sort();
    removed_notes.sort();
    changed_nodes.sort();
    changed_edges.sort();
    changed_notes.sort();

    let mut lines = vec![format!("= diff {left} -> {right}")];
    lines.push(format!("+ nodes ({})", added_nodes.len()));
    for id in added_nodes {
        lines.push(format!("+ node {id}"));
    }
    lines.push(format!("- nodes ({})", removed_nodes.len()));
    for id in removed_nodes {
        lines.push(format!("- node {id}"));
    }
    lines.push(format!("~ nodes ({})", changed_nodes.len()));
    for id in changed_nodes {
        if let (Some(left_node), Some(right_node)) = (
            left_node_map.get(id.as_str()),
            right_node_map.get(id.as_str()),
        ) {
            lines.extend(render_entity_diff_lines("node", &id, left_node, right_node));
        } else {
            lines.push(format!("~ node {id}"));
        }
    }
    lines.push(format!("+ edges ({})", added_edges.len()));
    for edge in added_edges {
        lines.push(format!("+ edge {edge}"));
    }
    lines.push(format!("- edges ({})", removed_edges.len()));
    for edge in removed_edges {
        lines.push(format!("- edge {edge}"));
    }
    lines.push(format!("~ edges ({})", changed_edges.len()));
    for edge in changed_edges {
        if let (Some(left_edge), Some(right_edge)) = (
            left_edge_map.get(edge.as_str()),
            right_edge_map.get(edge.as_str()),
        ) {
            lines.extend(render_entity_diff_lines(
                "edge", &edge, left_edge, right_edge,
            ));
        } else {
            lines.push(format!("~ edge {edge}"));
        }
    }
    lines.push(format!("+ notes ({})", added_notes.len()));
    for note_id in added_notes {
        lines.push(format!("+ note {note_id}"));
    }
    lines.push(format!("- notes ({})", removed_notes.len()));
    for note_id in removed_notes {
        lines.push(format!("- note {note_id}"));
    }
    lines.push(format!("~ notes ({})", changed_notes.len()));
    for note_id in changed_notes {
        if let (Some(left_note), Some(right_note)) = (
            left_note_map.get(note_id.as_str()),
            right_note_map.get(note_id.as_str()),
        ) {
            lines.extend(render_entity_diff_lines(
                "note", &note_id, left_note, right_note,
            ));
        } else {
            lines.push(format!("~ note {note_id}"));
        }
    }

    format!("{}\n", lines.join("\n"))
}

fn eq_serialized<T: Serialize>(left: &T, right: &T) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => left_value == right_value,
        _ => false,
    }
}

fn render_entity_diff_lines<T: Serialize>(
    kind: &str,
    id: &str,
    left: &T,
    right: &T,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("~ {kind} {id}"));
    for diff in diff_serialized_values(left, right) {
        lines.push(format!("  ~ {diff}"));
    }
    lines
}

fn diff_serialized_values<T: Serialize>(left: &T, right: &T) -> Vec<String> {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => {
            let mut diffs = Vec::new();
            collect_value_diffs("", &left_value, &right_value, &mut diffs);
            diffs
        }
        _ => vec!["<serialization failed>".to_owned()],
    }
}

fn diff_serialized_values_json<T: Serialize>(left: &T, right: &T) -> Vec<DiffEntry> {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => {
            let mut diffs = Vec::new();
            collect_value_diffs_json("", &left_value, &right_value, &mut diffs);
            diffs
        }
        _ => Vec::new(),
    }
}

fn collect_value_diffs_json(path: &str, left: &Value, right: &Value, out: &mut Vec<DiffEntry>) {
    if left == right {
        return;
    }
    match (left, right) {
        (Value::Object(left_obj), Value::Object(right_obj)) => {
            use std::collections::BTreeSet;

            let mut keys: BTreeSet<&str> = BTreeSet::new();
            for key in left_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in right_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in keys {
                let left_value = left_obj.get(key).unwrap_or(&Value::Null);
                let right_value = right_obj.get(key).unwrap_or(&Value::Null);
                let next_path = if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                collect_value_diffs_json(&next_path, left_value, right_value, out);
            }
        }
        (Value::Array(_), Value::Array(_)) => {
            let label = if path.is_empty() {
                "<root>[]".to_owned()
            } else {
                format!("{path}[]")
            };
            out.push(DiffEntry {
                path: label,
                left: left.clone(),
                right: right.clone(),
            });
        }
        _ => {
            let label = if path.is_empty() { "<root>" } else { path };
            out.push(DiffEntry {
                path: label.to_owned(),
                left: left.clone(),
                right: right.clone(),
            });
        }
    }
}

fn collect_value_diffs(path: &str, left: &Value, right: &Value, out: &mut Vec<String>) {
    if left == right {
        return;
    }
    match (left, right) {
        (Value::Object(left_obj), Value::Object(right_obj)) => {
            use std::collections::BTreeSet;

            let mut keys: BTreeSet<&str> = BTreeSet::new();
            for key in left_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in right_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in keys {
                let left_value = left_obj.get(key).unwrap_or(&Value::Null);
                let right_value = right_obj.get(key).unwrap_or(&Value::Null);
                let next_path = if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                collect_value_diffs(&next_path, left_value, right_value, out);
            }
        }
        (Value::Array(_), Value::Array(_)) => {
            let label = if path.is_empty() {
                "<root>[]".to_owned()
            } else {
                format!("{path}[]")
            };
            out.push(format!(
                "{label}: {} -> {}",
                format_value(left),
                format_value(right)
            ));
        }
        _ => {
            let label = if path.is_empty() { "<root>" } else { path };
            out.push(format!(
                "{label}: {} -> {}",
                format_value(left),
                format_value(right)
            ));
        }
    }
}

fn format_value(value: &Value) -> String {
    let mut rendered =
        serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_owned());
    rendered = rendered.replace('\n', "\\n");
    truncate_value(rendered, 160)
}

fn truncate_value(mut value: String, limit: usize) -> String {
    if value.len() <= limit {
        return value;
    }
    value.truncate(limit.saturating_sub(3));
    value.push_str("...");
    value
}
