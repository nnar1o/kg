#![allow(clippy::unnecessary_sort_by)]

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Result, bail};
use serde::Serialize;

use crate::graph::{Edge, GraphFile, Node, Note};

#[derive(Debug, Clone, PartialEq)]
enum FilterOp {
    Eq,
    Contains,
    NotEq,
    Prefix,
}

impl FilterOp {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "=" => Some(FilterOp::Eq),
            "~" => Some(FilterOp::Contains),
            "!=" => Some(FilterOp::NotEq),
            "^" => Some(FilterOp::Prefix),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Filter {
    key: String,
    op: FilterOp,
    value: String,
}

#[derive(Debug, Clone)]
enum Expr {
    True,
    Filter(Filter),
    And(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy)]
enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
struct SortSpec {
    key: String,
    dir: SortDir,
}

#[derive(Debug, Clone)]
enum QueryKind {
    Node {
        expr: Expr,
        limit: Option<usize>,
        sort: Option<SortSpec>,
    },
    Edge {
        expr: Expr,
        limit: Option<usize>,
        sort: Option<SortSpec>,
    },
    Note {
        expr: Expr,
        limit: Option<usize>,
        sort: Option<SortSpec>,
    },
    Neighbors {
        id: String,
        hops: usize,
        direction: NeighborDir,
        limit: Option<usize>,
    },
    Path {
        from: String,
        to: String,
        max_hops: usize,
    },
    Aggregate {
        kind: String,
        group_by: String,
    },
}

#[derive(Debug, Clone, Copy)]
enum NeighborDir {
    Out,
    In,
    Both,
}

fn parse_query(input: &str) -> Result<QueryKind> {
    let input = input.trim();

    if input.starts_with("neighbors") || input.starts_with("neighbour") {
        return parse_neighbors(input);
    }
    if input.starts_with("path") {
        return parse_path(input);
    }
    if input.starts_with("count") || input.starts_with("aggregate") {
        return parse_aggregate(input);
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        bail!("empty query");
    }

    let mut limit = None;
    let mut sort = None;
    let mut filters = Vec::new();
    let mut expr_parts = Vec::new();

    for part in &parts[1..] {
        if let Some(stripped) = part.strip_prefix("limit=") {
            limit = stripped.parse().ok();
        } else if let Some(s) = part.strip_prefix("sort=") {
            if let Some(stripped) = s.strip_prefix('-') {
                sort = Some(SortSpec {
                    key: stripped.to_string(),
                    dir: SortDir::Desc,
                });
            } else {
                sort = Some(SortSpec {
                    key: s.to_string(),
                    dir: SortDir::Asc,
                });
            }
        } else {
            let (key, op, value) = parse_filter_token(part);
            expr_parts.push(part);
            filters.push(Filter { key, op, value });
        }
    }

    let expr = if filters.len() == 1 {
        Expr::Filter(filters.into_iter().next().unwrap())
    } else if filters.len() > 1 {
        let mut iter = filters.into_iter();
        let first = iter.next().unwrap();
        let second = iter.next().unwrap();
        let rest: Vec<_> = iter.collect();
        let mut expr = Expr::And(
            Box::new(Expr::Filter(first)),
            Box::new(Expr::Filter(second)),
        );
        for f in rest {
            expr = Expr::And(Box::new(expr), Box::new(Expr::Filter(f)));
        }
        expr
    } else {
        Expr::True
    };

    let kind = parts[0].to_lowercase();
    match kind.as_str() {
        "node" | "nodes" => Ok(QueryKind::Node { expr, limit, sort }),
        "edge" | "edges" => Ok(QueryKind::Edge { expr, limit, sort }),
        "note" | "notes" => Ok(QueryKind::Note { expr, limit, sort }),
        _ => bail!("unknown query kind: {}", kind),
    }
}

fn parse_filter_token(token: &str) -> (String, FilterOp, String) {
    let mut key = String::new();
    let mut op = FilterOp::Eq;
    let mut value = String::new();
    let mut in_key = true;

    for c in token.chars() {
        if in_key {
            if let Some(op_str) = ['=', '~', '!', '^'].iter().find(|&&oc| oc == c) {
                op = FilterOp::from_str(&format!("{}", op_str)).unwrap_or(FilterOp::Eq);
                in_key = false;
            } else {
                key.push(c);
            }
        } else if !c.is_whitespace() || !value.is_empty() {
            value.push(c);
        }
    }

    (key, op, value.trim_matches('"').to_string())
}

fn parse_neighbors(input: &str) -> Result<QueryKind> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("neighbors requires an id");
    }

    let mut id = String::new();
    let mut hops = 1;
    let mut direction = NeighborDir::Both;
    let mut limit = None;

    for part in &parts[1..] {
        if let Some(stripped) = part.strip_prefix("from=") {
            id = stripped.to_string();
        } else if let Some(stripped) = part.strip_prefix("hops=") {
            hops = stripped.parse().unwrap_or(1);
        } else if *part == "out" {
            direction = NeighborDir::Out;
        } else if *part == "in" {
            direction = NeighborDir::In;
        } else if *part == "both" {
            direction = NeighborDir::Both;
        } else if let Some(stripped) = part.strip_prefix("limit=") {
            limit = stripped.parse().ok();
        } else if id.is_empty() {
            id = part.to_string();
        }
    }

    if id.is_empty() {
        bail!("neighbors requires a node id");
    }

    Ok(QueryKind::Neighbors {
        id,
        hops,
        direction,
        limit,
    })
}

