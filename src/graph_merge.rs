use std::collections::HashMap;

use anyhow::Result;

use crate::cli::MergeStrategy;
use crate::graph_lock;
use crate::storage::GraphStore;
use crate::append_event_snapshot;

pub(crate) fn merge_graphs(
    store: &dyn GraphStore,
    target: &str,
    source: &str,
    strategy: MergeStrategy,
) -> Result<String> {
    let target_path = store.resolve_graph_path(target)?;
    let _target_write_lock = graph_lock::acquire_for_graph(&target_path)?;
    let source_path = store.resolve_graph_path(source)?;
    let mut target_graph = store.load_graph(&target_path)?;
    let source_graph = store.load_graph(&source_path)?;

    let mut node_index: HashMap<String, usize> = HashMap::new();
    for (idx, node) in target_graph.nodes.iter().enumerate() {
        node_index.insert(node.id.clone(), idx);
    }

    let mut node_added = 0usize;
    let mut node_updated = 0usize;
    for node in &source_graph.nodes {
        if let Some(&idx) = node_index.get(&node.id) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.nodes[idx] = node.clone();
                node_updated += 1;
            }
        } else {
            target_graph.nodes.push(node.clone());
            node_index.insert(node.id.clone(), target_graph.nodes.len() - 1);
            node_added += 1;
        }
    }

    let mut edge_index: HashMap<String, usize> = HashMap::new();
    for (idx, edge) in target_graph.edges.iter().enumerate() {
        let key = format!("{} {} {}", edge.source_id, edge.relation, edge.target_id);
        edge_index.insert(key, idx);
    }

    let mut edge_added = 0usize;
    let mut edge_updated = 0usize;
    for edge in &source_graph.edges {
        let key = format!("{} {} {}", edge.source_id, edge.relation, edge.target_id);
        if let Some(&idx) = edge_index.get(&key) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.edges[idx] = edge.clone();
                edge_updated += 1;
            }
        } else {
            target_graph.edges.push(edge.clone());
            edge_index.insert(key, target_graph.edges.len() - 1);
            edge_added += 1;
        }
    }

    let mut note_index: HashMap<String, usize> = HashMap::new();
    for (idx, note) in target_graph.notes.iter().enumerate() {
        note_index.insert(note.id.clone(), idx);
    }

    let mut note_added = 0usize;
    let mut note_updated = 0usize;
    for note in &source_graph.notes {
        if let Some(&idx) = note_index.get(&note.id) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.notes[idx] = note.clone();
                note_updated += 1;
            }
        } else {
            target_graph.notes.push(note.clone());
            note_index.insert(note.id.clone(), target_graph.notes.len() - 1);
            note_added += 1;
        }
    }

    store.save_graph(&target_path, &target_graph)?;
    append_event_snapshot(
        &target_path,
        "graph.merge",
        Some(format!("{source} -> {target} ({strategy:?})")),
        &target_graph,
    )?;

    let mut lines = vec![format!("+ merged {source} -> {target}")];
    lines.push(format!("nodes: +{node_added} ~{node_updated}"));
    lines.push(format!("edges: +{edge_added} ~{edge_updated}"));
    lines.push(format!("notes: +{note_added} ~{note_updated}"));

    Ok(format!("{}\n", lines.join("\n")))
}
