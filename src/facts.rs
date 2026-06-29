use std::cmp::Ordering;
use std::collections::HashSet;

use serde::Serialize;

use crate::graph::GraphFile;
use crate::index::Bm25Index;
use crate::text_norm;

const MIN_NODE_CANDIDATES: usize = 20;
const NODE_CANDIDATE_MULTIPLIER: usize = 4;
const NO_OVERLAP_PENALTY: f32 = 0.25;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactMatch {
    pub node_id: String,
    pub fact: String,
    pub score: f32,
}

pub fn collect_facts(
    graph: &GraphFile,
    text: &str,
    limit: usize,
    index: Option<&Bm25Index>,
) -> Vec<FactMatch> {
    if limit == 0 {
        return Vec::new();
    }

    let query_terms = text_norm::expand_query_terms(text);
    if query_terms.is_empty() {
        return Vec::new();
    }

    let query_set: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
    let mut owned_index: Option<Bm25Index> = None;
    let index = match index {
        Some(index) => index,
        None => owned_index.insert(Bm25Index::build(graph)),
    };

    let node_limit = limit
        .saturating_mul(NODE_CANDIDATE_MULTIPLIER)
        .max(MIN_NODE_CANDIDATES);
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut matches = Vec::new();

    for (node_id, node_score) in index
        .search(&query_terms, graph)
        .into_iter()
        .take(node_limit)
    {
        let Some(node) = graph.node_by_id(&node_id) else {
            continue;
        };

        for fact in &node.properties.key_facts {
            if fact.trim().is_empty() {
                continue;
            }

            let dedupe_key = (node.id.clone(), fact.clone());
            if !seen.insert(dedupe_key) {
                continue;
            }

            let fact_terms = text_norm::tokenize(fact);
            if fact_terms.is_empty() {
                continue;
            }

            let overlap_count = fact_terms
                .iter()
                .filter(|term| query_set.contains(term.as_str()))
                .count();
            let overlap_ratio = overlap_count as f32 / fact_terms.len() as f32;
            let overlap_multiplier = if overlap_count == 0 {
                NO_OVERLAP_PENALTY
            } else {
                1.0 + overlap_count as f32 + overlap_ratio
            };

            matches.push(FactMatch {
                node_id: node.id.clone(),
                fact: fact.clone(),
                score: node_score * overlap_multiplier,
            });
        }
    }

    matches.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.node_id.cmp(&right.node_id))
            .then_with(|| left.fact.cmp(&right.fact))
    });
    matches.truncate(limit);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Node, NodeProperties};

    fn node(id: &str, name: &str, alias: &[&str], facts: &[&str], description: &str) -> Node {
        Node {
            id: id.to_owned(),
            r#type: "Concept".to_owned(),
            name: name.to_owned(),
            properties: NodeProperties {
                description: description.to_owned(),
                alias: alias.iter().map(|value| (*value).to_owned()).collect(),
                key_facts: facts.iter().map(|value| (*value).to_owned()).collect(),
                ..NodeProperties::default()
            },
            source_files: Vec::new(),
        }
    }

    fn sample_graph() -> GraphFile {
        let mut graph = GraphFile::new("test");
        graph.nodes = vec![
            node(
                "I:auth",
                "Authentication",
                &["auth", "login"],
                &["uses JWT tokens", "supports password reset via email"],
                "Authentication service for the app",
            ),
            node(
                "K:refrigerator",
                "Refrigerator",
                &["fridge"],
                &["refrigerates at 4C", "uses R600a refrigerant"],
                "Kitchen cooling appliance",
            ),
        ];
        graph.refresh_counts();
        graph
    }

    #[test]
    fn collect_facts_prioritizes_fact_overlap() {
        let graph = sample_graph();
        let matches = collect_facts(&graph, "auth jwt token login", 3, None);

        assert!(!matches.is_empty());
        assert_eq!(matches[0].node_id, "I:auth");
        assert_eq!(matches[0].fact, "uses JWT tokens");
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn collect_facts_returns_empty_when_query_has_no_terms() {
        let graph = sample_graph();
        let matches = collect_facts(&graph, "the and of", 5, None);

        assert!(matches.is_empty());
    }
}
