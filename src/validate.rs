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

pub const VALID_PROVENANCE_CODES: &[&str] = &["U", "D", "A"];

pub const VALID_SOURCE_TYPES: &[&str] = &[
    "URL",
    "SVN",
    "SOURCECODE",
    "WIKI",
    "CONFLUENCE",
    "CONVERSATION",
    "GIT_COMMIT",
    "PULL_REQUEST",
    "ISSUE",
    "DOC",
    "LOG",
    "OTHER",
];

const MAX_CUSTOM_TYPE_LEN: usize = 48;
const MAX_CUSTOM_RELATION_LEN: usize = 64;

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

/// Maps node type -> canonical short code used in IDs.
pub const TYPE_TO_CODE: &[(&str, &str)] = &[
    ("Concept", "K"),
    ("Process", "P"),
    ("DataStore", "D"),
    ("Interface", "I"),
    ("Rule", "R"),
    ("Feature", "F"),
    ("Decision", "Z"),
    ("Convention", "C"),
    ("Note", "N"),
    ("Bug", "B"),
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
        &[
            "Concept",
            "Process",
            "DataStore",
            "Interface",
            "Rule",
            "Feature",
            "Decision",
            "Bug",
        ],
        &[
            "Concept",
            "Process",
            "DataStore",
            "Interface",
            "Rule",
            "Feature",
            "Decision",
            "Convention",
            "Bug",
        ],
    ),
    (
        "AVAILABLE_IN",
        &["Feature", "DataStore", "Concept", "Process"],
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

pub fn edge_type_rule(
    relation: &str,
) -> Option<(&'static [&'static str], &'static [&'static str])> {
    EDGE_TYPE_RULES
        .iter()
        .find(|(rule_relation, _, _)| *rule_relation == relation)
        .map(|(_, source_types, target_types)| (*source_types, *target_types))
}

pub fn canonical_type_code_for(node_type: &str) -> Option<&'static str> {
    TYPE_TO_CODE
        .iter()
        .find(|(typ, _)| *typ == node_type)
        .map(|(_, code)| *code)
}

fn type_for_prefix(prefix: &str) -> Option<&'static str> {
    TYPE_TO_PREFIX
        .iter()
        .find(|(_, known_prefix)| *known_prefix == prefix)
        .map(|(typ, _)| *typ)
}

fn type_for_code(code: &str) -> Option<&'static str> {
    TYPE_TO_CODE
        .iter()
        .find(|(_, known_code)| *known_code == code)
        .map(|(typ, _)| *typ)
}

fn valid_id_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_valid_custom_token(token: &str, max_len: usize) -> bool {
    if token.is_empty() || token.len() > max_len {
        return false;
    }
    if token.chars().any(char::is_whitespace) {
        return false;
    }
    token.chars().all(|ch| ch.is_ascii_graphic())
}

pub fn is_valid_node_type(value: &str) -> bool {
    VALID_TYPES.contains(&value) || is_valid_custom_token(value, MAX_CUSTOM_TYPE_LEN)
}

pub fn is_valid_relation(value: &str) -> bool {
    VALID_RELATIONS.contains(&value) || is_valid_custom_token(value, MAX_CUSTOM_RELATION_LEN)
}

fn parse_similarity_score(value: &str) -> Option<f64> {
    let score = value.trim().parse::<f64>().ok()?;
    if (0.0..=1.0).contains(&score) {
        Some(score)
    } else {
        None
    }
}

fn is_valid_score_component_label(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('C'))
        && chars.clone().next().is_some()
        && chars.all(|ch| ch.is_ascii_digit())
}

pub fn validate_bidirectional_similarity_edge(
    source_id: &str,
    relation: &str,
    target_id: &str,
    detail: &str,
    bidirectional: bool,
) -> Result<(), String> {
    if !bidirectional {
        return Ok(());
    }
    if relation != "~" {
        return Err(format!(
            "bidirectional edge requires '~' relation: {} {} {}",
            source_id, relation, target_id
        ));
    }
    if source_id > target_id {
        return Err(format!(
            "bidirectional edge must be canonicalized (source <= target): {} ~ {}",
            source_id, target_id
        ));
    }
    if parse_similarity_score(detail).is_none() {
        return Err(format!(
            "bidirectional similarity edge requires score in range 0..1: {} ~ {}",
            source_id, target_id
        ));
    }
    Ok(())
}

