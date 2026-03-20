use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::graph::GraphFile;

pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Static ontology data
// ---------------------------------------------------------------------------

pub const VALID_TYPES: &[&str] = &[
    "Concept",
    "Process",
    "DataStore",
    "Interface",
    "Rule",
    "Feature",
    "Decision",
    "Convention",
    "Note",
    "Bug",
];

pub const VALID_RELATIONS: &[&str] = &[
    "HAS",
    "STORED_IN",
    "TRIGGERS",
    "CREATED_BY",
    "AFFECTED_BY",
    "AVAILABLE_IN",
    "DOCUMENTED_IN",
    "DEPENDS_ON",
    "TRANSITIONS",
    "DECIDED_BY",
    "GOVERNED_BY",
    "USES",
    "READS_FROM",
];

/// Maps node type -> expected id prefix.
pub const TYPE_TO_PREFIX: &[(&str, &str)] = &[
    ("Concept", "concept"),
    ("Process", "process"),
    ("DataStore", "datastore"),
    ("Interface", "interface"),
    ("Rule", "rule"),
    ("Feature", "feature"),
    ("Decision", "decision"),
    ("Convention", "convention"),
    ("Note", "note"),
    ("Bug", "bug"),
];

/// (relation, valid_source_types, valid_target_types)
/// Empty slice = no constraint for that side.
pub const EDGE_TYPE_RULES: &[(&str, &[&str], &[&str])] = &[
    (
        "HAS",
        &["Concept", "Process", "Interface"],
        &["Concept", "Feature", "DataStore", "Rule", "Interface"],
    ),
    ("STORED_IN", &["Concept", "Process", "Rule"], &["DataStore"]),
    (
        "CREATED_BY",
        &["Concept", "DataStore", "Interface", "Decision"],
        &["Process"],
    ),
    (
        "TRIGGERS",
        &["Process", "Rule"],
        &["Process", "Bug", "Rule"],
    ),
    (
        "AFFECTED_BY",
        &["Concept", "Process", "Decision"],
        &["Bug", "Rule", "Decision"],
    ),
    (
        "AVAILABLE_IN",
        &["Feature", "DataStore", "Concept"],
        &["Interface"],
    ),
    (
        "DOCUMENTED_IN",
        &["Concept", "Process", "Decision", "Rule", "Feature", "Bug"],
        &["Interface", "Note"],
    ),
    (
        "DEPENDS_ON",
        &["Feature", "Process", "Interface"],
        &["Feature", "DataStore", "Interface", "Concept"],
    ),
    ("TRANSITIONS", &["Process", "Rule"], &["Process", "Rule"]),
    (
        "DECIDED_BY",
        &["Concept", "Process", "Interface"],
        &["Decision"],
    ),
    (
        "GOVERNED_BY",
        &["Process", "Interface", "DataStore"],
        &["Convention", "Rule"],
    ),
];

// ---------------------------------------------------------------------------
// Core validation
// ---------------------------------------------------------------------------

