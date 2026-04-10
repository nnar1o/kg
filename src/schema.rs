use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::validate::{format_edge_source_type_error, format_edge_target_type_error};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GraphSchema {
    #[serde(default)]
    pub node_types: NodeTypeSchema,
    #[serde(default)]
    pub relations: RelationsSchema,
    #[serde(default)]
    pub edge_rules: Vec<EdgeRule>,
    #[serde(default)]
    pub uniqueness: Vec<UniquenessConstraint>,
    #[serde(default)]
    pub id_patterns: IdPatternsSchema,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NodeTypeSchema {
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub required_fields: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RelationsSchema {
    #[serde(default)]
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EdgeRule {
    pub relation: String,
    #[serde(default)]
    pub source_types: Vec<String>,
    #[serde(default)]
    pub target_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UniquenessConstraint {
    pub scope: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdPatternsSchema {
    #[serde(default)]
    pub prefix_to_type: HashMap<String, String>,
    #[serde(default)]
    pub enforce_prefix_match: bool,
}

impl GraphSchema {
    pub fn discover(start: &Path) -> Result<Option<(PathBuf, Self)>> {
        for dir in start.ancestors() {
            let path = dir.join(".kg.schema.toml");
            if path.exists() {
                let schema = Self::load(&path)?;
                return Ok(Some((path, schema)));
            }
        }
        Ok(None)
    }

    fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read schema: {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("invalid schema TOML: {}", path.display()))
    }

    pub fn validate_node_add(&self, node: &crate::graph::Node) -> Vec<SchemaViolation> {
        let mut violations = Vec::new();

        if !self.node_types.allowed.is_empty() && !self.node_types.allowed.contains(&node.r#type) {
            violations.push(SchemaViolation {
                kind: ViolationKind::InvalidType,
                message: format!(
                    "node type '{}' is not allowed by schema (allowed: {:?})",
                    node.r#type, self.node_types.allowed
                ),
                entity_id: Some(node.id.clone()),
                entity_type: Some("node".to_owned()),
            });
        }

        if let Some(required) = self.node_types.required_fields.get(&node.r#type) {
            for field in required {
                let has_field = match field.as_str() {
                    "description" => !node.properties.description.trim().is_empty(),
                    "domain_area" => !node.properties.domain_area.trim().is_empty(),
                    "provenance" => !node.properties.provenance.trim().is_empty(),
                    "confidence" => node.properties.confidence.is_some(),
                    "importance" => (0.0..=1.0).contains(&node.properties.importance),
                    "key_facts" => !node.properties.key_facts.is_empty(),
                    "alias" => !node.properties.alias.is_empty(),
                    "source_files" => !node.source_files.is_empty(),
                    _ => false,
                };
                if !has_field {
                    violations.push(SchemaViolation {
                        kind: ViolationKind::MissingRequiredField,
                        message: format!(
                            "node {} (type '{}') is missing required field '{}'",
                            node.id, node.r#type, field
                        ),
                        entity_id: Some(node.id.clone()),
                        entity_type: Some("node".to_owned()),
                    });
                }
            }
        }

        if self.id_patterns.enforce_prefix_match {
            if let Some((prefix, _suffix)) = node.id.split_once(':') {
                if let Some(expected_type) = self.id_patterns.prefix_to_type.get(prefix) {
                    if expected_type != &node.r#type {
                        violations.push(SchemaViolation {
                            kind: ViolationKind::IdPrefixMismatch,
                            message: format!(
                                "node {} has prefix '{}' but type '{}' (expected type for this prefix: '{}')",
                                node.id, prefix, node.r#type, expected_type
                            ),
                            entity_id: Some(node.id.clone()),
                            entity_type: Some("node".to_owned()),
                        });
                    }
                }
            }
        }

        violations
    }

    pub fn validate_edge_add(
        &self,
        source_id: &str,
        source_type: &str,
        relation: &str,
        target_id: &str,
        target_type: &str,
    ) -> Vec<SchemaViolation> {
        let mut violations = Vec::new();

        if !self.relations.allowed.is_empty()
            && !self.relations.allowed.contains(&relation.to_string())
        {
            violations.push(SchemaViolation {
                kind: ViolationKind::InvalidRelation,
                message: format!(
                    "relation '{}' is not allowed by schema (allowed: {:?})",
                    relation, self.relations.allowed
                ),
                entity_id: Some(format!("{} {} {}", source_id, relation, target_id)),
                entity_type: Some("edge".to_owned()),
            });
        }

        for rule in &self.edge_rules {
            if rule.relation == relation {
                if !rule.source_types.is_empty()
                    && !rule.source_types.contains(&source_type.to_string())
                {
                    violations.push(SchemaViolation {
                        kind: ViolationKind::EdgeTypeConstraint,
                        message: format!(
                            "edge {} {} {} invalid: {}",
                            source_id,
                            relation,
                            target_id,
                            format_edge_source_type_error(
                                source_type,
                                relation,
                                &rule.source_types
                            )
                        ),
                        entity_id: Some(format!("{} {} {}", source_id, relation, target_id)),
                        entity_type: Some("edge".to_owned()),
                    });
                }
                if !rule.target_types.is_empty()
                    && !rule.target_types.contains(&target_type.to_string())
                {
                    violations.push(SchemaViolation {
                        kind: ViolationKind::EdgeTypeConstraint,
                        message: format!(
                            "edge {} {} {} invalid: {}",
                            source_id,
                            relation,
                            target_id,
                            format_edge_target_type_error(
                                target_type,
                                relation,
                                &rule.target_types
                            )
                        ),
                        entity_id: Some(format!("{} {} {}", source_id, relation, target_id)),
                        entity_type: Some("edge".to_owned()),
                    });
                }
                break;
            }
        }

        violations
    }

    pub fn validate_uniqueness(&self, nodes: &[crate::graph::Node]) -> Vec<SchemaViolation> {
        let mut violations = Vec::new();

        for constraint in &self.uniqueness {
            if constraint.scope.as_str() == "global" {
                let mut seen: HashMap<String, &crate::graph::Node> = HashMap::new();
                for node in nodes {
                    let key = match constraint.fields.as_slice() {
                        [id] if id == "id" => node.id.clone(),
                        [type_f, name_f] if type_f == "type" && name_f == "name" => {
                            format!("{}:{}", node.r#type, node.name)
                        }
                        _ => continue,
                    };
                    if let Some(existing) = seen.get(&key) {
                        violations.push(SchemaViolation {
                            kind: ViolationKind::UniquenessViolation,
                            message: format!(
                                "uniqueness violation: '{}' appears in both {} and {}",
                                key, existing.id, node.id
                            ),
                            entity_id: Some(node.id.clone()),
                            entity_type: Some("node".to_owned()),
                        });
                    } else {
                        seen.insert(key, node);
                    }
                }
            }
        }

        violations
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaViolation {
    pub kind: ViolationKind,
    pub message: String,
    pub entity_id: Option<String>,
    pub entity_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViolationKind {
    InvalidType,
    InvalidRelation,
    MissingRequiredField,
    EdgeTypeConstraint,
    IdPrefixMismatch,
    UniquenessViolation,
}

impl std::fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
