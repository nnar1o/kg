use serde::Serialize;

use crate::graph::{GraphFile, Node};
use crate::index::Bm25Index;
use crate::output;

#[derive(Debug, Serialize)]
pub(crate) struct FindQueryResult {
    query: String,
    count: usize,
    nodes: Vec<ScoredFindNode>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ScoredFindNode {
    score: i64,
    node: Node,
    #[serde(skip_serializing_if = "Option::is_none")]
    score_breakdown: Option<ScoredFindBreakdown>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ScoredFindBreakdown {
    raw_relevance: f64,
    normalized_relevance: i64,
    lexical_boost: i64,
    feedback_boost: i64,
    importance_boost: i64,
    authority_raw: i64,
    authority_applied: i64,
    authority_cap: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct FindResponse {
    total: usize,
    queries: Vec<FindQueryResult>,
}

pub(crate) fn render_find_json_with_index(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_metadata: bool,
    mode: output::FindMode,
    debug_score: bool,
    index: Option<&Bm25Index>,
    tune: Option<&output::FindTune>,
) -> String {
    let mut total = 0usize;
    let mut results = Vec::new();
    for query in queries {
        let (count, scored_nodes) = output::find_scored_nodes_and_total_with_index_tuned(
            graph,
            query,
            limit,
            true,
            include_metadata,
            mode,
            index,
            tune,
        );
        total += count;
        let nodes = scored_nodes
            .into_iter()
            .map(|entry| ScoredFindNode {
                score: entry.score,
                node: entry.node,
                score_breakdown: debug_score.then_some(ScoredFindBreakdown {
                    raw_relevance: entry.breakdown.raw_relevance,
                    normalized_relevance: entry.breakdown.normalized_relevance,
                    lexical_boost: entry.breakdown.lexical_boost,
                    feedback_boost: entry.breakdown.feedback_boost,
                    importance_boost: entry.breakdown.importance_boost,
                    authority_raw: entry.breakdown.authority_raw,
                    authority_applied: entry.breakdown.authority_applied,
                    authority_cap: entry.breakdown.authority_cap,
                }),
            })
            .collect();
        results.push(FindQueryResult {
            query: query.clone(),
            count,
            nodes,
        });
    }
    let payload = FindResponse {
        total,
        queries: results,
    };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

#[derive(Debug, Serialize)]
pub(crate) struct NodeGetResponse {
    node: Node,
}

pub(crate) fn render_node_json(node: &Node) -> String {
    let payload = NodeGetResponse { node: node.clone() };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphFile, NodeProperties};

    fn sample_node(id: &str) -> Node {
        Node {
            id: id.to_owned(),
            r#type: "Concept".to_owned(),
            name: "Test".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        }
    }

    #[test]
    fn render_node_json_includes_id_and_type() {
        let json = render_node_json(&sample_node("concept:test"));
        assert!(json.contains("\"id\": \"concept:test\""));
        assert!(json.contains("\"type\": \"Concept\""));
    }

    #[test]
    fn render_find_json_with_index_empty_result() {
        let graph = GraphFile::new("test");
        let json = render_find_json_with_index(&graph, &["nonexistent".to_owned()], 10, false, crate::output::FindMode::Fuzzy, false, None, None);
        assert!(json.contains("\"total\": 0"));
        assert!(json.contains("\"nodes\": []"));
    }

    #[test]
    fn render_find_json_with_index_multi_query() {
        let graph = GraphFile::new("test");
        let json = render_find_json_with_index(&graph, &["a".to_owned(), "b".to_owned()], 5, false, crate::output::FindMode::Fuzzy, false, None, None);
        assert_eq!(json.matches("\"query\"").count(), 2);
    }
}