pub fn validate_graph(
    graph: &GraphFile,
    cwd: &Path,
    deep: bool,
    base_dir: Option<&str>,
) -> ValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let type_to_prefix: HashMap<&str, &str> = TYPE_TO_PREFIX.iter().copied().collect();
    let edge_rules: HashMap<&str, (&[&str], &[&str])> = EDGE_TYPE_RULES
        .iter()
        .map(|(rel, src, tgt)| (*rel, (*src, *tgt)))
        .collect();

    // -- metadata --
    if graph.metadata.name.trim().is_empty() {
        errors.push("metadata.name missing".to_owned());
    }

    // -- nodes --
    let mut id_counts = HashMap::<&str, usize>::new();
    for node in &graph.nodes {
        *id_counts.entry(node.id.as_str()).or_insert(0) += 1;

        if !VALID_TYPES.contains(&node.r#type.as_str()) {
            errors.push(format!("node {} has invalid type {}", node.id, node.r#type));
        }
        if node.name.trim().is_empty() {
            errors.push(format!("node {} missing name", node.id));
        }
        if node.source_files.is_empty() {
            errors.push(format!("node {} missing source_files", node.id));
        }

        // id convention: prefix:snake_case
        match node.id.split_once(':') {
            Some((prefix, suffix)) => {
                let valid_suffix = !suffix.is_empty()
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase())
                    && suffix
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                if !valid_suffix {
                    errors.push(format!(
                        "node id {} does not match prefix:snake_case",
                        node.id
                    ));
                }
                if let Some(expected) = type_to_prefix.get(node.r#type.as_str()) {
                    if prefix != *expected {
                        errors.push(format!(
                            "node {} prefix {} does not match type {}",
                            node.id, prefix, node.r#type
                        ));
                    }
                }
            }
            None => {
                errors.push(format!(
                    "node id {} does not match prefix:snake_case",
                    node.id
                ));
            }
        }

        // quality warnings (skip Feature nodes)
        if node.r#type != "Feature" {
            if node.properties.description.trim().is_empty() {
                warnings.push(format!("node {} missing description", node.id));
            }
            if node.properties.key_facts.is_empty() {
                warnings.push(format!("node {} missing key_facts", node.id));
            }
            if node.properties.provenance.trim().is_empty() {
                warnings.push(format!("node {} missing provenance", node.id));
            }
        }
        if let Some(confidence) = node.properties.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                warnings.push(format!(
                    "node {} confidence out of range: {}",
                    node.id, confidence
                ));
            }
        }
    }
    for (node_id, count) in &id_counts {
        if *count > 1 {
            errors.push(format!("duplicate node id: {} ({})", node_id, count));
        }
    }

    // -- edges --
    let node_type_map: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.r#type.as_str()))
        .collect();
    let node_ids: HashSet<&str> = node_type_map.keys().copied().collect();
    let mut touched = HashSet::new();
    let mut edge_keys = HashSet::new();

    for edge in &graph.edges {
        if !VALID_RELATIONS.contains(&edge.relation.as_str()) {
            errors.push(format!(
                "edge has invalid relation: {} {} {}",
                edge.source_id, edge.relation, edge.target_id
            ));
        }
        if !node_ids.contains(edge.source_id.as_str()) {
            errors.push(format!(
                "edge source missing: {} {} {}",
                edge.source_id, edge.relation, edge.target_id
            ));
        }
        if !node_ids.contains(edge.target_id.as_str()) {
            errors.push(format!(
                "edge target missing: {} {} {}",
                edge.source_id, edge.relation, edge.target_id
            ));
        }

        // ontology source/target type advisory warnings
        if let (Some(src_type), Some(tgt_type)) = (
            node_type_map.get(edge.source_id.as_str()),
            node_type_map.get(edge.target_id.as_str()),
        ) {
            if let Some((valid_src, valid_tgt)) = edge_rules.get(edge.relation.as_str()) {
                if !valid_src.is_empty() && !valid_src.contains(src_type) {
                    warnings.push(format!(
                        "edge source type unusual: {} {} {} ({})",
                        edge.source_id, edge.relation, edge.target_id, src_type
                    ));
                }
                if !valid_tgt.is_empty() && !valid_tgt.contains(tgt_type) {
                    warnings.push(format!(
                        "edge target type unusual: {} {} {} ({})",
                        edge.source_id, edge.relation, edge.target_id, tgt_type
                    ));
                }
            }
        }

        touched.insert(edge.source_id.as_str());
        touched.insert(edge.target_id.as_str());
        let key = format!("{}|{}|{}", edge.source_id, edge.relation, edge.target_id);
        if !edge_keys.insert(key.clone()) {
            errors.push(format!("duplicate edge: {}", key.replace('|', " ")));
        }
    }

    // orphan nodes = errors (not connected to any edge)
    for node in &graph.nodes {
        if !touched.contains(node.id.as_str()) {
            errors.push(format!("orphan node: {}", node.id));
        }
    }

    // deep: verify source files exist on disk
    if deep {
        let base = base_dir
            .map(|d| cwd.join(d))
            .unwrap_or_else(|| cwd.to_path_buf());
        for node in &graph.nodes {
            for source in &node.source_files {
                if !base.join(source).exists() {
                    errors.push(format!("missing source file: {} -> {}", node.id, source));
                }
            }
        }
    }

    errors.sort();
    warnings.sort();
    ValidationReport { errors, warnings }
}