fn parse_path(input: &str) -> Result<QueryKind> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let mut from = String::new();
    let mut to = String::new();
    let mut max_hops = 3;

    for part in &parts[1..] {
        if let Some(stripped) = part.strip_prefix("from=") {
            from = stripped.to_string();
        } else if let Some(stripped) = part.strip_prefix("to=") {
            to = stripped.to_string();
        } else if let Some(stripped) = part.strip_prefix("hops=") {
            max_hops = stripped.parse().unwrap_or(3);
        } else if from.is_empty() {
            from = part.to_string();
        } else if to.is_empty() {
            to = part.to_string();
        }
    }

    if from.is_empty() || to.is_empty() {
        bail!("path requires from and to");
    }

    Ok(QueryKind::Path { from, to, max_hops })
}

fn parse_aggregate(input: &str) -> Result<QueryKind> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("aggregate requires a kind");
    }

    let mut kind = "node".to_string();
    let mut group_by = "type".to_string();

    for part in &parts[1..] {
        if let Some(stripped) = part.strip_prefix("by=") {
            group_by = stripped.to_string();
        } else if !["node", "edge", "note"].contains(&part.to_lowercase().as_str()) {
            kind = part.to_lowercase();
        }
    }

    Ok(QueryKind::Aggregate { kind, group_by })
}