pub fn is_valid_iso_utc_timestamp(value: &str) -> bool {
    if value.len() != 20 {
        return false;
    }
    let bytes = value.as_bytes();
    let is_digit = |idx: usize| bytes.get(idx).is_some_and(|b| b.is_ascii_digit());
    if !(is_digit(0)
        && is_digit(1)
        && is_digit(2)
        && is_digit(3)
        && bytes.get(4) == Some(&b'-')
        && is_digit(5)
        && is_digit(6)
        && bytes.get(7) == Some(&b'-')
        && is_digit(8)
        && is_digit(9)
        && bytes.get(10) == Some(&b'T')
        && is_digit(11)
        && is_digit(12)
        && bytes.get(13) == Some(&b':')
        && is_digit(14)
        && is_digit(15)
        && bytes.get(16) == Some(&b':')
        && is_digit(17)
        && is_digit(18)
        && bytes.get(19) == Some(&b'Z'))
    {
        return false;
    }

    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let hour = value[11..13].parse::<u32>().ok();
    let minute = value[14..16].parse::<u32>().ok();
    let second = value[17..19].parse::<u32>().ok();
    matches!(month, Some(1..=12))
        && matches!(day, Some(1..=31))
        && matches!(hour, Some(0..=23))
        && matches!(minute, Some(0..=59))
        && matches!(second, Some(0..=59))
}

pub fn is_valid_iso_date(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let bytes = value.as_bytes();
    let is_digit = |idx: usize| bytes.get(idx).is_some_and(|b| b.is_ascii_digit());
    if !(is_digit(0)
        && is_digit(1)
        && is_digit(2)
        && is_digit(3)
        && bytes.get(4) == Some(&b'-')
        && is_digit(5)
        && is_digit(6)
        && bytes.get(7) == Some(&b'-')
        && is_digit(8)
        && is_digit(9))
    {
        return false;
    }
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    matches!(month, Some(1..=12)) && matches!(day, Some(1..=31))
}

