use anyhow::{Result, anyhow, bail};

use crate::graph::{Edge, GraphFile, Node};
use crate::validate::{EDGE_TYPE_RULES, TYPE_TO_PREFIX, VALID_TYPES};

// ---------------------------------------------------------------------------
// Node mutations
// ---------------------------------------------------------------------------

pub fn validate_node(node: &Node) -> Result<()> {
    if !VALID_TYPES.contains(&node.r#type.as_str()) {
        bail!(
            "invalid node_type '{}'. Valid types: {:?}",
            node.r#type,
            VALID_TYPES
        );
    }
    if let Some((prefix, suffix)) = node.id.split_once(':') {
        for (typ, exp_prefix) in TYPE_TO_PREFIX {
            if *typ == node.r#type {
                if prefix != *exp_prefix {
                    bail!(
                        "node id '{}' has prefix '{}' but type '{}' expects prefix '{}'",
                        node.id,
                        prefix,
                        node.r#type,
                        exp_prefix
                    );
                }
                break;
            }
        }
        if !suffix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            bail!(
                "node id '{}' must be prefix:snake_case (lowercase start)",
                node.id
            );
        }
        if !suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            bail!(
                "node id '{}' must be prefix:snake_case (lowercase, digits, underscore only)",
                node.id
            );
        }
    } else {
        bail!("node id '{}' must be in format prefix:identifier", node.id);
    }
    validate_importance(node.properties.importance)?;
    Ok(())
}

pub fn add_node(graph: &mut GraphFile, node: Node) -> Result<()> {
    validate_node(&node)?;
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
    importance: Option<u8>,
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
    if let Some(v) = importance {
        validate_importance(v)?;
        node.properties.importance = v;
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

fn validate_importance(value: u8) -> Result<()> {
    if (1..=6).contains(&value) {
        return Ok(());
    }
    bail!("importance must be in range 1..=6, got {value}")
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

pub fn validate_edge(graph: &GraphFile, edge: &Edge) -> Result<()> {
    use crate::validate::VALID_RELATIONS;

    if !VALID_RELATIONS.contains(&edge.relation.as_str()) {
        bail!(
            "invalid relation '{}'. Valid: {:?}",
            edge.relation,
            VALID_RELATIONS
        );
    }

    let src_node = graph.node_by_id(&edge.source_id);
    let tgt_node = graph.node_by_id(&edge.target_id);

    if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
        for (rel, src_types, tgt_types) in EDGE_TYPE_RULES {
            if *rel == edge.relation {
                if !src_types.is_empty() && !src_types.contains(&src.r#type.as_str()) {
                    bail!(
                        "edge relation '{}' requires source type in {:?}, got '{}'",
                        edge.relation,
                        src_types,
                        src.r#type
                    );
                }
                if !tgt_types.is_empty() && !tgt_types.contains(&tgt.r#type.as_str()) {
                    bail!(
                        "edge relation '{}' requires target type in {:?}, got '{}'",
                        edge.relation,
                        tgt_types,
                        tgt.r#type
                    );
                }
                break;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::graph::{GraphFile, Node, NodeProperties};
        let mut graph = GraphFile::new("test");
        let node = Node {
            id: "concept:n1".to_string(),
            name: "Test Node".to_string(),
            r#type: "Concept".to_string(),
            properties: NodeProperties::default(),
            source_files: vec!["test.rs".to_string()],
        };
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
        use crate::graph::{GraphFile, Node, NodeProperties};
        let mut graph = GraphFile::new("test");
        let node = Node {
            id: "concept:n1".to_string(),
            name: "Test Node".to_string(),
            r#type: "Concept".to_string(),
            properties: NodeProperties::default(),
            source_files: vec![],
        };
        let result = add_node(&mut graph, node);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "at least one --source is required"
        );
    }

    #[test]
    fn remove_node_returns_removed_count() {
        use crate::graph::{GraphFile, Node, NodeProperties};
        let mut graph = GraphFile::new("test");
        let node = Node {
            id: "concept:n1".to_string(),
            name: "Test Node".to_string(),
            r#type: "Concept".to_string(),
            properties: NodeProperties::default(),
            source_files: vec!["test.rs".to_string()],
        };
        add_node(&mut graph, node).unwrap();
        let removed = remove_node(&mut graph, "concept:n1").unwrap();
        assert_eq!(removed, 0);
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn add_edge_validates_source_exists() {
        use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};
        let mut graph = GraphFile::new("test");
        let node = Node {
            id: "concept:target".to_string(),
            name: "Target".to_string(),
            r#type: "Concept".to_string(),
            properties: NodeProperties::default(),
            source_files: vec!["test.rs".to_string()],
        };
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
}