pub fn render_query(graph: &GraphFile, input: &str) -> Result<String> {
    let response = query(graph, input)?;
    let mut lines = vec![format!("= kql {input}")];

    match &response {
        KqlResponse::Nodes { nodes, total } => {
            lines.push(format!("nodes: {} (total: {})", nodes.len(), total));
            for node in nodes {
                lines.push(format!("# {} | {} [{}]", node.id, node.name, node.r#type));
            }
        }
        KqlResponse::Edges { edges, total } => {
            lines.push(format!("edges: {} (total: {})", edges.len(), total));
            for edge in edges {
                lines.push(format!(
                    "- {} {} {}",
                    edge.source_id, edge.relation, edge.target_id
                ));
            }
        }
        KqlResponse::Notes { notes, total } => {
            lines.push(format!("notes: {} (total: {})", notes.len(), total));
            for note in notes {
                lines.push(format!("- {} | {}", note.id, note.node_id));
            }
        }
        KqlResponse::Neighbors { nodes, distance } => {
            lines.push(format!(
                "neighbors: {} (max distance: {})",
                nodes.len(),
                distance
            ));
            for node in nodes {
                lines.push(format!("# {} | {} [{}]", node.id, node.name, node.r#type));
            }
        }
        KqlResponse::Path { nodes, length } => {
            lines.push(format!("path: {} hops", length));
            for node in nodes {
                lines.push(format!("# {} | {}", node.id, node.name));
            }
        }
        KqlResponse::Aggregate { groups } => {
            lines.push(format!("aggregate: {} groups", groups.len()));
            for (key, count) in groups {
                lines.push(format!("- {}: {}", key, count));
            }
        }
    }

    Ok(format!("{}\n", lines.join("\n")))
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum KqlResponse {
    Nodes { nodes: Vec<Node>, total: usize },
    Edges { edges: Vec<Edge>, total: usize },
    Notes { notes: Vec<Note>, total: usize },
    Neighbors { nodes: Vec<Node>, distance: usize },
    Path { nodes: Vec<Node>, length: usize },
    Aggregate { groups: Vec<(String, usize)> },
}

pub fn query(graph: &GraphFile, input: &str) -> Result<KqlResponse> {
    let query = parse_query(input)?;
    match query {
        QueryKind::Node { expr, limit, sort } => {
            let all: Vec<Node> = graph
                .nodes
                .iter()
                .filter(|n| eval_node_expr(n, &expr))
                .cloned()
                .collect();
            let total = all.len();
            let nodes = apply_sort_limit(all, sort, limit);
            Ok(KqlResponse::Nodes { nodes, total })
        }
        QueryKind::Edge { expr, limit, sort } => {
            let all: Vec<Edge> = graph
                .edges
                .iter()
                .filter(|e| eval_edge_expr(e, &expr))
                .cloned()
                .collect();
            let total = all.len();
            let edges = apply_sort_limit_edge(all, sort, limit);
            Ok(KqlResponse::Edges { edges, total })
        }
        QueryKind::Note { expr, limit, sort } => {
            let all: Vec<Note> = graph
                .notes
                .iter()
                .filter(|n| eval_note_expr(n, &expr))
                .cloned()
                .collect();
            let total = all.len();
            let notes = apply_sort_limit_note(all, sort, limit);
            Ok(KqlResponse::Notes { notes, total })
        }
        QueryKind::Neighbors {
            id,
            hops,
            direction,
            limit,
        } => {
            let neighbors = find_neighbors(graph, &id, hops, direction, limit);
            Ok(KqlResponse::Neighbors {
                nodes: neighbors,
                distance: hops,
            })
        }
        QueryKind::Path { from, to, max_hops } => {
            let path = find_path(graph, &from, &to, max_hops)?;
            Ok(KqlResponse::Path {
                nodes: path,
                length: max_hops,
            })
        }
        QueryKind::Aggregate { kind, group_by } => {
            let groups = aggregate(graph, &kind, &group_by)?;
            Ok(KqlResponse::Aggregate { groups })
        }
    }
}

fn eval_node_expr(node: &Node, expr: &Expr) -> bool {
    match expr {
        Expr::True => true,
        Expr::Filter(f) => matches_node(node, f),
        Expr::And(left, right) => eval_node_expr(node, left) && eval_node_expr(node, right),
    }
}

fn eval_edge_expr(edge: &Edge, expr: &Expr) -> bool {
    match expr {
        Expr::True => true,
        Expr::Filter(f) => matches_edge(edge, f),
        Expr::And(left, right) => eval_edge_expr(edge, left) && eval_edge_expr(edge, right),
    }
}

fn eval_note_expr(note: &Note, expr: &Expr) -> bool {
    match expr {
        Expr::True => true,
        Expr::Filter(f) => matches_note(note, f),
        Expr::And(left, right) => eval_note_expr(note, left) && eval_note_expr(note, right),
    }
}

fn matches_node(node: &Node, filter: &Filter) -> bool {
    match filter.key.as_str() {
        "id" => compare(&node.id, filter),
        "type" => compare(&node.r#type, filter),
        "name" => compare(&node.name, filter),
        "description" => compare(&node.properties.description, filter),
        "domain" | "domain_area" => compare(&node.properties.domain_area, filter),
        "provenance" => compare(&node.properties.provenance, filter),
        "alias" => compare_list(&node.properties.alias, filter),
        "fact" | "key_fact" => compare_list(&node.properties.key_facts, filter),
        "source" | "source_file" => compare_list(&node.source_files, filter),
        "confidence" => {
            if let Some(c) = node.properties.confidence {
                compare(&format!("{}", c), filter)
            } else {
                false
            }
        }
        "importance" => compare(&node.properties.importance.to_string(), filter),
        _ => false,
    }
}

fn matches_edge(edge: &Edge, filter: &Filter) -> bool {
    match filter.key.as_str() {
        "source" | "source_id" => compare(&edge.source_id, filter),
        "relation" => compare(&edge.relation, filter),
        "target" | "target_id" => compare(&edge.target_id, filter),
        "detail" => compare(&edge.properties.detail, filter),
        _ => false,
    }
}

fn matches_note(note: &Note, filter: &Filter) -> bool {
    match filter.key.as_str() {
        "id" => compare(&note.id, filter),
        "node" | "node_id" => compare(&note.node_id, filter),
        "body" => compare(&note.body, filter),
        "tag" | "tags" => compare_list(&note.tags, filter),
        "author" => compare(&note.author, filter),
        "provenance" => compare(&note.provenance, filter),
        "source" | "source_file" => compare_list(&note.source_files, filter),
        _ => false,
    }
}

fn compare(value: &str, filter: &Filter) -> bool {
    match filter.op {
        FilterOp::Eq => value == filter.value,
        FilterOp::NotEq => value != filter.value,
        FilterOp::Contains => value.contains(&filter.value),
        FilterOp::Prefix => value.starts_with(&filter.value),
    }
}

fn compare_list(values: &[String], filter: &Filter) -> bool {
    values.iter().any(|value| compare(value, filter))
}

fn apply_sort_limit(
    mut nodes: Vec<Node>,
    sort: Option<SortSpec>,
    limit: Option<usize>,
) -> Vec<Node> {
    if let Some(s) = sort {
        match s.key.as_str() {
            "name" => nodes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.name.cmp(&b.name),
                SortDir::Desc => b.name.cmp(&a.name),
            }),
            "type" => nodes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.r#type.cmp(&b.r#type),
                SortDir::Desc => b.r#type.cmp(&a.r#type),
            }),
            "id" => nodes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.id.cmp(&b.id),
                SortDir::Desc => b.id.cmp(&a.id),
            }),
            _ => {}
        }
    }
    if let Some(l) = limit {
        nodes.truncate(l);
    }
    nodes
}

