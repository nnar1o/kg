use std::cmp::Reverse;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::graph::{Edge, GraphFile, Node};

pub fn render_find(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    full: bool,
) -> String {
    let mut sections = Vec::new();
    for query in queries {
        let matches = find_matches(graph, query, limit, include_features);
        let mut lines = vec![format!("? {query} ({})", matches.len())];
        for node in matches {
            lines.push(render_node_block(graph, node, full));
        }
        sections.push(lines.join("\n"));
    }
    format!("{}\n", sections.join("\n\n"))
}

pub fn render_node(graph: &GraphFile, node: &Node, full: bool) -> String {
    format!("{}\n", render_node_block(graph, node, full))
}

fn render_node_block(graph: &GraphFile, node: &Node, full: bool) -> String {
    let mut lines = Vec::new();
    lines.push(format!("# {} | {}", node.id, node.name));

    if !node.properties.alias.is_empty() {
        lines.push(format!("aka: {}", node.properties.alias.join(", ")));
    }
    if full {
        if !node.properties.domain_area.is_empty() {
            lines.push(format!("domain_area: {}", node.properties.domain_area));
        }
        if !node.properties.provenance.is_empty() {
            lines.push(format!("provenance: {}", node.properties.provenance));
        }
        if let Some(confidence) = node.properties.confidence {
            lines.push(format!("confidence: {confidence}"));
        }
        if !node.properties.created_at.is_empty() {
            lines.push(format!("created_at: {}", node.properties.created_at));
        }
    }

    let facts_to_show = if full {
        node.properties.key_facts.len()
    } else {
        node.properties.key_facts.len().min(2)
    };
    for fact in node.properties.key_facts.iter().take(facts_to_show) {
        lines.push(format!("- {fact}"));
    }
    if node.properties.key_facts.len() > facts_to_show || full {
        lines.push(format!("({} facts total)", node.properties.key_facts.len()));
    }

    for edge in outgoing_edges(graph, &node.id, full) {
        if let Some(target) = graph.node_by_id(&edge.target_id) {
            lines.push(format_edge("->", edge, target));
        }
    }
    for edge in incoming_edges(graph, &node.id, full) {
        if let Some(source) = graph.node_by_id(&edge.source_id) {
            lines.push(format_edge("<-", edge, source));
        }
    }

    lines.join("\n")
}

fn outgoing_edges<'a>(graph: &'a GraphFile, node_id: &str, full: bool) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|edge| edge.source_id == node_id)
        .collect();
    edges.sort_by_key(|edge| (&edge.relation, &edge.target_id));
    if !full {
        edges.truncate(3);
    }
    edges
}

fn incoming_edges<'a>(graph: &'a GraphFile, node_id: &str, full: bool) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|edge| edge.target_id == node_id)
        .collect();
    edges.sort_by_key(|edge| (&edge.relation, &edge.source_id));
    if !full {
        edges.truncate(3);
    }
    edges
}

fn format_edge(prefix: &str, edge: &Edge, related: &Node) -> String {
    let (arrow, relation) = if edge.relation.starts_with("NOT_") {
        (
            format!("{prefix}!"),
            edge.relation.trim_start_matches("NOT_"),
        )
    } else {
        (prefix.to_owned(), edge.relation.as_str())
    };

    let mut line = format!("{arrow} {relation} | {} | {}", related.id, related.name);
    if !edge.properties.detail.is_empty() {
        line.push_str(" | ");
        line.push_str(&truncate(&edge.properties.detail, 80));
    }
    line
}

fn truncate(value: &str, max_len: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_len {
        return value.to_owned();
    }
    let truncated: String = value.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}

fn find_matches<'a>(
    graph: &'a GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
) -> Vec<&'a Node> {
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut scored: Vec<(u32, Reverse<&str>, &'a Node)> = graph
        .nodes
        .iter()
        .filter(|node| include_features || node.r#type != "Feature")
        .filter_map(|node| {
            score_node(node, query, &pattern, &mut matcher)
                .map(|score| (score, Reverse(node.id.as_str()), node))
        })
        .collect();

    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, node)| node)
        .collect()
}

fn score_node(node: &Node, query: &str, pattern: &Pattern, matcher: &mut Matcher) -> Option<u32> {
    let mut total = 0;
    let mut primary_hits = 0;

    let id_score = score_primary_field(query, pattern, matcher, &node.id, 4);
    if id_score > 0 {
        primary_hits += 1;
    }
    total += id_score;

    let name_score = score_primary_field(query, pattern, matcher, &node.name, 3);
    if name_score > 0 {
        primary_hits += 1;
    }
    total += name_score;

    for alias in &node.properties.alias {
        let alias_score = score_primary_field(query, pattern, matcher, alias, 3);
        if alias_score > 0 {
            primary_hits += 1;
        }
        total += alias_score;
    }

    if primary_hits > 0 {
        total += score_secondary_field(query, pattern, matcher, &node.properties.description, 1);
    }

    (total > 0).then_some(total)
}

fn score_field(pattern: &Pattern, matcher: &mut Matcher, value: &str) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    let mut buf = Vec::new();
    let haystack = Utf32Str::new(value, &mut buf);
    pattern.score(haystack, matcher)
}

fn score_primary_field(
    query: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
    value: &str,
    weight: u32,
) -> u32 {
    let bonus = textual_bonus(query, value);
    if bonus == 0 {
        return 0;
    }
    let fuzzy = score_field(pattern, matcher, value).unwrap_or(0);
    (fuzzy + bonus) * weight
}

fn score_secondary_field(
    query: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
    value: &str,
    weight: u32,
) -> u32 {
    let bonus = textual_bonus(query, value);
    if bonus == 0 {
        return 0;
    }
    let fuzzy = score_field(pattern, matcher, value).unwrap_or(0);
    (fuzzy + bonus / 2) * weight
}

fn textual_bonus(query: &str, value: &str) -> u32 {
    let query = query.trim().to_lowercase();
    let value = value.to_lowercase();

    if value == query {
        return 400;
    }
    if value.contains(&query) {
        return 200;
    }

    let token_bonus = query
        .split_whitespace()
        .map(|token| {
            if value.contains(token) {
                80
            } else if is_subsequence(token, &value) {
                40
            } else {
                0
            }
        })
        .sum();

    token_bonus
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    let mut chars = needle.chars();
    let mut current = match chars.next() {
        Some(ch) => ch,
        None => return false,
    };

    for ch in haystack.chars() {
        if ch == current {
            match chars.next() {
                Some(next) => current = next,
                None => return true,
            }
        }
    }

    false
}
