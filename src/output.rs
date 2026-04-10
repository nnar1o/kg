use std::collections::{HashMap, HashSet, VecDeque};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::graph::{Edge, GraphFile, Node, Note};
use crate::index::Bm25Index;

const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;
const DEFAULT_TARGET_CHARS: usize = 1400;
const MIN_TARGET_CHARS: usize = 300;
const MAX_TARGET_CHARS: usize = 12_000;
const FUZZY_NEIGHBOR_CONTEXT_CAP: u32 = 220;
const FUZZY_NO_PRIMARY_CONTEXT_DIVISOR: u32 = 3;
const FUZZY_DESCRIPTION_WEIGHT: u32 = 2;
const FUZZY_FACT_WEIGHT: u32 = 2;
const FUZZY_NOTE_BODY_WEIGHT: u32 = 1;
const FUZZY_NOTE_TAG_WEIGHT: u32 = 2;
const BM25_PHRASE_MATCH_BOOST: i64 = 120;
const BM25_TOKEN_MATCH_BOOST: i64 = 45;
const BM25_ID_WEIGHT: usize = 4;
const BM25_NAME_WEIGHT: usize = 3;
const BM25_ALIAS_WEIGHT: usize = 2;
const BM25_DESCRIPTION_WEIGHT: usize = 2;
const BM25_FACT_WEIGHT: usize = 2;
const BM25_NOTE_BODY_WEIGHT: usize = 1;
const BM25_NOTE_TAG_WEIGHT: usize = 1;
const BM25_NEIGHBOR_WEIGHT: usize = 1;
const IMPORTANCE_NEUTRAL: i64 = 4;
const IMPORTANCE_STEP_BOOST: i64 = 22;
const SCORE_META_MAX_RATIO: f64 = 0.35;
const SCORE_META_MIN_CAP: i64 = 30;
const SCORE_META_MAX_CAP: i64 = 240;

#[derive(Debug, Clone, Copy)]
pub enum FindMode {
    Fuzzy,
    Bm25,
}

#[derive(Clone, Copy)]
struct ScoredNode<'a> {
    score: i64,
    node: &'a Node,
    breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Copy)]
struct ScoreBreakdown {
    raw_relevance: f64,
    normalized_relevance: i64,
    lexical_boost: i64,
    feedback_boost: i64,
    importance_boost: i64,
    authority_raw: i64,
    authority_applied: i64,
    authority_cap: i64,
}

struct RawCandidate<'a> {
    node: &'a Node,
    raw_relevance: f64,
    lexical_boost: i64,
}

struct FindQueryContext<'a> {
    notes_by_node: HashMap<&'a str, Vec<&'a Note>>,
    neighbors_by_node: HashMap<&'a str, Vec<&'a Node>>,
}

impl<'a> FindQueryContext<'a> {
    fn build(graph: &'a GraphFile) -> Self {
        let node_by_id: HashMap<&'a str, &'a Node> = graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();

        let mut notes_by_node: HashMap<&'a str, Vec<&'a Note>> = HashMap::new();
        for note in &graph.notes {
            notes_by_node
                .entry(note.node_id.as_str())
                .or_default()
                .push(note);
        }

        let mut neighbors_by_node: HashMap<&'a str, Vec<&'a Node>> = HashMap::new();
        for edge in &graph.edges {
            if let (Some(source), Some(target)) = (
                node_by_id.get(edge.source_id.as_str()),
                node_by_id.get(edge.target_id.as_str()),
            ) {
                neighbors_by_node
                    .entry(source.id.as_str())
                    .or_default()
                    .push(*target);
                neighbors_by_node
                    .entry(target.id.as_str())
                    .or_default()
                    .push(*source);
            }
        }

        for neighbors in neighbors_by_node.values_mut() {
            neighbors.sort_by(|left, right| left.id.cmp(&right.id));
            neighbors.dedup_by(|left, right| left.id == right.id);
        }

        Self {
            notes_by_node,
            neighbors_by_node,
        }
    }

