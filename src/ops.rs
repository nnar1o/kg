use anyhow::{Result, anyhow, bail};

use crate::graph::{Edge, GraphFile, Node};
use crate::validate::{
    edge_type_rule, format_edge_source_type_error, format_edge_target_type_error,
    is_generated_node_type, is_generated_relation, is_valid_node_type, is_valid_relation,
};

// ---------------------------------------------------------------------------
// Node mutations
// ---------------------------------------------------------------------------

pub fn validate_node(node: &Node) -> Result<()> {
    if is_generated_node_type(&node.r#type) {
        bail!("generated nodes are managed by kg-index: {}", node.id);
    }
    if !is_valid_node_type(&node.r#type) {
        bail!(
            "invalid node_type '{}'. Valid types: {:?}",
            node.r#type,
            crate::validate::VALID_TYPES
        );
    }
    if let Err(error) = crate::validate::canonicalize_node_id_for_type(&node.id, &node.r#type) {
        bail!(error);
    }
    validate_required_metadata(node)?;
    Ok(())
}

pub fn add_node(graph: &mut GraphFile, mut node: Node) -> Result<()> {
    normalize_sources(&mut node.source_files);
    validate_node(&node)?;
    if graph.node_by_id(&node.id).is_some() {
        bail!("node already exists: {}", node.id);
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
    importance: Option<f64>,
    facts: Vec<String>,
    aliases: Vec<String>,
    sources: Vec<String>,
    valid_from: Option<String>,
    valid_to: Option<String>,
) -> Result<()> {
    let idx = graph
        .nodes
        .iter()
        .position(|node| node.id == id)
        .ok_or_else(|| anyhow!("node not found: {id}"))?;

    if is_generated_node_type(&graph.nodes[idx].r#type) {
        bail!("generated nodes are managed by kg-index: {id}");
    }

    let mut updated = graph.nodes[idx].clone();

    if let Some(v) = node_type {
        updated.r#type = v;
    }
    if let Some(v) = name {
        updated.name = v;
    }
    if let Some(v) = description {
        updated.properties.description = v;
    }
    if let Some(v) = domain_area {
        updated.properties.domain_area = v;
    }
    if let Some(v) = provenance {
        updated.properties.provenance = v;
    }
    if let Some(v) = confidence {
        updated.properties.confidence = Some(v);
    }
    if let Some(v) = created_at {
        updated.properties.created_at = v;
    }
    if let Some(v) = importance {
        validate_importance(v)?;
        updated.properties.importance = v;
    }
    for fact in facts {
        push_unique(&mut updated.properties.key_facts, fact);
    }
    for alias in aliases {
        push_unique(&mut updated.properties.alias, alias);
    }
    for source in sources {
        push_unique(&mut updated.source_files, source);
    }
    if let Some(v) = valid_from {
        updated.properties.valid_from = v;
    }
    if let Some(v) = valid_to {
        updated.properties.valid_to = v;
    }

    normalize_sources(&mut updated.source_files);

    validate_node(&updated)?;
    graph.nodes[idx] = updated;

    graph.refresh_counts();
    Ok(())
}

fn validate_importance(value: f64) -> Result<()> {
    if crate::validate::is_valid_importance(value) {
        return Ok(());
    }
    bail!("importance must be in range 0..1, got {value}")
}

fn validate_required_metadata(node: &Node) -> Result<()> {
    if node.name.trim().is_empty() {
        bail!("name is required and cannot be empty");
    }
    if node.properties.description.trim().is_empty() {
        bail!("description is required and cannot be empty");
    }
    if node.properties.domain_area.trim().is_empty() {
        bail!("domain_area is required and cannot be empty");
    }
    if node.properties.provenance.trim().is_empty() {
        bail!("provenance is required and cannot be empty");
    }
    if !crate::validate::VALID_PROVENANCE_CODES.contains(&node.properties.provenance.as_str()) {
        bail!(
            "provenance must be one of: {}",
            crate::validate::VALID_PROVENANCE_CODES.join(", ")
        );
    }
    let Some(confidence) = node.properties.confidence else {
        bail!("confidence is required and must be in range 0..1");
    };
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be in range 0..1, got {confidence}");
    }
    if node.properties.created_at.trim().is_empty() {
        bail!("created_at is required and cannot be empty");
    }
    if !crate::validate::is_valid_iso_utc_timestamp(node.properties.created_at.trim()) {
        bail!("created_at must use UTC format YYYY-MM-DDTHH:MM:SSZ");
    }

    if node.source_files.is_empty() {
        bail!("at least one --source is required");
    }
    for source in &node.source_files {
        if let Err(err) = crate::validate::validate_source_reference(source) {
            bail!("invalid source '{}': {err}", source);
        }
    }

    validate_importance(node.properties.importance)?;
    Ok(())
}

