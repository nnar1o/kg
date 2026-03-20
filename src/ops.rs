use anyhow::{anyhow, bail, Result};

use crate::graph::{Edge, GraphFile, Node};

// ---------------------------------------------------------------------------
// Node mutations
// ---------------------------------------------------------------------------

pub fn add_node(graph: &mut GraphFile, node: Node) -> Result<()> {
    if graph.node_by_id(&node.id).is_some() {
        bail!("node already exists: {}", node.id);
    }
    if node.source_files.is_empty() {
        bail!("at least one --source is required");
    }
    graph.nodes.push(node);
    graph.refresh_counts();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn modify_node(
    graph: &mut GraphFile,
    id: &str,
    node_type: Option<String>,
    name: Option<String>,
    description: Option<String>,
    domain_area: Option<String>,
    provenance: Option<String>,
    confidence: Option<f64>,
    created_at: Option<String>,
    facts: Vec<String>,
    aliases: Vec<String>,
    sources: Vec<String>,
) -> Result<()> {
    let node = graph
        .node_by_id_mut(id)
        .ok_or_else(|| anyhow!("node not found: {id}"))?;

    if let Some(v) = node_type {
        node.r#type = v;
    }
    if let Some(v) = name {
        node.name = v;
    }
    if let Some(v) = description {
        node.properties.description = v;
    }
    if let Some(v) = domain_area {
        node.properties.domain_area = v;
    }
    if let Some(v) = provenance {
        node.properties.provenance = v;
    }
    if let Some(v) = confidence {
        node.properties.confidence = Some(v);
    }
    if let Some(v) = created_at {
        node.properties.created_at = v;
    }
    for fact in facts {
        push_unique(&mut node.properties.key_facts, fact);
    }
    for alias in aliases {
        push_unique(&mut node.properties.alias, alias);
    }
    for source in sources {
        push_unique(&mut node.source_files, source);
    }

    graph.refresh_counts();
    Ok(())
}

pub fn remove_node(graph: &mut GraphFile, id: &str) -> Result<usize> {
    let before_nodes = graph.nodes.len();
    graph.nodes.retain(|node| node.id != id);
    if before_nodes == graph.nodes.len() {
        bail!("node not found: {id}");
    }
    let before_edges = graph.edges.len();
    graph
        .edges
        .retain(|edge| edge.source_id != id && edge.target_id != id);
    let removed_edges = before_edges - graph.edges.len();
    graph.refresh_counts();
    Ok(removed_edges)
}

// ---------------------------------------------------------------------------
// Edge mutations
// ---------------------------------------------------------------------------

pub fn add_edge(graph: &mut GraphFile, edge: Edge) -> Result<()> {
    if graph.node_by_id(&edge.source_id).is_none() {
        bail!("source node not found: {}", edge.source_id);
    }
    if graph.node_by_id(&edge.target_id).is_none() {
        bail!("target node not found: {}", edge.target_id);
    }
    if graph.has_edge(&edge.source_id, &edge.relation, &edge.target_id) {
        bail!(
            "edge already exists: {} {} {}",
            edge.source_id,
            edge.relation,
            edge.target_id
        );
    }
    graph.edges.push(edge);
    graph.refresh_counts();
    Ok(())
}

pub fn remove_edge(
    graph: &mut GraphFile,
    source_id: &str,
    relation: &str,
    target_id: &str,
) -> Result<()> {
    let before = graph.edges.len();
    graph.edges.retain(|edge| {
        !(edge.source_id == source_id && edge.relation == relation && edge.target_id == target_id)
    });
    if before == graph.edges.len() {
        bail!("edge not found: {source_id} {relation} {target_id}");
    }
    graph.refresh_counts();
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|item| item == &value) {
        items.push(value);
    }
}
