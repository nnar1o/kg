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
    GreaterEq,  // >=
    LessEq,     // <=
    Greater,    // >
    Less,       // <
}

impl FilterOp {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "=" => Some(FilterOp::Eq),
            "~" => Some(FilterOp::Contains),
            "!=" => Some(FilterOp::NotEq),
            "^" => Some(FilterOp::Prefix),
            ">=" => Some(FilterOp::GreaterEq),
            "<=" => Some(FilterOp::LessEq),
            ">" => Some(FilterOp::Greater),
            "<" => Some(FilterOp::Less),
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
        Expr::Filter(Filter {
            key: "id".to_string(),
            op: FilterOp::Eq,
            value: "*".to_string(),
        })
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
    let chars: Vec<char> = token.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if in_key {
            // Check for 2-char operators first (>-, <-)
            if c == '>' && i + 1 < chars.len() && chars[i + 1] == '=' {
                op = FilterOp::GreaterEq;
                in_key = false;
                i += 2;
                continue;
            }
            if c == '<' && i + 1 < chars.len() && chars[i + 1] == '=' {
                op = FilterOp::LessEq;
                in_key = false;
                i += 2;
                continue;
            }
            // Single char operators
            if let Some(op_str) = ['=', '~', '!', '^', '>', '<'].iter().find(|&&oc| oc == c) {
                op = FilterOp::from_str(&format!("{}", c)).unwrap_or(FilterOp::Eq);
                in_key = false;
                i += 1;
                continue;
            }
            key.push(c);
            i += 1;
        } else {
            if !c.is_whitespace() || !value.is_empty() {
                value.push(c);
            }
            i += 1;
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
                // Show validity period if set
                let validity = if !node.properties.valid_to.is_empty() {
                    format!(" (valid: {}-{})", node.properties.valid_from, node.properties.valid_to)
                } else if !node.properties.valid_from.is_empty() {
                    format!(" (valid_from: {})", node.properties.valid_from)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "# {} | {} [{}] @ {}{}",
                    node.id,
                    node.name,
                    node.r#type,
node.properties.created_at,
                    validity
                ));
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
        Expr::Filter(f) => matches_node(node, f),
        Expr::And(left, right) => eval_node_expr(node, left) && eval_node_expr(node, right),
    }
}

fn eval_edge_expr(edge: &Edge, expr: &Expr) -> bool {
    match expr {
        Expr::Filter(f) => matches_edge(edge, f),
        Expr::And(left, right) => eval_edge_expr(edge, left) && eval_edge_expr(edge, right),
    }
}