fn apply_sort_limit_edge(
    mut edges: Vec<Edge>,
    sort: Option<SortSpec>,
    limit: Option<usize>,
) -> Vec<Edge> {
    if let Some(s) = sort {
        match s.key.as_str() {
            "source" => edges.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.source_id.cmp(&b.source_id),
                SortDir::Desc => b.source_id.cmp(&a.source_id),
            }),
            "relation" => edges.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.relation.cmp(&b.relation),
                SortDir::Desc => b.relation.cmp(&a.relation),
            }),
            "target" => edges.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.target_id.cmp(&b.target_id),
                SortDir::Desc => b.target_id.cmp(&a.target_id),
            }),
            _ => {}
        }
    }
    if let Some(l) = limit {
        edges.truncate(l);
    }
    edges
}

fn apply_sort_limit_note(
    mut notes: Vec<Note>,
    sort: Option<SortSpec>,
    limit: Option<usize>,
) -> Vec<Note> {
    if let Some(s) = sort {
        match s.key.as_str() {
            "id" => notes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.id.cmp(&b.id),
                SortDir::Desc => b.id.cmp(&a.id),
            }),
            "node" => notes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.node_id.cmp(&b.node_id),
                SortDir::Desc => b.node_id.cmp(&a.node_id),
            }),
            "created" => notes.sort_by(|a, b| match s.dir {
                SortDir::Asc => a.created_at.cmp(&b.created_at),
                SortDir::Desc => b.created_at.cmp(&a.created_at),
            }),
            _ => {}
        }
    }
    if let Some(l) = limit {
        notes.truncate(l);
    }
    notes
}

fn find_neighbors(
    graph: &GraphFile,
    id: &str,
    hops: usize,
    direction: NeighborDir,
    limit: Option<usize>,
) -> Vec<Node> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut frontier: HashSet<String> = HashSet::new();
    frontier.insert(id.to_string());
    visited.insert(id.to_string());

    for _ in 0..hops {
        let mut next_frontier: HashSet<String> = HashSet::new();
        for current in &frontier {
            for edge in &graph.edges {
                let next_id = match direction {
                    NeighborDir::Out => {
                        if edge.source_id == *current {
                            Some(edge.target_id.clone())
                        } else {
                            None
                        }
                    }
                    NeighborDir::In => {
                        if edge.target_id == *current {
                            Some(edge.source_id.clone())
                        } else {
                            None
                        }
                    }
                    NeighborDir::Both => {
                        if edge.source_id == *current {
                            Some(edge.target_id.clone())
                        } else if edge.target_id == *current {
                            Some(edge.source_id.clone())
                        } else {
                            None
                        }
                    }
                };
                if let Some(nid) = next_id {
                    if visited.insert(nid.clone()) {
                        next_frontier.insert(nid);
                    }
                }
            }
        }
        frontier = next_frontier;
    }

    visited.remove(id);
    let mut nodes: Vec<Node> = visited
        .iter()
        .filter_map(|nid| graph.node_by_id(nid).cloned())
        .collect();

    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    if let Some(l) = limit {
        nodes.truncate(l);
    }
    nodes
}