pub fn validate_source_reference(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("source entry cannot be empty".to_owned());
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!(
            "source '{}' must have format '<TYPE> <LINK_OR_DATE> <OPTIONAL_DETAILS>'",
            value
        ));
    }

    let source_type = parts[0];
    if !VALID_SOURCE_TYPES.contains(&source_type) {
        return Err(format!(
            "source '{}' uses invalid type '{}'; valid types: {}",
            value,
            source_type,
            VALID_SOURCE_TYPES.join(", ")
        ));
    }

    match source_type {
        "CONVERSATION" => {
            if !is_valid_iso_date(parts[1]) {
                return Err(format!(
                    "source '{}' must use date format YYYY-MM-DD for CONVERSATION",
                    value
                ));
            }
        }
        "GIT_COMMIT" => {
            if parts.len() < 3 {
                return Err(format!(
                    "source '{}' must use format 'GIT_COMMIT <REPO_URL_OR_NAME> <COMMIT_SHA> <OPTIONAL_DETAILS>'",
                    value
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

pub fn normalize_source_reference(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let source_type = trimmed.split_whitespace().next().unwrap_or_default();
    if VALID_SOURCE_TYPES.contains(&source_type) {
        return trimmed.to_owned();
    }
    format!("DOC {trimmed}")
}

pub fn is_valid_importance(value: f64) -> bool {
    (0.0..=1.0).contains(&value)
}

pub fn is_legacy_importance(value: f64) -> bool {
    value > 1.0 && (1.0..=6.0).contains(&value) && value.fract() == 0.0
}

/// Normalize a node id to legacy `<type_prefix>:snake_case` when possible.
///
/// Accepted inputs include both canonical `TYPE_CODE:snake_case` and legacy
/// `prefix:snake_case` forms. Unknown prefixes are returned unchanged.
pub fn normalize_node_id(id: &str) -> String {
    let Some((head, suffix)) = id.split_once(':') else {
        return id.to_owned();
    };
    let Some(node_type) = type_for_code(head).or_else(|| type_for_prefix(head)) else {
        return id.to_owned();
    };
    let Some(prefix) = TYPE_TO_PREFIX
        .iter()
        .find(|(typ, _)| *typ == node_type)
        .map(|(_, prefix)| *prefix)
    else {
        return id.to_owned();
    };
    format!("{prefix}:{suffix}")
}

/// Validate and canonicalize a node id for a concrete node type.
///
/// Returns canonical legacy `prefix:snake_case` on success.
pub fn canonicalize_node_id_for_type(id: &str, node_type: &str) -> Result<String, String> {
    let Some((head, suffix)) = id.split_once(':') else {
        return Err(format!(
            "node id '{}' must be in format <type_code>:snake_case",
            id
        ));
    };
    if !valid_id_suffix(suffix) {
        return Err(format!(
            "node id '{}' must use snake_case suffix (lowercase, digits, underscore only)",
            id
        ));
    }

    if !is_valid_node_type(node_type) {
        return Err(format!("invalid node type '{node_type}'"));
    }

    let Some(expected_code) = canonical_type_code_for(node_type) else {
        if head == node_type {
            return Ok(format!("{node_type}:{suffix}"));
        }
        return Err(format!(
            "node id '{}' has type marker '{}'; expected '{}' for custom node type",
            id, head, node_type
        ));
    };
    let Some(expected_prefix) = TYPE_TO_PREFIX
        .iter()
        .find(|(typ, _)| *typ == node_type)
        .map(|(_, prefix)| *prefix)
    else {
        return Err(format!("invalid node type '{node_type}'"));
    };

    if head == expected_code || head == expected_prefix {
        return Ok(format!("{expected_prefix}:{suffix}"));
    }

    if let Some(actual_type) = type_for_code(head).or_else(|| type_for_prefix(head)) {
        return Err(format!(
            "node id '{}' has type marker '{}' (type '{}') but node_type is '{}'",
            id, head, actual_type, node_type
        ));
    }

    Err(format!(
        "node id '{}' has unknown type marker '{}'; expected '{}' or '{}'",
        id, head, expected_code, expected_prefix
    ))
}

pub fn format_edge_source_type_error(
    source_type: &str,
    relation: &str,
    allowed_source_types: &[impl AsRef<str>],
) -> String {
    format!(
        "{} cannot be source of {} (allowed: {})",
        source_type,
        relation,
        allowed_source_types
            .iter()
            .map(|value| value.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn format_edge_target_type_error(
    target_type: &str,
    relation: &str,
    allowed_target_types: &[impl AsRef<str>],
) -> String {
    format!(
        "{} cannot be target of {} (allowed: {})",
        target_type,
        relation,
        allowed_target_types
            .iter()
            .map(|value| value.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn validate_graph(
    graph: &GraphFile,
    cwd: &Path,
    deep: bool,
    base_dir: Option<&str>,
) -> ValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let type_to_prefix: HashMap<&str, &str> = TYPE_TO_PREFIX.iter().copied().collect();
    let type_to_code: HashMap<&str, &str> = TYPE_TO_CODE.iter().copied().collect();
    // -- metadata --
    if graph.metadata.name.trim().is_empty() {
        errors.push("metadata.name missing".to_owned());
    }

    // -- nodes --
    let mut id_counts = HashMap::<&str, usize>::new();
    for node in &graph.nodes {
        *id_counts.entry(node.id.as_str()).or_insert(0) += 1;

        if !is_valid_node_type(&node.r#type) {
            errors.push(format!("node {} has invalid type {}", node.id, node.r#type));
        }
        if node.name.trim().is_empty() {
            errors.push(format!("node {} missing name", node.id));
        }
        if node.source_files.is_empty() {
            errors.push(format!("node {} missing source_files", node.id));
        }

        match canonicalize_node_id_for_type(&node.id, &node.r#type) {
            Ok(_) => {}
            Err(_) => {
                if let Some((head, _)) = node.id.split_once(':') {
                    if let (Some(expected_code), Some(expected_prefix)) = (
                        type_to_code.get(node.r#type.as_str()),
                        type_to_prefix.get(node.r#type.as_str()),
                    ) {
                        errors.push(format!(
                            "node id {} invalid for type {} (expected {}:* or {}:*)",
                            node.id, node.r#type, expected_code, expected_prefix
                        ));
                        if type_for_code(head).is_none() && type_for_prefix(head).is_none() {
                            errors.push(format!(
                                "node id {} has unknown type marker '{}'",
                                node.id, head
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "node id {} invalid for custom type {} (expected {}:*)",
                            node.id, node.r#type, node.r#type
                        ));
                    }
                } else {
                    errors.push(format!(
                        "node id {} does not match prefix:snake_case",
                        node.id
                    ));
                }
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
        if is_legacy_importance(node.properties.importance) {
            warnings.push(format!(
                "node {} uses legacy importance scale (1..6): {}",
                node.id, node.properties.importance
            ));
        } else if !is_valid_importance(node.properties.importance) {
            errors.push(format!(
                "node {} importance out of range: {}",
                node.id, node.properties.importance
            ));
        }

        if !node.properties.provenance.trim().is_empty()
            && !VALID_PROVENANCE_CODES.contains(&node.properties.provenance.as_str())
        {
            warnings.push(format!(
                "node {} has non-dictionary provenance '{}' (expected one of: {})",
                node.id,
                node.properties.provenance,
                VALID_PROVENANCE_CODES.join(", ")
            ));
        }

        for source in &node.source_files {
            if let Err(err) = validate_source_reference(source) {
                warnings.push(format!(
                    "node {} has non-standard source '{}': {}",
                    node.id, source, err
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
        if !is_valid_relation(&edge.relation) {
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

        if let Err(err) = validate_bidirectional_similarity_edge(
            &edge.source_id,
            &edge.relation,
            &edge.target_id,
            &edge.properties.detail,
            edge.properties.bidirectional,
        ) {
            errors.push(err);
        }

        for (label, score) in &edge.properties.score_components {
            if !is_valid_score_component_label(label) {
                errors.push(format!(
                    "edge {} {} {} has invalid score component label '{}'",
                    edge.source_id, edge.relation, edge.target_id, label
                ));
            }
            if !(0.0..=1.0).contains(score) {
                errors.push(format!(
                    "edge {} {} {} score component '{}' out of range: {}",
                    edge.source_id, edge.relation, edge.target_id, label, score
                ));
            }
        }

        // Enforce relation semantics from decision table rules.
        if let (Some(src_type), Some(tgt_type)) = (
            node_type_map.get(edge.source_id.as_str()),
            node_type_map.get(edge.target_id.as_str()),
        ) {
            if VALID_TYPES.contains(src_type) && VALID_TYPES.contains(tgt_type) {
                if let Some((valid_src, valid_tgt)) = edge_type_rule(edge.relation.as_str()) {
                    if !valid_src.is_empty() && !valid_src.contains(src_type) {
                        errors.push(format!(
                            "edge {} {} {} invalid: {}",
                            edge.source_id,
                            edge.relation,
                            edge.target_id,
                            format_edge_source_type_error(
                                src_type,
                                edge.relation.as_str(),
                                valid_src
                            )
                        ));
                    }
                    if !valid_tgt.is_empty() && !valid_tgt.contains(tgt_type) {
                        errors.push(format!(
                            "edge {} {} {} invalid: {}",
                            edge.source_id,
                            edge.relation,
                            edge.target_id,
                            format_edge_target_type_error(
                                tgt_type,
                                edge.relation.as_str(),
                                valid_tgt
                            )
                        ));
                    }
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

#[cfg(test)]
mod tests {
    use super::{
        canonicalize_node_id_for_type, is_valid_node_type, is_valid_relation,
        validate_bidirectional_similarity_edge,
    };

    #[test]
    fn canonicalize_node_id_allows_custom_type_marker() {
        let canonical = canonicalize_node_id_for_type("~:dedupe_anchor", "~").expect("custom id");
        assert_eq!(canonical, "~:dedupe_anchor");
    }

    #[test]
    fn canonicalize_node_id_rejects_mismatched_custom_marker() {
        let err = canonicalize_node_id_for_type("custom:dedupe_anchor", "~").unwrap_err();
        assert!(err.contains("expected '~' for custom node type"));
    }

    #[test]
    fn relation_and_node_type_validation_accepts_custom_tokens() {
        assert!(is_valid_node_type("~"));
        assert!(is_valid_relation("~"));
        assert!(!is_valid_node_type(""));
        assert!(!is_valid_relation(" "));
    }

    #[test]
    fn bidirectional_similarity_validation_requires_score_and_canonical_order() {
        assert!(validate_bidirectional_similarity_edge("~:a", "~", "~:b", "0.8", true).is_ok());

        let invalid_score =
            validate_bidirectional_similarity_edge("~:a", "~", "~:b", "1.8", true).unwrap_err();
        assert!(invalid_score.contains("requires score in range 0..1"));

        let invalid_order =
            validate_bidirectional_similarity_edge("~:b", "~", "~:a", "0.8", true).unwrap_err();
        assert!(invalid_order.contains("must be canonicalized"));
    }

    #[test]
    fn score_component_label_validation_accepts_only_c_numeric() {
        assert!(super::is_valid_score_component_label("C1"));
        assert!(super::is_valid_score_component_label("C2"));
        assert!(!super::is_valid_score_component_label("DESC"));
        assert!(!super::is_valid_score_component_label("C"));
    }
}