fn eval_note_expr(note: &Note, expr: &Expr) -> bool {
    match expr {
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
        "fact" | "key_fact" | "facts" => compare_list(&node.properties.key_facts, filter),
        "source" | "source_file" => compare_list(&node.source_files, filter),
        "confidence" => {
            if let Some(c) = node.properties.confidence {
                compare(&format!("{}", c), filter)
            } else {
                false
            }
        }
        "importance" => compare(&node.properties.importance.to_string(), filter),
        // Time-based filtering (ISO 8601 timestamps support lexicographic comparison)
        "created_at" | "created" | "createdat" => compare(&node.properties.created_at, filter),
        // updated_at removed - does not exist in NodeProperties
        // Temporal validity (fact/node validity period)
        "valid_from" | "validfrom" => compare(&node.properties.valid_from, filter),
        "valid_to" | "validto" => compare(&node.properties.valid_to, filter),
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
    let filter_val = filter.value.as_str();
    match filter.op {
        FilterOp::Eq => value == filter_val,
        FilterOp::NotEq => value != filter_val,
        FilterOp::Contains => value.contains(filter_val),
        FilterOp::Prefix => value.starts_with(filter_val),
        // Numeric/date comparisons using lexicographic string comparison
        // Works for ISO timestamps and importance floats
        FilterOp::GreaterEq => value >= filter_val,
        FilterOp::LessEq => value <= filter_val,
        FilterOp::Greater => value > filter_val,
        FilterOp::Less => value < filter_val,
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
            "created_at" | "created" | "createdat" => {
                nodes.sort_by(|a, b| match s.dir {
                    SortDir::Asc => a.properties.created_at.cmp(&b.properties.created_at),
                    SortDir::Desc => b.properties.created_at.cmp(&a.properties.created_at),
                })
            }
            "importance" => nodes.sort_by(|a, b| match s.dir {
                SortDir::Asc => {
                    if a.properties.importance == b.properties.importance {
                        std::cmp::Ordering::Equal
                    } else if a.properties.importance > b.properties.importance {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                }
                SortDir::Desc => {
                    if a.properties.importance == b.properties.importance {
                        std::cmp::Ordering::Equal
                    } else if a.properties.importance < b.properties.importance {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                }
            }),
            // updated_at removed - does not exist in NodeProperties
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

// ============================================================
// Unit tests for KQL parser and sorting
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, name: &str, node_type: &str, created_at: &str, importance: f64) -> Node {
        Node {
            id: id.to_string(),
            r#type: node_type.to_string(),
            name: name.to_string(),
            created_at: created_at.to_string(),
            updated_at: String::new(),
            properties: NodeProperties {
                description: String::new(),
                domain_area: String::new(),
                provenance: String::new(),
                confidence: None,
                importance,
                alias: vec![],
                key_facts: vec![],
            },
            source_files: vec![],
        }
    }

    fn parse_nodes(input: &str) -> QueryKind {
        parse_query(input).unwrap()
    }

    #[test]
    fn test_parse_query_basic_node() {
        let q = parse_nodes("node type=Concept");
        match q {
            QueryKind::Node { expr, .. } => {
                assert!(matches!(expr, Expr::Filter(_)));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_with_limit() {
        let q = parse_nodes("node limit=10");
        match q {
            QueryKind::Node { limit, .. } => {
                assert_eq!(limit, Some(10));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_sort_created_at_asc() {
        let q = parse_nodes("node sort=created_at");
        match q {
            QueryKind::Node { sort, .. } => {
                let s = sort.unwrap();
                assert_eq!(s.key, "created_at");
                assert!(matches!(s.dir, SortDir::Asc));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_sort_created_at_desc() {
        let q = parse_nodes("node sort=-created_at");
        match q {
            QueryKind::Node { sort, .. } => {
                let s = sort.unwrap();
                assert_eq!(s.key, "created_at");
                assert!(matches!(s.dir, SortDir::Desc));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_sort_importance_desc() {
        let q = parse_nodes("node sort=-importance");
        match q {
            QueryKind::Node { sort, .. } => {
                let s = sort.unwrap();
                assert_eq!(s.key, "importance");
                assert!(matches!(s.dir, SortDir::Desc));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_sort_updated_at() {
        for field in &["updated_at", "updated", "updatedat"] {
            let q = parse_nodes(&format!("node sort={field}"));
            match q {
                QueryKind::Node { sort, .. } => {
                    let s = sort.unwrap();
                    assert_eq!(s.key, *field);
                }
                _ => panic!("expected Node query for field {field}"),
            }
        }
    }

    #[test]
    fn test_parse_query_alias_created() {
        for field in &["created_at", "created", "createdat"] {
            let q = parse_nodes(&format!("node sort={field}"));
            match q {
                QueryKind::Node { sort, .. } => {
                    let s = sort.unwrap();
                    assert_eq!(s.key, *field);
                }
                _ => panic!("expected Node query for alias {field}"),
            }
        }
    }

    #[test]
    fn test_parse_query_combined() {
        let q = parse_nodes("node type=Bug sort=-created_at limit=5");
        match q {
            QueryKind::Node {
                limit,
                sort,
                expr,
            } => {
                assert_eq!(limit, Some(5));
                let s = sort.unwrap();
                assert_eq!(s.key, "created_at");
                assert!(matches!(s.dir, SortDir::Desc));
                assert!(matches!(expr, Expr::Filter(_)));
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_parse_query_invalid_kind() {
        let result = parse_query("foo type=Concept");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_query_unknown_sort_passes() {
        // Unknown sort keys should parse without error (sort is skipped in apply)
        let q = parse_nodes("node sort=unknown_field");
        match q {
            QueryKind::Node { sort, .. } => {
                let s = sort.unwrap();
                assert_eq!(s.key, "unknown_field");
            }
            _ => panic!("expected Node query"),
        }
    }

    #[test]
    fn test_apply_sort_limit_created_at_asc() {
        let nodes = vec![
            make_node("a", "A", "Concept", "2026-04-20T10:00:00Z", 0.5),
            make_node("b", "B", "Concept", "2026-04-21T10:00:00Z", 0.5),
            make_node("c", "C", "Concept", "2026-04-19T10:00:00Z", 0.5),
        ];
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "created_at".to_string(),
                dir: SortDir::Asc,
            }),
            None,
        );
        assert_eq!(result[0].id, "c"); // oldest first
        assert_eq!(result[2].id, "b"); // newest last
    }

    #[test]
    fn test_apply_sort_limit_created_at_desc() {
        let nodes = vec![
            make_node("a", "A", "Concept", "2026-04-20T10:00:00Z", 0.5),
            make_node("b", "B", "Concept", "2026-04-21T10:00:00Z", 0.5),
            make_node("c", "C", "Concept", "2026-04-19T10:00:00Z", 0.5),
        ];
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "created_at".to_string(),
                dir: SortDir::Desc,
            }),
            None,
        );
        assert_eq!(result[0].id, "b"); // newest first
        assert_eq!(result[2].id, "c"); // oldest last
    }

    #[test]
    fn test_apply_sort_limit_importance_desc() {
        let nodes = vec![
            make_node("a", "A", "Feature", "2026-04-20T10:00:00Z", 0.3),
            make_node("b", "B", "Feature", "2026-04-20T10:00:00Z", 0.9),
            make_node("c", "C", "Feature", "2026-04-20T10:00:00Z", 0.6),
        ];
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "importance".to_string(),
                dir: SortDir::Desc,
            }),
            None,
        );
        assert_eq!(result[0].id, "b"); // importance 0.9 first
        assert_eq!(result[1].id, "c"); // importance 0.6 second
        assert_eq!(result[2].id, "a"); // importance 0.3 last
    }

    #[test]
    fn test_apply_sort_limit_importance_asc() {
        let nodes = vec![
            make_node("a", "A", "Feature", "2026-04-20T10:00:00Z", 0.8),
            make_node("b", "B", "Feature", "2026-04-20T10:00:00Z", 0.2),
            make_node("c", "C", "Feature", "2026-04-20T10:00:00Z", 0.5),
        ];
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "importance".to_string(),
                dir: SortDir::Asc,
            }),
            None,
        );
        assert_eq!(result[0].id, "b"); // lowest first
        assert_eq!(result[2].id, "a"); // highest last
    }

    #[test]
    fn test_apply_sort_limit_with_limit() {
        let nodes = vec![
            make_node("a", "A", "Concept", "2026-04-22T10:00:00Z", 0.5),
            make_node("b", "B", "Concept", "2026-04-21T10:00:00Z", 0.5),
            make_node("c", "C", "Concept", "2026-04-20T10:00:00Z", 0.5),
            make_node("d", "D", "Concept", "2026-04-19T10:00:00Z", 0.5),
        ];
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "created_at".to_string(),
                dir: SortDir::Desc,
            }),
            Some(2),
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "a"); // newest
        assert_eq!(result[1].id, "b"); // second newest
    }

    #[test]
    fn test_apply_sort_limit_unknown_key_no_panic() {
        let nodes = vec![
            make_node("a", "A", "Concept", "2026-04-20T10:00:00Z", 0.5),
            make_node("b", "B", "Concept", "2026-04-21T10:00:00Z", 0.5),
        ];
        // Unknown key should not panic, just return unsorted
        let result = apply_sort_limit(
            nodes,
            Some(SortSpec {
                key: "unknown_field".to_string(),
                dir: SortDir::Asc,
            }),
            None,
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_neighbors_basic() {
        let q = parse_query("neighbors concept:refrigerator").unwrap();
        match q {
            QueryKind::Neighbors { id, hops, .. } => {
                assert_eq!(id, "concept:refrigerator");
                assert_eq!(hops, 1);
            }
            _ => panic!("expected Neighbors query"),
        }
    }

    #[test]
    fn test_parse_neighbors_with_hops() {
        let q = parse_query("neighbors concept:refrigerator hops=3").unwrap();
        match q {
            QueryKind::Neighbors { id, hops, .. } => {
                assert_eq!(id, "concept:refrigerator");
                assert_eq!(hops, 3);
            }
            _ => panic!("expected Neighbors query"),
        }
    }

    #[test]
    fn test_parse_path_basic() {
        let q = parse_query("path from=concept:foo to=concept:bar").unwrap();
        match q {
            QueryKind::Path { from, to, .. } => {
                assert_eq!(from, "concept:foo");
                assert_eq!(to, "concept:bar");
            }
            _ => panic!("expected Path query"),
        }
    }

    #[test]
    fn test_parse_aggregate_node_by_type() {
        let q = parse_query("count node by=type").unwrap();
        match q {
            QueryKind::Aggregate {
                kind,
                group_by,
            } => {
                assert_eq!(kind, "node");
                assert_eq!(group_by, "type");
            }
            _ => panic!("expected Aggregate query"),
        }
    }

    #[test]
    fn test_parse_edge_query() {
        let q = parse_nodes("edge relation=DEPENDS_ON");
        match q {
            QueryKind::Edge { expr, .. } => {
                assert!(matches!(expr, Expr::Filter(_)));
            }
            _ => panic!("expected Edge query"),
        }
    }

    #[test]
    fn test_parse_note_query() {
        let q = parse_nodes("note tag=bug");
        match q {
            QueryKind::Note { expr, .. } => {
                assert!(matches!(expr, Expr::Filter(_)));
            }
            _ => panic!("expected Note query"),
        }
    }

    // ============================================================
    // Range query tests (temporal filtering)
    // ============================================================

    #[test]
    fn test_parse_filter_token_greater_eq() {
        let (key, op, value) = parse_filter_token("created_at>=2026-04-20");
        assert_eq!(key, "created_at");
        assert!(matches!(op, FilterOp::GreaterEq));
        assert_eq!(value, "2026-04-20");
    }

    #[test]
    fn test_parse_filter_token_less_eq() {
        let (key, op, value) = parse_filter_token("created_at<=2026-04-20");
        assert_eq!(key, "created_at");
        assert!(matches!(op, FilterOp::LessEq));
        assert_eq!(value, "2026-04-20");
    }

    #[test]
    fn test_parse_filter_token_greater() {
        let (key, op, value) = parse_filter_token("importance>0.8");
        assert_eq!(key, "importance");
        assert!(matches!(op, FilterOp::Greater));
        assert_eq!(value, "0.8");
    }

    #[test]
    fn test_parse_filter_token_less() {
        let (key, op, value) = parse_filter_token("importance<0.5");
        assert_eq!(key, "importance");
        assert!(matches!(op, FilterOp::Less));
        assert_eq!(value, "0.5");
    }

    #[test]
    fn test_parse_filter_token_timestamp_range() {
        // created_after=... is expanded to created_at>=...
        let (key, op, value) = parse_filter_token("created_at>=2026-04-20");
        assert_eq!(key, "created_at");
        assert!(matches!(op, FilterOp::GreaterEq));
    }

    #[test]
    fn test_compare_iso_timestamps_lexicographic() {
        // ISO 8601 timestamps compare correctly lexicographically
        let filter = Filter {
            key: "created_at".to_string(),
            op: FilterOp::GreaterEq,
            value: "2026-04-20T00:00:00Z".to_string(),
        };

        // Earlier timestamp should not match >=
        assert!(!compare("2026-04-19T10:00:00Z", &filter));

        // Same timestamp should match
        assert!(compare("2026-04-20T00:00:00Z", &filter));

        // Later timestamp should match
        assert!(compare("2026-04-21T00:00:00Z", &filter));
    }

    #[test]
    fn test_compare_iso_timestamps_range() {
        // Filter for 2026-04-20 to 2026-04-21
        let filter_start = Filter {
            key: "created_at".to_string(),
            op: FilterOp::GreaterEq,
            value: "2026-04-20".to_string(),
        };
        let filter_end = Filter {
            key: "created_at".to_string(),
            op: FilterOp::Less,
            value: "2026-04-21".to_string(),
        };

        // Before range - should not match
        assert!(!compare("2026-04-19T00:00:00Z", &filter_start));
        assert!(compare("2026-04-19T00:00:00Z", &filter_end));

        // In range - should match both
        assert!(compare("2026-04-20T12:00:00Z", &filter_start));
        assert!(!compare("2026-04-20T12:00:00Z", &filter_end));

        // After range - should match start only
        assert!(compare("2026-04-21T00:00:00Z", &filter_start));
        assert!(compare("2026-04-21T00:00:00Z", &filter_end));
    }

    #[test]
    fn test_compare_importance_ranges() {
        let filter = Filter {
            key: "importance".to_string(),
            op: FilterOp::Greater,
            value: "0.7".to_string(),
        };

        assert!(!compare("0.5", &filter));
        assert!(!compare("0.7", &filter));
        assert!(compare("0.8", &filter));
        assert!(compare("1.0", &filter));
    }

    #[test]
    fn test_apply_sort_limit_created_at_desc_then_filter_by_range() {
        let nodes = vec![
            make_node("a", "A", "Feature", "2026-04-22T10:00:00Z", 0.5),
            make_node("b", "B", "Feature", "2026-04-21T10:00:00Z", 0.8),
            make_node("c", "C", "Feature", "2026-04-20T10:00:00Z", 0.6),
            make_node("d", "D", "Feature", "2026-04-19T10:00:00Z", 0.3),
        ];

        // Filter for nodes created on or after 2026-04-20
        let filter = Filter {
            key: "created_at".to_string(),
            op: FilterOp::GreaterEq,
            value: "2026-04-20".to_string(),
        };

        let filtered: Vec<Node> = nodes
            .into_iter()
            .filter(|n| matches_node(n, &filter))
            .collect();

        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].id, "c"); // oldest in range first (before sort)
        assert_eq!(filtered[1].id, "b");
        assert_eq!(filtered[2].id, "a");
    }

    #[test]
    fn test_iso_timestamp_prefix_matching() {
        // Using prefix match to find all nodes from a specific day
        let filter = Filter {
            key: "created_at".to_string(),
            op: FilterOp::Prefix,
            value: "2026-04-20".to_string(),
        };

        assert!(compare("2026-04-20T10:00:00Z", &filter));
        assert!(compare("2026-04-20T23:59:59Z", &filter));
        assert!(!compare("2026-04-21T00:00:00Z", &filter));
    }

    // ============================================================
    // Temporal validity tests (valid_from/valid_to)
    // ============================================================

    fn make_node_with_validity(
        id: &str,
        name: &str,
        node_type: &str,
        created_at: &str,
        valid_from: &str,
        valid_to: &str,
    ) -> Node {
        Node {
            id: id.to_string(),
            r#type: node_type.to_string(),
            name: name.to_string(),
            created_at: created_at.to_string(),
            updated_at: String::new(),
            properties: NodeProperties {
                description: String::new(),
                domain_area: String::new(),
                provenance: String::new(),
                confidence: None,
                importance: 0.5,
                alias: vec![],
                key_facts: vec![],
                valid_from: valid_from.to_string(),
                valid_to: valid_to.to_string(),
            },
            source_files: vec![],
        }
    }

    #[test]
    fn test_matches_node_valid_from() {
        let node = make_node_with_validity("bug:x", "Bug X", "Bug", "2026-04-20T00:00:00Z", "2026-04-01", "");
        let filter = Filter {
            key: "valid_from".to_string(),
            op: FilterOp::GreaterEq,
            value: "2026-04-15".to_string(),
        };
        assert!(matches_node(&node, &filter));

        let filter_old = Filter {
            key: "valid_from".to_string(),
            op: FilterOp::Less,
            value: "2026-03-01".to_string(),
        };
        assert!(!matches_node(&node, &filter_old));
    }

    #[test]
    fn test_matches_node_valid_to() {
        // Node with validity period that has expired
        let node = make_node_with_validity("bug:x", "Bug X", "Bug", "2026-04-20T00:00:00Z", "2026-01-01", "2026-04-01");
        let filter = Filter {
            key: "valid_to".to_string(),
            op: FilterOp::Less,
            value: "2026-04-20".to_string(),
        };
        assert!(matches_node(&node, &filter));

        // Node still valid (valid_to is empty)
        let node_valid = make_node_with_validity("bug:y", "Bug Y", "Bug", "2026-04-20T00:00:00Z", "", "");
        assert!(!matches_node(&node_valid, &filter));
    }

    #[test]
    fn test_filter_currently_valid_nodes() {
        // Currently valid = valid_to is empty OR valid_to >= now
        let nodes = vec![
            make_node_with_validity("a", "A", "Bug", "2026-04-20", "", ""), // always valid
            make_node_with_validity("b", "B", "Bug", "2026-04-20", "", "2026-12-31"), // still valid
            make_node_with_validity("c", "C", "Bug", "2026-04-20", "", "2026-04-01"), // expired
            make_node_with_validity("d", "D", "Bug", "2026-04-20", "2026-01-01", "2026-04-01"), // expired
        ];

        let currently_valid: Vec<_> = nodes
            .into_iter()
            .filter(|n| {
                let valid_to = &n.properties.valid_to;
                valid_to.is_empty() || valid_to >= "2026-04-20"
            })
            .collect();

        assert_eq!(currently_valid.len(), 2);
        assert_eq!(currently_valid[0].id, "a");
        assert_eq!(currently_valid[1].id, "b");
    }

    #[test]
    fn test_filter_invalidated_nodes() {
        // Nodes that are no longer valid (valid_to is set and < now)
        let nodes = vec![
            make_node_with_validity("a", "A", "Bug", "2026-04-20", "", ""),
            make_node_with_validity("b", "B", "Bug", "2026-04-20", "", "2026-12-31"),
            make_node_with_validity("c", "C", "Bug", "2026-04-20", "", "2026-04-01"),
        ];

        let invalidated: Vec<_> = nodes
            .into_iter()
            .filter(|n| !n.properties.valid_to.is_empty() && n.properties.valid_to < "2026-04-20")
            .collect();

        assert_eq!(invalidated.len(), 1);
        assert_eq!(invalidated[0].id, "c");
    }

    #[test]
    fn test_filter_valid_in_period() {
        // Nodes valid in a specific period (e.g., checking state as of 2026-04-15)
        let nodes = vec![
            make_node_with_validity("a", "A", "Bug", "2026-04-20", "", ""), // always valid
            make_node_with_validity("b", "B", "Bug", "2026-04-20", "2026-01-01", "2026-04-01"), // valid until April
            make_node_with_validity("c", "C", "Bug", "2026-04-20", "2026-05-01", ""), // valid from May
        ];

        // Query: what was valid as of 2026-04-15?
        let as_of_check = "2026-04-15";
        let valid_as_of: Vec<_> = nodes
            .into_iter()
            .filter(|n| {
                let from_ok = n.properties.valid_from.is_empty() || n.properties.valid_from <= as_of_check;
                let to_ok = n.properties.valid_to.is_empty() || n.properties.valid_to > as_of_check;
                from_ok && to_ok
            })
            .collect();

        assert_eq!(valid_as_of.len(), 2);
        assert_eq!(valid_as_of[0].id, "a");
        assert_eq!(valid_as_of[1].id, "b");
    }
}