pub fn remove_node(graph: &mut GraphFile, id: &str) -> Result<usize> {
    if graph
        .node_by_id(id)
        .is_some_and(|node| is_generated_node_type(&node.r#type))
    {
        bail!("generated nodes are managed by kg-index: {id}");
    }
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

pub fn validate_edge(graph: &GraphFile, edge: &Edge) -> Result<()> {
    if is_generated_relation(&edge.relation) {
        bail!("generated edges are managed by kg-index: {} {} {}", edge.source_id, edge.relation, edge.target_id);
    }
    if !is_valid_relation(&edge.relation) {
        bail!(
            "invalid relation '{}'. Valid: {:?}",
            edge.relation,
            crate::validate::VALID_RELATIONS
        );
    }

    if let Err(error) = crate::validate::validate_bidirectional_similarity_edge(
        &edge.source_id,
        &edge.relation,
        &edge.target_id,
        &edge.properties.detail,
        edge.properties.bidirectional,
    ) {
        bail!(error);
    }

    let src_node = graph.node_by_id(&edge.source_id);
    let tgt_node = graph.node_by_id(&edge.target_id);

    if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
        if crate::validate::VALID_TYPES.contains(&src.r#type.as_str())
            && crate::validate::VALID_TYPES.contains(&tgt.r#type.as_str())
        {
            if let Some((src_types, tgt_types)) = edge_type_rule(edge.relation.as_str()) {
                if !src_types.is_empty() && !src_types.contains(&src.r#type.as_str()) {
                    bail!(
                        "{}",
                        format_edge_source_type_error(
                            &src.r#type,
                            edge.relation.as_str(),
                            src_types
                        )
                    );
                }
                if !tgt_types.is_empty() && !tgt_types.contains(&tgt.r#type.as_str()) {
                    bail!(
                        "{}",
                        format_edge_target_type_error(
                            &tgt.r#type,
                            edge.relation.as_str(),
                            tgt_types
                        )
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn add_edge(graph: &mut GraphFile, edge: Edge) -> Result<()> {
    validate_edge(graph, &edge)?;
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
    if is_generated_relation(relation) {
        bail!("generated edges are managed by kg-index: {source_id} {relation} {target_id}");
    }
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

fn normalize_sources(sources: &mut Vec<String>) {
    for source in sources.iter_mut() {
        *source = crate::validate::normalize_source_reference(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_node(id: &str, name: &str, node_type: &str) -> crate::graph::Node {
        let mut properties = crate::graph::NodeProperties::default();
        properties.description = "Test description".to_owned();
        properties.domain_area = "test_domain".to_owned();
        properties.provenance = "U".to_owned();
        properties.confidence = Some(0.9);
        properties.created_at = "2026-01-01T00:00:00Z".to_owned();
        properties.importance = 0.8;
        crate::graph::Node {
            id: id.to_owned(),
            name: name.to_owned(),
            r#type: node_type.to_owned(),
            properties,
            source_files: vec!["DOC /tmp/test.md".to_owned()],
        }
    }

    #[test]
    fn push_unique_adds_new_items() {
        let mut items = vec!["a".to_string(), "b".to_string()];
        push_unique(&mut items, "c".to_string());
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    #[test]
    fn push_unique_ignores_duplicates() {
        let mut items = vec!["a".to_string(), "b".to_string()];
        push_unique(&mut items, "a".to_string());
        assert_eq!(items, vec!["a", "b"]);
    }

    #[test]
    fn add_node_rejects_duplicate_id() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let node = valid_node("concept:n1", "Test Node", "Concept");
        add_node(&mut graph, node.clone()).unwrap();
        let result = add_node(&mut graph, node);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "node already exists: concept:n1"
        );
    }

    #[test]
    fn add_node_requires_source() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let mut node = valid_node("concept:n1", "Test Node", "Concept");
        node.source_files.clear();
        let result = add_node(&mut graph, node);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "at least one --source is required"
        );
    }

    #[test]
    fn remove_node_returns_removed_count() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let node = valid_node("concept:n1", "Test Node", "Concept");
        add_node(&mut graph, node).unwrap();
        let removed = remove_node(&mut graph, "concept:n1").unwrap();
        assert_eq!(removed, 0);
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn add_edge_validates_source_exists() {
        use crate::graph::{Edge, EdgeProperties, GraphFile};
        let mut graph = GraphFile::new("test");
        let node = valid_node("concept:target", "Target", "Concept");
        add_node(&mut graph, node).unwrap();
        let edge = Edge {
            source_id: "concept:nonexistent".to_string(),
            target_id: "concept:target".to_string(),
            relation: "DEPENDS_ON".to_string(),
            properties: EdgeProperties::default(),
        };
        let result = add_edge(&mut graph, edge);
        let err_msg = result.unwrap_err().to_string();
        eprintln!("Error: {}", err_msg);
        assert!(
            err_msg.contains("source node not found"),
            "Got: {}",
            err_msg
        );
    }

    #[test]
    fn add_node_accepts_custom_type() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let node = valid_node("~:n1", "Virtual Node", "~");

        add_node(&mut graph, node).expect("custom type should be accepted");
        assert!(graph.node_by_id("~:n1").is_some());
    }

    #[test]
    fn add_node_rejects_generated_type() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let node = valid_node("GDIR:root", "Root", "GDIR");

        let err = add_node(&mut graph, node).expect_err("generated type should be rejected");
        assert!(err.to_string().contains("managed by kg-index"));
    }

    #[test]
    fn remove_node_rejects_generated_node() {
        use crate::graph::GraphFile;
        let mut graph = GraphFile::new("test");
        let node = valid_node("GDIR:root", "Root", "GDIR");
        graph.nodes.push(node);

        let err = remove_node(&mut graph, "GDIR:root").expect_err("generated node should be blocked");
        assert!(err.to_string().contains("managed by kg-index"));
    }

    #[test]
    fn add_edge_accepts_custom_relation() {
        use crate::graph::{Edge, EdgeProperties, GraphFile};
        let mut graph = GraphFile::new("test");
        let source = valid_node("concept:source", "Source", "Concept");
        let target = valid_node("concept:target", "Target", "Concept");
        add_node(&mut graph, source).expect("source node");
        add_node(&mut graph, target).expect("target node");

        add_edge(
            &mut graph,
            Edge {
                source_id: "concept:source".to_owned(),
                relation: "~".to_owned(),
                target_id: "concept:target".to_owned(),
                properties: EdgeProperties::default(),
            },
        )
        .expect("custom relation should be accepted");

        assert!(graph.has_edge("concept:source", "~", "concept:target"));
    }

    #[test]
    fn add_edge_rejects_generated_relation() {
        use crate::graph::{Edge, EdgeProperties, GraphFile};
        let mut graph = GraphFile::new("test");
        let source = valid_node("concept:source", "Source", "Concept");
        let target = valid_node("concept:target", "Target", "Concept");
        add_node(&mut graph, source).expect("source node");
        add_node(&mut graph, target).expect("target node");

        let err = add_edge(
            &mut graph,
            Edge {
                source_id: "concept:source".to_owned(),
                relation: "GCONTAINS".to_owned(),
                target_id: "concept:target".to_owned(),
                properties: EdgeProperties::default(),
            },
        )
        .expect_err("generated relation should be rejected");

        assert!(err.to_string().contains("managed by kg-index"));
    }

    #[test]
    fn add_edge_rejects_invalid_bidirectional_similarity_score() {
        use crate::graph::{Edge, EdgeProperties, GraphFile};
        let mut graph = GraphFile::new("test");
        let source = valid_node("~:a", "A", "~");
        let target = valid_node("~:b", "B", "~");
        add_node(&mut graph, source).expect("source node");
        add_node(&mut graph, target).expect("target node");

        let err = add_edge(
            &mut graph,
            Edge {
                source_id: "~:a".to_owned(),
                relation: "~".to_owned(),
                target_id: "~:b".to_owned(),
                properties: EdgeProperties {
                    detail: "2.0".to_owned(),
                    bidirectional: true,
                    ..Default::default()
                },
            },
        )
        .expect_err("invalid score should fail");

        assert!(err.to_string().contains("requires score in range 0..1"));
    }

    #[test]
    fn add_edge_allows_has_from_custom_cluster_type() {
        use crate::graph::{Edge, EdgeProperties, GraphFile};
        let mut graph = GraphFile::new("test");
        add_node(&mut graph, valid_node("@:cluster_0001", "Cluster", "@")).expect("cluster");
        add_node(&mut graph, valid_node("concept:x", "X", "Concept")).expect("member");

        add_edge(
            &mut graph,
            Edge {
                source_id: "@:cluster_0001".to_owned(),
                relation: "HAS".to_owned(),
                target_id: "concept:x".to_owned(),
                properties: EdgeProperties {
                    detail: "0.88".to_owned(),
                    ..Default::default()
                },
            },
        )
        .expect("custom cluster membership should be accepted");
    }
}