    fn notes_for(&self, node_id: &str) -> &[&'a Note] {
        self.notes_by_node
            .get(node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn neighbors_for(&self, node_id: &str) -> &[&'a Node] {
        self.neighbors_by_node
            .get(node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone)]
pub struct ScoreBreakdownResult {
    pub raw_relevance: f64,
    pub normalized_relevance: i64,
    pub lexical_boost: i64,
    pub feedback_boost: i64,
    pub importance_boost: i64,
    pub authority_raw: i64,
    pub authority_applied: i64,
    pub authority_cap: i64,
}

#[derive(Debug, Clone)]
pub struct ScoredNodeResult {
    pub score: i64,
    pub node: Node,
    pub breakdown: ScoreBreakdownResult,
}

pub fn render_find(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    mode: FindMode,
    full: bool,
) -> String {
    render_find_with_index(
        graph,
        queries,
        limit,
        include_features,
        mode,
        full,
        false,
        None,
    )
}

pub fn render_find_with_index(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    mode: FindMode,
    full: bool,
    debug_score: bool,
    index: Option<&Bm25Index>,
) -> String {
    let mut sections = Vec::new();
    for query in queries {
        let matches = find_all_matches_with_index(graph, query, include_features, mode, index);
        let total = matches.len();
        let visible: Vec<_> = matches.into_iter().take(limit).collect();
        let shown = visible.len();
        let mut lines = vec![render_result_header(query, shown, total)];
        for scored in visible {
            lines.push(render_scored_node_block(graph, &scored, full, debug_score));
        }
        push_limit_omission_line(&mut lines, shown, total);
        sections.push(lines.join("\n"));
    }
    format!("{}\n", sections.join("\n\n"))
}

pub fn find_nodes(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    mode: FindMode,
) -> Vec<Node> {
    find_matches_with_index(graph, query, limit, include_features, mode, None)
        .into_iter()
        .map(|item| item.node.clone())
        .collect()
}

pub fn find_nodes_with_index(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> Vec<Node> {
    find_matches_with_index(graph, query, limit, include_features, mode, index)
        .into_iter()
        .map(|item| item.node.clone())
        .collect()
}

pub fn find_nodes_and_total_with_index(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> (usize, Vec<Node>) {
    let matches = find_all_matches_with_index(graph, query, include_features, mode, index);
    let total = matches.len();
    let nodes = matches
        .into_iter()
        .take(limit)
        .map(|item| item.node.clone())
        .collect();
    (total, nodes)
}

pub fn find_scored_nodes_and_total_with_index(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> (usize, Vec<ScoredNodeResult>) {
    let matches = find_all_matches_with_index(graph, query, include_features, mode, index);
    let total = matches.len();
    let nodes = matches
        .into_iter()
        .take(limit)
        .map(|item| ScoredNodeResult {
            score: item.score,
            node: item.node.clone(),
            breakdown: ScoreBreakdownResult {
                raw_relevance: item.breakdown.raw_relevance,
                normalized_relevance: item.breakdown.normalized_relevance,
                lexical_boost: item.breakdown.lexical_boost,
                feedback_boost: item.breakdown.feedback_boost,
                importance_boost: item.breakdown.importance_boost,
                authority_raw: item.breakdown.authority_raw,
                authority_applied: item.breakdown.authority_applied,
                authority_cap: item.breakdown.authority_cap,
            },
        })
        .collect();
    (total, nodes)
}

pub fn count_find_results(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    mode: FindMode,
) -> usize {
    count_find_results_with_index(graph, queries, limit, include_features, mode, None)
}

pub fn count_find_results_with_index(
    graph: &GraphFile,
    queries: &[String],
    _limit: usize,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> usize {
    let mut total = 0;
    for query in queries {
        total += find_all_matches_with_index(graph, query, include_features, mode, index).len();
    }
    total
}

pub fn render_node(graph: &GraphFile, node: &Node, full: bool) -> String {
    format!("{}\n", render_node_block(graph, node, full))
}

pub fn render_node_adaptive(graph: &GraphFile, node: &Node, target_chars: Option<usize>) -> String {
    let target = clamp_target_chars(target_chars);
    let full = format!("{}\n", render_node_block(graph, node, true));
    if fits_target_chars(&full, target) {
        return full;
    }
    let mut candidates = Vec::new();
    for (depth, detail, edge_cap) in [
        (0usize, DetailLevel::Rich, 8usize),
        (1usize, DetailLevel::Rich, 8usize),
        (2usize, DetailLevel::Rich, 6usize),
        (2usize, DetailLevel::Compact, 6usize),
        (2usize, DetailLevel::Minimal, 2usize),
    ] {
        let rendered = render_single_node_candidate(graph, node, depth, detail, edge_cap);
        candidates.push(Candidate {
            rendered,
            depth,
            detail,
            shown_nodes: 1 + depth,
        });
    }
    pick_best_candidate(candidates, target)
}

pub fn render_find_adaptive_with_index(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    mode: FindMode,
    target_chars: Option<usize>,
    debug_score: bool,
    index: Option<&Bm25Index>,
) -> String {
    let target = clamp_target_chars(target_chars);
    let mut sections = Vec::new();
    for query in queries {
        let matches = find_all_matches_with_index(graph, query, include_features, mode, index);
        let total = matches.len();
        let visible: Vec<_> = matches.into_iter().take(limit).collect();
        let section = if visible.len() == 1 {
            render_single_result_section(graph, query, &visible[0], total, target, debug_score)
        } else {
            render_multi_result_section(graph, query, &visible, total, target, debug_score)
        };
        sections.push(section);
    }
    format!("{}\n", sections.join("\n\n"))
}

#[derive(Clone, Copy)]
enum DetailLevel {
    Rich,
    Compact,
    Minimal,
}

struct Candidate {
    rendered: String,
    depth: usize,
    detail: DetailLevel,
    shown_nodes: usize,
}

impl DetailLevel {
    fn utility_bonus(self) -> usize {
        match self {
            DetailLevel::Rich => 20,
            DetailLevel::Compact => 10,
            DetailLevel::Minimal => 0,
        }
    }
}

fn clamp_target_chars(target_chars: Option<usize>) -> usize {
    target_chars
        .unwrap_or(DEFAULT_TARGET_CHARS)
        .clamp(MIN_TARGET_CHARS, MAX_TARGET_CHARS)
}

fn render_single_result_section(
    graph: &GraphFile,
    query: &str,
    node: &ScoredNode<'_>,
    total_available: usize,
    target: usize,
    debug_score: bool,
) -> String {
    let header = render_result_header(query, 1, total_available);
    let full = render_single_result_candidate(
        graph,
        &header,
        node,
        total_available,
        0,
        DetailLevel::Rich,
        8,
        true,
        debug_score,
    );
    if fits_target_chars(&full, target) {
        return full.trim_end().to_owned();
    }
    let mut candidates = Vec::new();
    for (depth, detail, edge_cap) in [
        (0usize, DetailLevel::Rich, 8usize),
        (1usize, DetailLevel::Rich, 8usize),
        (2usize, DetailLevel::Rich, 6usize),
        (2usize, DetailLevel::Compact, 6usize),
        (2usize, DetailLevel::Minimal, 2usize),
    ] {
        candidates.push(Candidate {
            rendered: render_single_result_candidate(
                graph,
                &header,
                node,
                total_available,
                depth,
                detail,
                edge_cap,
                false,
                debug_score,
            ),
            depth,
            detail,
            shown_nodes: 1 + depth,
        });
    }
    pick_best_candidate(candidates, target)
        .trim_end()
        .to_owned()
}

fn render_multi_result_section(
    graph: &GraphFile,
    query: &str,
    nodes: &[ScoredNode<'_>],
    total_available: usize,
    target: usize,
    debug_score: bool,
) -> String {
    let visible_total = nodes.len();
    let full = render_full_result_section(graph, query, nodes, total_available, debug_score);
    if fits_target_chars(&full, target) {
        return full;
    }
    let mut candidates = Vec::new();
    let full_cap = visible_total;
    let mid_cap = full_cap.min(5);
    let low_cap = full_cap.min(3);

    for (detail, edge_cap, result_cap, depth) in [
        (DetailLevel::Rich, 4usize, full_cap.min(4), 0usize),
        (DetailLevel::Compact, 3usize, full_cap, 0usize),
        (DetailLevel::Rich, 2usize, mid_cap, 1usize),
        (DetailLevel::Compact, 1usize, full_cap, 0usize),
        (DetailLevel::Minimal, 1usize, mid_cap, 0usize),
        (DetailLevel::Minimal, 0usize, low_cap, 0usize),
        (DetailLevel::Minimal, 0usize, low_cap.min(2), 1usize),
    ] {
        let shown = result_cap.min(nodes.len());
        let mut lines = vec![render_result_header(query, shown, total_available)];
        for node in nodes.iter().take(shown) {
            lines.extend(render_scored_node_candidate_lines(
                graph,
                node,
                0,
                detail,
                edge_cap,
                debug_score,
            ));
            if depth > 0 {
                lines.extend(render_neighbor_layers(graph, node.node, depth, detail));
            }
        }
        if visible_total > shown {
            lines.push(format!("... +{} more nodes omitted", visible_total - shown));
        }
        push_limit_omission_line(&mut lines, visible_total, total_available);
        candidates.push(Candidate {
            rendered: format!("{}\n", lines.join("\n")),
            depth,
            detail,
            shown_nodes: shown,
        });
    }

    pick_best_candidate(candidates, target)
        .trim_end()
        .to_owned()
}

fn pick_best_candidate(candidates: Vec<Candidate>, target: usize) -> String {
    let lower = (target as f64 * 0.7) as usize;
    let mut best: Option<(usize, usize, usize, usize, String)> = None;

    for candidate in candidates {
        let chars = candidate.rendered.chars().count();
        let overshoot = chars.saturating_sub(target);
        let undershoot = lower.saturating_sub(chars);
        let penalty = overshoot.saturating_mul(10).saturating_add(undershoot);
        let utility = candidate
            .depth
            .saturating_mul(100)
            .saturating_add(candidate.shown_nodes.saturating_mul(5))
            .saturating_add(candidate.detail.utility_bonus());

        let entry = (
            penalty,
            overshoot,
            usize::MAX - utility,
            usize::MAX - chars,
            candidate.rendered,
        );
        if best.as_ref().is_none_or(|current| {
            entry.0 < current.0
                || (entry.0 == current.0 && entry.1 < current.1)
                || (entry.0 == current.0 && entry.1 == current.1 && entry.2 < current.2)
                || (entry.0 == current.0
                    && entry.1 == current.1
                    && entry.2 == current.2
                    && entry.3 < current.3)
        }) {
            best = Some(entry);
        }
    }

    best.map(|item| item.4).unwrap_or_else(|| "\n".to_owned())
}

fn render_full_result_section(
    graph: &GraphFile,
    query: &str,
    nodes: &[ScoredNode<'_>],
    total_available: usize,
    debug_score: bool,
) -> String {
    let mut lines = vec![render_result_header(query, nodes.len(), total_available)];
    for node in nodes {
        lines.push(render_scored_node_block(graph, node, true, debug_score));
    }
    push_limit_omission_line(&mut lines, nodes.len(), total_available);
    lines.join("\n")
}

fn render_result_header(query: &str, shown: usize, total: usize) -> String {
    let query = escape_cli_text(query);
    if shown < total {
        format!("? {query} ({shown}/{total})")
    } else {
        format!("? {query} ({total})")
    }
}

fn push_limit_omission_line(lines: &mut Vec<String>, shown: usize, total: usize) {
    let omitted = total.saturating_sub(shown);
    if omitted > 0 {
        lines.push(format!("... {omitted} more nodes omitted by limit"));
    }
}

fn fits_target_chars(rendered: &str, target: usize) -> bool {
    rendered.chars().count() <= target
}

fn render_single_node_candidate(
    graph: &GraphFile,
    node: &Node,
    depth: usize,
    detail: DetailLevel,
    edge_cap: usize,
) -> String {
    let lines = render_single_node_candidate_lines(graph, node, depth, detail, edge_cap);
    format!("{}\n", lines.join("\n"))
}

fn render_single_result_candidate(
    graph: &GraphFile,
    header: &str,
    node: &ScoredNode<'_>,
    total_available: usize,
    depth: usize,
    detail: DetailLevel,
    edge_cap: usize,
    full: bool,
    debug_score: bool,
) -> String {
    let mut lines = vec![header.to_owned()];
    if full {
        lines.push(render_scored_node_block(graph, node, true, debug_score));
    } else {
        lines.extend(render_scored_node_candidate_lines(
            graph,
            node,
            depth,
            detail,
            edge_cap,
            debug_score,
        ));
    }
    push_limit_omission_line(&mut lines, 1, total_available);
    format!("{}\n", lines.join("\n"))
}

fn render_single_node_candidate_lines(
    graph: &GraphFile,
    node: &Node,
    depth: usize,
    detail: DetailLevel,
    edge_cap: usize,
) -> Vec<String> {
    let mut lines = render_node_lines_with_edges(graph, node, detail, edge_cap);
    if depth > 0 {
        lines.extend(render_neighbor_layers(graph, node, depth, detail));
    }
    lines
}

fn render_scored_node_candidate_lines(
    graph: &GraphFile,
    node: &ScoredNode<'_>,
    depth: usize,
    detail: DetailLevel,
    edge_cap: usize,
    debug_score: bool,
) -> Vec<String> {
    let mut lines = vec![format!("score: {}", node.score)];
    if debug_score {
        lines.push(render_score_debug_line(node));
    }
    lines.extend(render_single_node_candidate_lines(
        graph, node.node, depth, detail, edge_cap,
    ));
    lines
}

fn render_scored_node_block(
    graph: &GraphFile,
    node: &ScoredNode<'_>,
    full: bool,
    debug_score: bool,
) -> String {
    if debug_score {
        format!(
            "score: {}\n{}\n{}",
            node.score,
            render_score_debug_line(node),
            render_node_block(graph, node.node, full)
        )
    } else {
        format!(
            "score: {}\n{}",
            node.score,
            render_node_block(graph, node.node, full)
        )
    }
}

fn render_score_debug_line(node: &ScoredNode<'_>) -> String {
    format!(
        "score_debug: raw_relevance={:.3} normalized_relevance={} lexical_boost={} feedback_boost={} importance_boost={} authority_raw={} authority_applied={} authority_cap={}",
        node.breakdown.raw_relevance,
        node.breakdown.normalized_relevance,
        node.breakdown.lexical_boost,
        node.breakdown.feedback_boost,
        node.breakdown.importance_boost,
        node.breakdown.authority_raw,
        node.breakdown.authority_applied,
        node.breakdown.authority_cap,
    )
}

fn render_neighbor_layers(
    graph: &GraphFile,
    root: &Node,
    max_depth: usize,
    detail: DetailLevel,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::from([root.id.clone()]);
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(root.id.clone(), 0usize)]);
    let mut layers: Vec<Vec<&Node>> = vec![Vec::new(); max_depth + 1];

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for incident in incident_edges(graph, &node_id) {
            if seen.insert(incident.related.id.clone()) {
                let next_depth = depth + 1;
                if next_depth <= max_depth {
                    layers[next_depth].push(incident.related);
                    queue.push_back((incident.related.id.clone(), next_depth));
                }
            }
        }
    }

    for depth in 1..=max_depth {
        if layers[depth].is_empty() {
            continue;
        }
        let cap = match detail {
            DetailLevel::Rich => 6,
            DetailLevel::Compact => 4,
            DetailLevel::Minimal => 3,
        };
        let shown = layers[depth].len().min(cap);
        out.push(format!(
            "depth {depth}: {shown}/{} neighbors",
            layers[depth].len()
        ));
        for node in layers[depth].iter().take(shown) {
            out.extend(render_node_identity_lines(node, detail));
        }
        if layers[depth].len() > shown {
            out.push(format!(
                "... +{} more neighbors omitted",
                layers[depth].len() - shown
            ));
        }
    }

    out
}

fn render_node_lines_with_edges(
    graph: &GraphFile,
    node: &Node,
    detail: DetailLevel,
    edge_cap: usize,
) -> Vec<String> {
    let mut lines = render_node_identity_lines(node, detail);
    lines.extend(render_node_link_lines(graph, node, edge_cap));
    lines
}

fn render_node_identity_lines(node: &Node, detail: DetailLevel) -> Vec<String> {
    let mut lines = Vec::new();
    match detail {
        DetailLevel::Rich => {
            lines.push(format!(
                "# {} | {} [{}]",
                node.id,
                escape_cli_text(&node.name),
                node.r#type
            ));
            if !node.properties.alias.is_empty() {
                lines.push(format!(
                    "aka: {}",
                    node.properties
                        .alias
                        .iter()
                        .map(|alias| escape_cli_text(alias))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            push_description_line(&mut lines, &node.properties.description, None);
            let shown_facts = node.properties.key_facts.len().min(3);
            for fact in node.properties.key_facts.iter().take(shown_facts) {
                lines.push(format!("- {}", escape_cli_text(fact)));
            }
            let omitted = node.properties.key_facts.len().saturating_sub(shown_facts);
            if omitted > 0 {
                lines.push(format!("... {omitted} more facts omitted"));
            }
        }
        DetailLevel::Compact => {
            lines.push(format!(
                "# {} | {} [{}]",
                node.id,
                escape_cli_text(&node.name),
                node.r#type
            ));
            push_description_line(&mut lines, &node.properties.description, Some(140));
            if let Some(fact) = node.properties.key_facts.first() {
                lines.push(format!("- {}", escape_cli_text(fact)));
            }
        }
        DetailLevel::Minimal => {
            lines.push(format!(
                "# {} | {} [{}]",
                node.id,
                escape_cli_text(&node.name),
                node.r#type
            ));
        }
    }
    lines
}

fn render_node_link_lines(graph: &GraphFile, node: &Node, edge_cap: usize) -> Vec<String> {
    let incident = incident_edges(graph, &node.id);
    if incident.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    if incident.len() > 12 {
        lines.push(format!("links: {} total", incident.len()));
        let (out_summary, in_summary) = summarize_relations(&incident);
        if !out_summary.is_empty() {
            lines.push(format!("out: {out_summary}"));
        }
        if !in_summary.is_empty() {
            lines.push(format!("in: {in_summary}"));
        }
    }

    let shown = incident.len().min(edge_cap);
    for edge in incident.into_iter().take(shown) {
        let prefix = if edge.incoming { "<-" } else { "->" };
        lines.extend(render_edge_lines(prefix, edge.edge, edge.related, false));
    }
    if edge_cap > 0 && incident_count(graph, &node.id) > shown {
        lines.push(format!(
            "... {} more links omitted",
            incident_count(graph, &node.id) - shown
        ));
    }
    lines
}

fn incident_count(graph: &GraphFile, node_id: &str) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| edge.source_id == node_id || edge.target_id == node_id)
        .count()
}

struct IncidentEdge<'a> {
    edge: &'a Edge,
    related: &'a Node,
    incoming: bool,
}

fn incident_edges<'a>(graph: &'a GraphFile, node_id: &str) -> Vec<IncidentEdge<'a>> {
    let mut edges = Vec::new();
    for edge in &graph.edges {
        if edge.source_id == node_id {
            if let Some(related) = graph.node_by_id(&edge.target_id) {
                edges.push(IncidentEdge {
                    edge,
                    related,
                    incoming: false,
                });
            }
        } else if edge.target_id == node_id {
            if let Some(related) = graph.node_by_id(&edge.source_id) {
                edges.push(IncidentEdge {
                    edge,
                    related,
                    incoming: true,
                });
            }
        }
    }
    edges.sort_by(|left, right| {
        right
            .related
            .properties
            .importance
            .cmp(&left.related.properties.importance)
            .then_with(|| left.edge.relation.cmp(&right.edge.relation))
            .then_with(|| left.related.id.cmp(&right.related.id))
    });
    edges
}

fn summarize_relations(edges: &[IncidentEdge<'_>]) -> (String, String) {
    let mut out: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut incoming: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for edge in edges {
        let bucket = if edge.incoming {
            &mut incoming
        } else {
            &mut out
        };
        *bucket.entry(edge.edge.relation.clone()).or_insert(0) += 1;
    }

    (join_relation_counts(&out), join_relation_counts(&incoming))
}

fn join_relation_counts(counts: &std::collections::BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .take(3)
        .map(|(relation, count)| format!("{relation} x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_node_block(graph: &GraphFile, node: &Node, full: bool) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "# {} | {} [{}]",
        node.id,
        escape_cli_text(&node.name),
        node.r#type
    ));

    if !node.properties.alias.is_empty() {
        lines.push(format!(
            "aka: {}",
            node.properties
                .alias
                .iter()
                .map(|alias| escape_cli_text(alias))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    push_description_line(
        &mut lines,
        &node.properties.description,
        if full { None } else { Some(200) },
    );
    if full {
        if !node.properties.domain_area.is_empty() {
            lines.push(format!(
                "domain_area: {}",
                escape_cli_text(&node.properties.domain_area)
            ));
        }
        if !node.properties.provenance.is_empty() {
            lines.push(format!(
                "provenance: {}",
                escape_cli_text(&node.properties.provenance)
            ));
        }
        if let Some(confidence) = node.properties.confidence {
            lines.push(format!("confidence: {confidence}"));
        }
        lines.push(format!("importance: {}", node.properties.importance));
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
        lines.push(format!("- {}", escape_cli_text(fact)));
    }
    let omitted = node
        .properties
        .key_facts
        .len()
        .saturating_sub(facts_to_show);
    if omitted > 0 {
        lines.push(format!("... {omitted} more facts omitted"));
    }

    if full {
        if !node.source_files.is_empty() {
            lines.push(format!(
                "sources: {}",
                node.source_files
                    .iter()
                    .map(|source| escape_cli_text(source))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        push_feedback_lines(
            &mut lines,
            node.properties.feedback_score,
            node.properties.feedback_count,
            node.properties.feedback_last_ts_ms,
            None,
        );
    }

    let attached_notes: Vec<_> = graph
        .notes
        .iter()
        .filter(|note| note.node_id == node.id)
        .collect();
    if full && !attached_notes.is_empty() {
        lines.push(format!("notes: {}", attached_notes.len()));
        for note in attached_notes {
            lines.extend(render_attached_note_lines(note));
        }
    }

    for edge in outgoing_edges(graph, &node.id, full) {
        if let Some(target) = graph.node_by_id(&edge.target_id) {
            lines.extend(render_edge_lines("->", edge, target, full));
        }
    }
    for edge in incoming_edges(graph, &node.id, full) {
        if let Some(source) = graph.node_by_id(&edge.source_id) {
            lines.extend(render_edge_lines("<-", edge, source, full));
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

fn render_edge_lines(prefix: &str, edge: &Edge, related: &Node, full: bool) -> Vec<String> {
    let (arrow, relation) = if edge.relation.starts_with("NOT_") {
        (
            format!("{prefix}!"),
            edge.relation.trim_start_matches("NOT_"),
        )
    } else {
        (prefix.to_owned(), edge.relation.as_str())
    };

    let mut line = format!(
        "{arrow} {relation} | {} | {}",
        related.id,
        escape_cli_text(&related.name)
    );
    if !edge.properties.detail.is_empty() {
        line.push_str(" | ");
        let detail = escape_cli_text(&edge.properties.detail);
        if full {
            line.push_str(&detail);
        } else {
            line.push_str(&truncate(&detail, 80));
        }
    }
    let mut lines = vec![line];
    if full {
        push_feedback_lines(
            &mut lines,
            edge.properties.feedback_score,
            edge.properties.feedback_count,
            edge.properties.feedback_last_ts_ms,
            Some("edge_"),
        );
        if !edge.properties.valid_from.is_empty() {
            lines.push(format!("edge_valid_from: {}", edge.properties.valid_from));
        }
        if !edge.properties.valid_to.is_empty() {
            lines.push(format!("edge_valid_to: {}", edge.properties.valid_to));
        }
    }
    lines
}

fn truncate(value: &str, max_len: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_len {
        return value.to_owned();
    }
    let truncated: String = value.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}

fn escape_cli_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn push_description_line(lines: &mut Vec<String>, description: &str, max_len: Option<usize>) {
    if description.is_empty() {
        return;
    }
    let escaped = escape_cli_text(description);
    let rendered = match max_len {
        Some(limit) => truncate(&escaped, limit),
        None => escaped,
    };
    lines.push(format!("desc: {rendered}"));
}

fn push_feedback_lines(
    lines: &mut Vec<String>,
    score: f64,
    count: u64,
    last_ts_ms: Option<u64>,
    prefix: Option<&str>,
) {
    let prefix = prefix.unwrap_or("");
    if score != 0.0 {
        lines.push(format!("{prefix}feedback_score: {score}"));
    }
    if count != 0 {
        lines.push(format!("{prefix}feedback_count: {count}"));
    }
    if let Some(ts) = last_ts_ms {
        lines.push(format!("{prefix}feedback_last_ts_ms: {ts}"));
    }
}

fn render_attached_note_lines(note: &crate::graph::Note) -> Vec<String> {
    let mut lines = vec![format!("! {}", note.id)];
    if !note.body.is_empty() {
        lines.push(format!("note_body: {}", escape_cli_text(&note.body)));
    }
    if !note.tags.is_empty() {
        lines.push(format!(
            "note_tags: {}",
            note.tags
                .iter()
                .map(|tag| escape_cli_text(tag))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !note.author.is_empty() {
        lines.push(format!("note_author: {}", escape_cli_text(&note.author)));
    }
    if !note.created_at.is_empty() {
        lines.push(format!("note_created_at: {}", note.created_at));
    }
    if !note.provenance.is_empty() {
        lines.push(format!(
            "note_provenance: {}",
            escape_cli_text(&note.provenance)
        ));
    }
    if !note.source_files.is_empty() {
        lines.push(format!(
            "note_sources: {}",
            note.source_files
                .iter()
                .map(|source| escape_cli_text(source))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines
}

fn find_matches_with_index<'a>(
    graph: &'a GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> Vec<ScoredNode<'a>> {
    let mut matches = find_all_matches_with_index(graph, query, include_features, mode, index);
    matches.truncate(limit);
    matches
}

fn find_all_matches_with_index<'a>(
    graph: &'a GraphFile,
    query: &str,
    include_features: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> Vec<ScoredNode<'a>> {
    let context = FindQueryContext::build(graph);
    let mut scored: Vec<ScoredNode<'a>> = match mode {
        FindMode::Fuzzy => {
            let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let candidates = graph
                .nodes
                .iter()
                .filter(|node| include_features || node.r#type != "Feature")
                .filter_map(|node| {
                    score_node(&context, node, query, &pattern, &mut matcher).map(|score| {
                        RawCandidate {
                            node,
                            raw_relevance: score as f64,
                            lexical_boost: 0,
                        }
                    })
                })
                .collect();
            compose_scores(candidates)
        }
        FindMode::Bm25 => compose_scores(score_bm25_raw(
            graph,
            &context,
            query,
            include_features,
            index,
        )),
    };

    scored.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.node.id.cmp(&right.node.id))
    });
    scored
}

fn compose_scores<'a>(candidates: Vec<RawCandidate<'a>>) -> Vec<ScoredNode<'a>> {
    let max_raw = candidates
        .iter()
        .map(|candidate| candidate.raw_relevance)
        .fold(0.0f64, f64::max);
    let max_raw_log = max_raw.ln_1p();

    candidates
        .into_iter()
        .filter_map(|candidate| {
            if candidate.raw_relevance <= 0.0 {
                return None;
            }
            let normalized_relevance = if max_raw_log > 0.0 {
                ((candidate.raw_relevance.ln_1p() / max_raw_log) * 1000.0).round() as i64
            } else {
                0
            };
            let feedback = feedback_boost(candidate.node);
            let importance = importance_boost(candidate.node);
            let authority_raw = feedback + importance;
            let relative_cap =
                ((normalized_relevance as f64) * SCORE_META_MAX_RATIO).round() as i64;
            let authority_cap = relative_cap.max(SCORE_META_MIN_CAP).min(SCORE_META_MAX_CAP);
            let authority_applied = authority_raw.clamp(-authority_cap, authority_cap);
            let final_score = normalized_relevance + authority_applied;

            Some(ScoredNode {
                score: final_score,
                node: candidate.node,
                breakdown: ScoreBreakdown {
                    raw_relevance: candidate.raw_relevance,
                    normalized_relevance,
                    lexical_boost: candidate.lexical_boost,
                    feedback_boost: feedback,
                    importance_boost: importance,
                    authority_raw,
                    authority_applied,
                    authority_cap,
                },
            })
        })
        .collect()
}

fn feedback_boost(node: &Node) -> i64 {
    let count = node.properties.feedback_count as f64;
    if count <= 0.0 {
        return 0;
    }
    let avg = node.properties.feedback_score / count;
    let confidence = (count.ln_1p() / 3.0).min(1.0);
    let scaled = avg * 200.0 * confidence;
    scaled.clamp(-300.0, 300.0).round() as i64
}

fn importance_boost(node: &Node) -> i64 {
    (i64::from(node.properties.importance) - IMPORTANCE_NEUTRAL) * IMPORTANCE_STEP_BOOST
}

fn score_bm25_raw<'a>(
    graph: &'a GraphFile,
    context: &FindQueryContext<'a>,
    query: &str,
    include_features: bool,
    index: Option<&Bm25Index>,
) -> Vec<RawCandidate<'a>> {
    let terms = tokenize(query);
    if terms.is_empty() {
        return Vec::new();
    }

    if let Some(idx) = index {
        let results = idx.search(&terms, graph);
        return results
            .into_iter()
            .filter_map(|(node_id, score)| {
                let node = graph.node_by_id(&node_id)?;
                if !include_features && node.r#type == "Feature" {
                    return None;
                }
                let document_terms = node_document_terms(context, node);
                let lexical_boost = bm25_lexical_boost(&terms, &document_terms);
                Some(RawCandidate {
                    node,
                    raw_relevance: score as f64 * 100.0 + lexical_boost as f64,
                    lexical_boost,
                })
            })
            .collect();
    }

    let mut docs: Vec<(&'a Node, Vec<String>)> = graph
        .nodes
        .iter()
        .filter(|node| include_features || node.r#type != "Feature")
        .map(|node| (node, node_document_terms(context, node)))
        .collect();

    if docs.is_empty() {
        return Vec::new();
    }

    let mut df: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for term in &terms {
        let mut count = 0usize;
        for (_, tokens) in &docs {
            if tokens.iter().any(|t| t == term) {
                count += 1;
            }
        }
        df.insert(term.as_str(), count);
    }

    let total_docs = docs.len() as f64;
    let avgdl = docs
        .iter()
        .map(|(_, tokens)| tokens.len() as f64)
        .sum::<f64>()
        / total_docs;

    let mut scored = Vec::new();

    for (node, tokens) in docs.drain(..) {
        let dl = tokens.len() as f64;
        if dl == 0.0 {
            continue;
        }
        let mut score = 0.0f64;
        for term in &terms {
            let tf = tokens.iter().filter(|t| *t == term).count() as f64;
            if tf == 0.0 {
                continue;
            }
            let df_t = *df.get(term.as_str()).unwrap_or(&0) as f64;
            let idf = (1.0 + (total_docs - df_t + 0.5) / (df_t + 0.5)).ln();
            let denom = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avgdl));
            score += idf * (tf * (BM25_K1 + 1.0) / denom);
        }
        if score > 0.0 {
            let lexical_boost = bm25_lexical_boost(&terms, &tokens);
            scored.push(RawCandidate {
                node,
                raw_relevance: score * 100.0 + lexical_boost as f64,
                lexical_boost,
            });
        }
    }

    scored
}

fn node_document_terms(context: &FindQueryContext<'_>, node: &Node) -> Vec<String> {
    let mut tokens = Vec::new();
    push_terms(&mut tokens, &node.id, BM25_ID_WEIGHT);
    push_terms(&mut tokens, &node.name, BM25_NAME_WEIGHT);
    push_terms(
        &mut tokens,
        &node.properties.description,
        BM25_DESCRIPTION_WEIGHT,
    );
    for alias in &node.properties.alias {
        push_terms(&mut tokens, alias, BM25_ALIAS_WEIGHT);
    }
    for fact in &node.properties.key_facts {
        push_terms(&mut tokens, fact, BM25_FACT_WEIGHT);
    }
    for note in context.notes_for(&node.id) {
        push_terms(&mut tokens, &note.body, BM25_NOTE_BODY_WEIGHT);
        for tag in &note.tags {
            push_terms(&mut tokens, tag, BM25_NOTE_TAG_WEIGHT);
        }
    }
    for neighbor in context.neighbors_for(&node.id) {
        push_terms(&mut tokens, &neighbor.id, BM25_NEIGHBOR_WEIGHT);
        push_terms(&mut tokens, &neighbor.name, BM25_NEIGHBOR_WEIGHT);
        push_terms(
            &mut tokens,
            &neighbor.properties.description,
            BM25_NEIGHBOR_WEIGHT,
        );
        for alias in &neighbor.properties.alias {
            push_terms(&mut tokens, alias, BM25_NEIGHBOR_WEIGHT);
        }
    }
    tokens
}

fn push_terms(target: &mut Vec<String>, value: &str, weight: usize) {
    if value.is_empty() {
        return;
    }
    let terms = tokenize(value);
    for _ in 0..weight {
        target.extend(terms.iter().cloned());
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn bm25_lexical_boost(query_terms: &[String], document_terms: &[String]) -> i64 {
    if query_terms.is_empty() || document_terms.is_empty() {
        return 0;
    }
    if query_terms.len() > 1 && contains_token_phrase(document_terms, query_terms) {
        return BM25_PHRASE_MATCH_BOOST;
    }
    let document_vocab: HashSet<&str> = document_terms.iter().map(String::as_str).collect();
    let query_vocab: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
    let matched_tokens = query_vocab
        .iter()
        .filter(|token| document_vocab.contains(**token))
        .count() as i64;
    if matched_tokens == 0 {
        return 0;
    }
    let query_token_count = query_vocab.len() as i64;
    (matched_tokens * BM25_TOKEN_MATCH_BOOST + query_token_count - 1) / query_token_count
}

fn contains_token_phrase(document_terms: &[String], query_terms: &[String]) -> bool {
    if query_terms.is_empty() || query_terms.len() > document_terms.len() {
        return false;
    }
    document_terms
        .windows(query_terms.len())
        .any(|window| window == query_terms)
}

fn score_node(
    context: &FindQueryContext<'_>,
    node: &Node,
    query: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
) -> Option<u32> {
    let mut primary_score = 0;
    let mut primary_hits = 0;

    let id_score = score_primary_field(query, pattern, matcher, &node.id, 4);
    if id_score > 0 {
        primary_hits += 1;
    }
    primary_score += id_score;

    let name_score = score_primary_field(query, pattern, matcher, &node.name, 3);
    if name_score > 0 {
        primary_hits += 1;
    }
    primary_score += name_score;

    for alias in &node.properties.alias {
        let alias_score = score_primary_field(query, pattern, matcher, alias, 3);
        if alias_score > 0 {
            primary_hits += 1;
        }
        primary_score += alias_score;
    }

    let mut contextual_score = score_secondary_field(
        query,
        pattern,
        matcher,
        &node.properties.description,
        FUZZY_DESCRIPTION_WEIGHT,
    );
    for fact in &node.properties.key_facts {
        contextual_score += score_secondary_field(query, pattern, matcher, fact, FUZZY_FACT_WEIGHT);
    }
    contextual_score += score_notes_context(context, node, query, pattern, matcher);

    let neighbor_context = score_neighbor_context(context, node, query, pattern, matcher)
        .min(FUZZY_NEIGHBOR_CONTEXT_CAP);
    contextual_score += if primary_hits > 0 {
        neighbor_context / 2
    } else {
        neighbor_context
    };

    if primary_hits == 0 {
        contextual_score /= FUZZY_NO_PRIMARY_CONTEXT_DIVISOR;
    }

    let total = primary_score + contextual_score;
    (total > 0).then_some(total)
}

fn score_notes_context(
    context: &FindQueryContext<'_>,
    node: &Node,
    query: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
) -> u32 {
    let mut total = 0;
    for note in context.notes_for(&node.id) {
        total += score_secondary_field(query, pattern, matcher, &note.body, FUZZY_NOTE_BODY_WEIGHT);
        for tag in &note.tags {
            total += score_secondary_field(query, pattern, matcher, tag, FUZZY_NOTE_TAG_WEIGHT);
        }
    }
    total
}

fn score_neighbor_context(
    context: &FindQueryContext<'_>,
    node: &Node,
    query: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
) -> u32 {
    let mut best = 0;

    for neighbor in context.neighbors_for(&node.id) {
        let mut score = score_secondary_field(query, pattern, matcher, &neighbor.id, 1)
            + score_secondary_field(query, pattern, matcher, &neighbor.name, 1)
            + score_secondary_field(query, pattern, matcher, &neighbor.properties.description, 1);

        for alias in &neighbor.properties.alias {
            score += score_secondary_field(query, pattern, matcher, alias, 1);
        }

        best = best.max(score);
    }

    best
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
    let fuzzy = score_field(pattern, matcher, value).unwrap_or(0);
    if bonus == 0 && fuzzy == 0 {
        return 0;
    }
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
    let fuzzy = score_field(pattern, matcher, value).unwrap_or(0);
    if bonus == 0 && fuzzy == 0 {
        return 0;
    }
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

    query
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
        .sum()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(
        id: &str,
        name: &str,
        description: &str,
        key_facts: &[&str],
        alias: &[&str],
        importance: u8,
        feedback_score: f64,
        feedback_count: u64,
    ) -> Node {
        let mut properties = crate::graph::NodeProperties::default();
        properties.description = description.to_owned();
        properties.key_facts = key_facts.iter().map(|v| (*v).to_owned()).collect();
        properties.alias = alias.iter().map(|v| (*v).to_owned()).collect();
        properties.importance = importance;
        properties.feedback_score = feedback_score;
        properties.feedback_count = feedback_count;
        Node {
            id: id.to_owned(),
            r#type: "Concept".to_owned(),
            name: name.to_owned(),
            properties,
            source_files: Vec::new(),
        }
    }

    fn score_for(results: &[ScoredNode<'_>], id: &str) -> i64 {
        results
            .iter()
            .find(|item| item.node.id == id)
            .map(|item| item.score)
            .expect("score for node")
    }

    #[test]
    fn textual_bonus_tiers_are_stable() {
        assert_eq!(textual_bonus("abc", "abc"), 400);
        assert_eq!(textual_bonus("abc", "xxabcxx"), 200);
        assert_eq!(textual_bonus("abc def", "aa abc and def zz"), 160);
        assert_eq!(textual_bonus("abc", "aXbYc"), 40);
        assert_eq!(textual_bonus("abc", "zzz"), 0);
    }

    #[test]
    fn tokenize_handles_unicode_casefolding() {
        let tokens = tokenize("ŁÓDŹ smart-home");
        assert_eq!(tokens, vec!["łódź", "smart", "home"]);
    }

    #[test]
    fn bm25_lexical_boost_prefers_phrase_then_tokens() {
        let query_terms = tokenize("smart home api");
        assert_eq!(
            bm25_lexical_boost(&query_terms, &tokenize("x smart home api y")),
            120
        );
        assert_eq!(
            bm25_lexical_boost(&query_terms, &tokenize("smart x api y home")),
            45
        );
        assert_eq!(
            bm25_lexical_boost(&query_terms, &tokenize("nothing here")),
            0
        );
    }

    #[test]
    fn score_node_uses_key_facts_and_notes_without_primary_match() {
        let node = make_node(
            "concept:gateway",
            "Gateway",
            "",
            &["Autentykacja OAuth2 przez konto producenta"],
            &[],
            4,
            0.0,
            0,
        );
        let mut graph = GraphFile::new("test");
        graph.nodes.push(node.clone());
        graph.notes.push(crate::graph::Note {
            id: "note:oauth".to_owned(),
            node_id: node.id.clone(),
            body: "Token refresh przez OAuth2".to_owned(),
            tags: vec!["oauth2".to_owned()],
            ..Default::default()
        });

        let pattern = Pattern::parse(
            "oauth2 producenta",
            CaseMatching::Ignore,
            Normalization::Smart,
        );
        let context = FindQueryContext::build(&graph);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let score = score_node(&context, &node, "oauth2 producenta", &pattern, &mut matcher);
        assert!(score.is_some_and(|value| value > 0));

        let empty_graph = GraphFile::new("empty");
        let empty_node = make_node("concept:gateway", "Gateway", "", &[], &[], 4, 0.0, 0);
        let empty_context = FindQueryContext::build(&empty_graph);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let empty_score = score_node(
            &empty_context,
            &empty_node,
            "oauth2 producenta",
            &pattern,
            &mut matcher,
        );
        assert!(empty_score.is_none());
    }

    #[test]
    fn score_bm25_respects_importance_boost_for_equal_documents() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "concept:high",
            "High",
            "smart home api",
            &[],
            &[],
            6,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:low",
            "Low",
            "smart home api",
            &[],
            &[],
            1,
            0.0,
            0,
        ));

        let results =
            find_all_matches_with_index(&graph, "smart home api", true, FindMode::Bm25, None);
        let high_score = score_for(&results, "concept:high");
        let low_score = score_for(&results, "concept:low");
        assert!(high_score > low_score);
    }

    #[test]
    fn final_score_caps_authority_boost_for_weak_relevance() {
        let weak = make_node(
            "concept:weak",
            "Weak",
            "smart home api",
            &[],
            &[],
            6,
            300.0,
            1,
        );
        let strong = make_node(
            "concept:strong",
            "Strong",
            "smart home api smart home api smart home api smart home api",
            &[],
            &[],
            4,
            0.0,
            0,
        );
        let candidates = vec![
            RawCandidate {
                node: &weak,
                raw_relevance: 12.0,
                lexical_boost: 0,
            },
            RawCandidate {
                node: &strong,
                raw_relevance: 100.0,
                lexical_boost: 0,
            },
        ];
        let scored = compose_scores(candidates);
        let weak_scored = scored
            .iter()
            .find(|item| item.node.id == "concept:weak")
            .expect("weak node");
        assert_eq!(
            weak_scored.breakdown.authority_applied,
            weak_scored.breakdown.authority_cap
        );
        assert!(weak_scored.breakdown.authority_raw > weak_scored.breakdown.authority_cap);
    }

    #[test]
    fn importance_and_feedback_boost_have_expected_ranges() {
        let high_importance = make_node("concept:high", "High", "", &[], &[], 6, 0.0, 0);
        let low_importance = make_node("concept:low", "Low", "", &[], &[], 1, 0.0, 0);
        assert_eq!(importance_boost(&high_importance), 44);
        assert_eq!(importance_boost(&low_importance), -66);

        let positive = make_node("concept:pos", "Pos", "", &[], &[], 4, 1.0, 1);
        let negative = make_node("concept:neg", "Neg", "", &[], &[], 4, -2.0, 1);
        let saturated = make_node("concept:sat", "Sat", "", &[], &[], 4, 300.0, 1);
        assert_eq!(feedback_boost(&positive), 46);
        assert_eq!(feedback_boost(&negative), -92);
        assert_eq!(feedback_boost(&saturated), 300);
    }
}