fn find_path(graph: &GraphFile, from: &str, to: &str, max_hops: usize) -> Result<Vec<Node>> {
    if graph.node_by_id(from).is_none() {
        bail!("node not found: {}", from);
    }
    if graph.node_by_id(to).is_none() {
        bail!("node not found: {}", to);
    }

    let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();
    queue.push_back((from.to_string(), vec![from.to_string()]));
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(from.to_string());

    while let Some((current, path)) = queue.pop_front() {
        if path.len() > max_hops {
            continue;
        }
        if current == to {
            let mut nodes = Vec::new();
            for nid in &path {
                if let Some(node) = graph.node_by_id(nid) {
                    nodes.push(node.clone());
                }
            }
            return Ok(nodes);
        }

        for edge in &graph.edges {
            let next = if edge.source_id == current {
                Some(edge.target_id.clone())
            } else if edge.target_id == current {
                Some(edge.source_id.clone())
            } else {
                None
            };

            if let Some(nid) = next {
                if !visited.contains(&nid) {
                    visited.insert(nid.clone());
                    let mut new_path = path.clone();
                    new_path.push(nid.clone());
                    queue.push_back((nid, new_path));
                }
            }
        }
    }

    bail!(
        "no path found from {} to {} (max {} hops)",
        from,
        to,
        max_hops
    )
}

fn aggregate(graph: &GraphFile, kind: &str, group_by: &str) -> Result<Vec<(String, usize)>> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    match kind {
        "node" | "nodes" => {
            for node in &graph.nodes {
                let key = match group_by {
                    "type" => node.r#type.clone(),
                    "domain" | "domain_area" => {
                        if node.properties.domain_area.is_empty() {
                            "(none)".to_string()
                        } else {
                            node.properties.domain_area.clone()
                        }
                    }
                    "source" => {
                        if node.source_files.is_empty() {
                            "(none)".to_string()
                        } else {
                            node.source_files[0].clone()
                        }
                    }
                    "provenance" => {
                        if node.properties.provenance.is_empty() {
                            "(none)".to_string()
                        } else {
                            node.properties.provenance.clone()
                        }
                    }
                    "importance" => node.properties.importance.to_string(),
                    _ => node.r#type.clone(),
                };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        "edge" | "edges" => {
            for edge in &graph.edges {
                let key = match group_by {
                    "relation" => edge.relation.clone(),
                    "source" => edge.source_id.clone(),
                    "target" => edge.target_id.clone(),
                    _ => edge.relation.clone(),
                };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        "note" | "notes" => {
            for note in &graph.notes {
                let key = match group_by {
                    "node" => note.node_id.clone(),
                    "author" => {
                        if note.author.is_empty() {
                            "(none)".to_string()
                        } else {
                            note.author.clone()
                        }
                    }
                    "tag" => {
                        if note.tags.is_empty() {
                            "(none)".to_string()
                        } else {
                            note.tags[0].clone()
                        }
                    }
                    _ => "(all)".to_string(),
                };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        _ => bail!("unknown aggregate kind: {}", kind),
    }

    let mut groups: Vec<(String, usize)> = counts.into_iter().collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_without_filters_matches_all_nodes() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "concept:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "A".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });
        graph.nodes.push(Node {
            id: "concept:b".to_owned(),
            r#type: "Concept".to_owned(),
            name: "B".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });

        let result = query(&graph, "node").expect("query should succeed");
        match result {
            KqlResponse::Nodes { nodes, total } => {
                assert_eq!(total, 2);
                assert_eq!(nodes.len(), 2);
            }
            _ => panic!("expected node response"),
        }
    }

    #[test]
    fn query_without_filters_matches_all_edges() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "concept:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "A".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });
        graph.nodes.push(Node {
            id: "concept:b".to_owned(),
            r#type: "Concept".to_owned(),
            name: "B".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });
        graph.edges.push(Edge {
            source_id: "concept:a".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:b".to_owned(),
            properties: Default::default(),
        });

        let result = query(&graph, "edge").expect("query should succeed");
        match result {
            KqlResponse::Edges { edges, total } => {
                assert_eq!(total, 1);
                assert_eq!(edges.len(), 1);
            }
            _ => panic!("expected edge response"),
        }
    }

    #[test]
    fn query_without_filters_matches_all_notes() {
        let mut graph = GraphFile::new("test");
        graph.notes.push(Note {
            id: "note:1".to_owned(),
            node_id: "concept:a".to_owned(),
            body: "text".to_owned(),
            tags: vec!["tag".to_owned()],
            author: "tester".to_owned(),
            created_at: "2026-01-01".to_owned(),
            provenance: "U".to_owned(),
            source_files: vec![],
        });

        let result = query(&graph, "note").expect("query should succeed");
        match result {
            KqlResponse::Notes { notes, total } => {
                assert_eq!(total, 1);
                assert_eq!(notes.len(), 1);
            }
            _ => panic!("expected note response"),
        }
    }
}
