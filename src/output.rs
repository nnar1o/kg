use std::collections::{HashMap, HashSet, VecDeque};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::graph::{Edge, GraphFile, Node, Note};
use crate::index::Bm25Index;
use crate::text_norm;

const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;
const DEFAULT_TARGET_CHARS: usize = 4200;
const MIN_TARGET_CHARS: usize = 300;
const MAX_TARGET_CHARS: usize = 12_000;
const FUZZY_NEIGHBOR_CONTEXT_CAP: u32 = 220;
const FUZZY_NO_PRIMARY_CONTEXT_DIVISOR: u32 = 3;
const FUZZY_NEIGHBOR_CONTEXT_DIVISOR: u32 = 3;
const FUZZY_DESCRIPTION_WEIGHT: u32 = 2;
const FUZZY_FACT_WEIGHT: u32 = 2;
const FUZZY_NOTE_BODY_WEIGHT: u32 = 1;
const FUZZY_NOTE_TAG_WEIGHT: u32 = 2;
const BM25_PHRASE_MATCH_BOOST: i64 = 120;
const BM25_PROXIMITY_MATCH_BOOST: i64 = 80;
const BM25_TOKEN_MATCH_BOOST: i64 = 45;
const BM25_ID_WEIGHT: usize = 5;
const BM25_NAME_WEIGHT: usize = 4;
const BM25_ALIAS_WEIGHT: usize = 4;
const BM25_DESCRIPTION_WEIGHT: usize = 2;
const BM25_FACT_WEIGHT: usize = 2;
const BM25_NOTE_BODY_WEIGHT: usize = 1;
const BM25_NOTE_TAG_WEIGHT: usize = 1;
const BM25_NEIGHBOR_WEIGHT: usize = 1;
const BM25_SELF_CONTEXT_WEIGHT: f64 = 3.0;
const BM25_NEIGHBOR_CONTEXT_WEIGHT: f64 = 1.0;
const BM25_PROXIMITY_WINDOW_TOKENS: usize = 6;
const FACT_VOLUME_BASE_CHARS: f64 = 500.0;
const FACT_VOLUME_MIN_FACTOR: f64 = 0.35;
const IMPORTANCE_NEUTRAL: f64 = 0.5;
const IMPORTANCE_MAX_ABS_BOOST: f64 = 66.0;
const SCORE_META_MAX_RATIO: f64 = 0.35;
const SCORE_META_MIN_CAP: i64 = 30;
const SCORE_META_MAX_CAP: i64 = 240;

#[derive(Debug, Clone, Copy)]
pub enum FindMode {
    Fuzzy,
    Bm25,
    Hybrid,
}

#[derive(Debug, Clone, Copy)]
pub struct FindTune {
    pub bm25: f64,
    pub fuzzy: f64,
    pub vector: f64,
}

impl FindTune {
    pub fn parse(raw: &str) -> Option<Self> {
        let mut tune = Self::default();
        for part in raw.split(',') {
            let (key, value) = part.split_once('=')?;
            let value = value.trim().parse::<f64>().ok()?;
            match key.trim() {
                "bm25" => tune.bm25 = value,
                "fuzzy" => tune.fuzzy = value,
                "vector" => tune.vector = value,
                _ => return None,
            }
        }
        Some(tune.clamped())
    }

    fn clamped(self) -> Self {
        Self {
            bm25: self.bm25.clamp(0.0, 1.0),
            fuzzy: self.fuzzy.clamp(0.0, 1.0),
            vector: self.vector.clamp(0.0, 1.0),
        }
    }
}

impl Default for FindTune {
    fn default() -> Self {
        Self {
            bm25: 0.55,
            fuzzy: 0.35,
            vector: 0.10,
        }
    }
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
    include_metadata: bool,
    mode: FindMode,
    full: bool,
) -> String {
    render_find_with_index(
        graph,
        queries,
        limit,
        include_features,
        include_metadata,
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
    include_metadata: bool,
    mode: FindMode,
    full: bool,
    debug_score: bool,
    index: Option<&Bm25Index>,
) -> String {
    render_find_with_index_tuned(
        graph,
        queries,
        limit,
        include_features,
        include_metadata,
        mode,
        full,
        debug_score,
        index,
        None,
    )
}

pub fn render_find_with_index_tuned(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    full: bool,
    debug_score: bool,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> String {
    let mut sections = Vec::new();
    for query in queries {
        let matches = find_all_matches_with_index(
            graph,
            query,
            include_features,
            include_metadata,
            mode,
            index,
            tune,
        );
        let total = matches.len();
        let visible: Vec<_> = matches.into_iter().take(limit).collect();
        let shown = visible.len();
        let mut lines = vec![render_result_header(query, shown, total)];
        for scored in visible {
            lines.push(render_scored_node_block(
                graph,
                &scored,
                full,
                debug_score,
                Some(query.as_str()),
            ));
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
    include_metadata: bool,
    mode: FindMode,
) -> Vec<Node> {
    find_matches_with_index(
        graph,
        query,
        limit,
        include_features,
        include_metadata,
        mode,
        None,
        None,
    )
    .into_iter()
    .map(|item| item.node.clone())
    .collect()
}

pub fn find_nodes_with_index(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> Vec<Node> {
    find_matches_with_index(
        graph,
        query,
        limit,
        include_features,
        include_metadata,
        mode,
        index,
        None,
    )
    .into_iter()
    .map(|item| item.node.clone())
    .collect()
}

pub fn find_nodes_with_index_tuned(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> Vec<Node> {
    find_matches_with_index(
        graph,
        query,
        limit,
        include_features,
        include_metadata,
        mode,
        index,
        tune,
    )
    .into_iter()
    .map(|item| item.node.clone())
    .collect()
}

pub fn find_nodes_and_total_with_index(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> (usize, Vec<Node>) {
    let matches = find_all_matches_with_index(
        graph,
        query,
        include_features,
        include_metadata,
        mode,
        index,
        None,
    );
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
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> (usize, Vec<ScoredNodeResult>) {
    find_scored_nodes_and_total_with_index_tuned(
        graph,
        query,
        limit,
        include_features,
        include_metadata,
        mode,
        index,
        None,
    )
}

pub fn find_scored_nodes_and_total_with_index_tuned(
    graph: &GraphFile,
    query: &str,
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> (usize, Vec<ScoredNodeResult>) {
    let matches = find_all_matches_with_index(
        graph,
        query,
        include_features,
        include_metadata,
        mode,
        index,
        tune,
    );
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
    include_metadata: bool,
    mode: FindMode,
) -> usize {
    count_find_results_with_index(
        graph,
        queries,
        limit,
        include_features,
        include_metadata,
        mode,
        None,
    )
}

pub fn count_find_results_with_index(
    graph: &GraphFile,
    queries: &[String],
    _limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
) -> usize {
    let mut total = 0;
    for query in queries {
        total += find_all_matches_with_index(
            graph,
            query,
            include_features,
            include_metadata,
            mode,
            index,
            None,
        )
        .len();
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
    include_metadata: bool,
    mode: FindMode,
    target_chars: Option<usize>,
    debug_score: bool,
    index: Option<&Bm25Index>,
) -> String {
    render_find_adaptive_with_index_tuned(
        graph,
        queries,
        limit,
        include_features,
        include_metadata,
        mode,
        target_chars,
        debug_score,
        index,
        None,
    )
}

pub fn render_find_adaptive_with_index_tuned(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    target_chars: Option<usize>,
    debug_score: bool,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> String {
    let target = clamp_target_chars(target_chars);
    let mut sections = Vec::new();
    for query in queries {
        let matches = find_all_matches_with_index(
            graph,
            query,
            include_features,
            include_metadata,
            mode,
            index,
            tune,
        );
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
        query,
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
                query,
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
                query,
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
        lines.push(render_scored_node_block(
            graph,
            node,
            true,
            debug_score,
            Some(query),
        ));
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
    let lines = render_single_node_candidate_lines(graph, node, depth, detail, edge_cap, None);
    format!("{}\n", lines.join("\n"))
}

fn render_single_result_candidate(
    graph: &GraphFile,
    query: &str,
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
        lines.push(render_scored_node_block(
            graph,
            node,
            true,
            debug_score,
            Some(query),
        ));
    } else {
        lines.extend(render_scored_node_candidate_lines(
            graph,
            query,
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
    query: Option<&str>,
) -> Vec<String> {
    let mut lines = render_node_lines_with_edges(graph, node, detail, edge_cap, query);
    if depth > 0 {
        lines.extend(render_neighbor_layers(graph, node, depth, detail));
    }
    lines
}

fn render_scored_node_candidate_lines(
    graph: &GraphFile,
    query: &str,
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
        graph,
        node.node,
        depth,
        detail,
        edge_cap,
        Some(query),
    ));
    lines
}

fn render_scored_node_block(
    graph: &GraphFile,
    node: &ScoredNode<'_>,
    full: bool,
    debug_score: bool,
    query: Option<&str>,
) -> String {
    if debug_score {
        format!(
            "score: {}\n{}\n{}",
            node.score,
            render_score_debug_line(node),
            render_node_block_with_query(graph, node.node, full, query)
        )
    } else {
        format!(
            "score: {}\n{}",
            node.score,
            render_node_block_with_query(graph, node.node, full, query)
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
    query: Option<&str>,
) -> Vec<String> {
    let mut lines = render_node_identity_lines(node, detail);
    lines.extend(render_node_link_lines(graph, node, edge_cap, query));
    lines
}

fn render_node_identity_lines(node: &Node, detail: DetailLevel) -> Vec<String> {
    let mut lines = Vec::new();
    let display_name = node_display_name(node);
    match detail {
        DetailLevel::Rich => {
            lines.push(format!(
                "# {} | {} [{}]",
                node.id,
                escape_cli_text(&display_name),
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
                escape_cli_text(&display_name),
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
                escape_cli_text(&display_name),
                node.r#type
            ));
        }
    }
    lines
}

fn node_display_name(node: &Node) -> String {
    if !node.name.trim().is_empty() {
        return node.name.clone();
    }
    let raw = node
        .id
        .split_once(':')
        .map(|(_, suffix)| {
            suffix
                .rsplit_once(':')
                .map(|(name, _)| name)
                .unwrap_or(suffix)
        })
        .unwrap_or(node.id.as_str())
        .to_owned();
    unescape_generated_name(&raw)
}

fn unescape_generated_name(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '~' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('~') => out.push('~'),
            Some('c') => out.push(':'),
            Some(other) => {
                out.push('~');
                out.push(other);
            }
            None => out.push('~'),
        }
    }
    out
}

fn render_node_link_lines(
    graph: &GraphFile,
    node: &Node,
    edge_cap: usize,
    query: Option<&str>,
) -> Vec<String> {
    let mut incident = incident_edges(graph, &node.id);
    if let Some(query) = query {
        let query_terms = text_norm::expand_query_terms(query);
        if !query_terms.is_empty() {
            incident.sort_by(|left, right| {
                let right_relevance = incident_edge_query_relevance(right, &query_terms);
                let left_relevance = incident_edge_query_relevance(left, &query_terms);
                right_relevance
                    .cmp(&left_relevance)
                    .then_with(|| incident_edge_default_cmp(left, right))
            });
        }
    }
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
    edges.sort_by(incident_edge_default_cmp);
    edges
}

fn incident_edge_default_cmp(
    left: &IncidentEdge<'_>,
    right: &IncidentEdge<'_>,
) -> std::cmp::Ordering {
    right
        .related
        .properties
        .importance
        .partial_cmp(&left.related.properties.importance)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.edge.relation.cmp(&right.edge.relation))
        .then_with(|| left.related.id.cmp(&right.related.id))
}

fn incident_edge_query_relevance(edge: &IncidentEdge<'_>, query_terms: &[String]) -> i64 {
    if query_terms.is_empty() {
        return 0;
    }
    let related = edge.related;
    let mut score = 0;
    score += query_overlap_score(&related.id, query_terms, 6);
    score += query_overlap_score(&related.name, query_terms, 5);
    score += query_overlap_score(&related.properties.description, query_terms, 2);
    score += query_overlap_score(&edge.edge.relation, query_terms, 2);
    score += query_overlap_score(&edge.edge.properties.detail, query_terms, 2);
    for alias in &related.properties.alias {
        score += query_overlap_score(alias, query_terms, 4);
    }
    score
}

fn query_overlap_score(value: &str, query_terms: &[String], weight: i64) -> i64 {
    if value.is_empty() || query_terms.is_empty() {
        return 0;
    }
    let value_terms: HashSet<String> = tokenize(value).into_iter().collect();
    if value_terms.is_empty() {
        return 0;
    }
    let matches = query_terms
        .iter()
        .filter(|term| value_terms.contains(term.as_str()))
        .count() as i64;
    matches * weight
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
    render_node_block_with_query(graph, node, full, None)
}

fn render_node_block_with_query(
    graph: &GraphFile,
    node: &Node,
    full: bool,
    query: Option<&str>,
) -> String {
    let mut lines = Vec::new();
    let display_name = node_display_name(node);
    let generated = crate::validate::is_generated_node_type(&node.r#type);
    lines.push(format!(
        "# {} | {} [{}]",
        node.id,
        escape_cli_text(&display_name),
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
    if full && !generated {
        if !node.properties.domain_area.is_empty() {
            lines.push(format!(
                "domain_area: {}",
                escape_cli_text(&node.properties.domain_area)
            ));
        }
        if let Some(scan) = node.properties.scan {
            lines.push(format!("scan: {scan}"));
        }
        if let Some(scan_ignore_unknown) = node.properties.scan_ignore_unknown {
            lines.push(format!("scan_ignore_unknown: {scan_ignore_unknown}"));
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

    if full && !generated {
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

    for edge in outgoing_edges(graph, &node.id, full, query) {
        if let Some(target) = graph.node_by_id(&edge.target_id) {
            lines.extend(render_edge_lines("->", edge, target, full));
        }
    }
    for edge in incoming_edges(graph, &node.id, full, query) {
        if let Some(source) = graph.node_by_id(&edge.source_id) {
            lines.extend(render_edge_lines("<-", edge, source, full));
        }
    }

    lines.join("\n")
}

fn outgoing_edges<'a>(
    graph: &'a GraphFile,
    node_id: &str,
    full: bool,
    query: Option<&str>,
) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|edge| edge.source_id == node_id)
        .collect();
    if let Some(query) = query {
        let query_terms = text_norm::expand_query_terms(query);
        if !query_terms.is_empty() {
            edges.sort_by(|left, right| {
                let right_score = directed_edge_query_relevance(graph, right, false, &query_terms);
                let left_score = directed_edge_query_relevance(graph, left, false, &query_terms);
                right_score
                    .cmp(&left_score)
                    .then_with(|| left.relation.cmp(&right.relation))
                    .then_with(|| left.target_id.cmp(&right.target_id))
            });
        } else {
            edges.sort_by_key(|edge| (&edge.relation, &edge.target_id));
        }
    } else {
        edges.sort_by_key(|edge| (&edge.relation, &edge.target_id));
    }
    if !full {
        edges.truncate(3);
    }
    edges
}

fn incoming_edges<'a>(
    graph: &'a GraphFile,
    node_id: &str,
    full: bool,
    query: Option<&str>,
) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|edge| edge.target_id == node_id)
        .collect();
    if let Some(query) = query {
        let query_terms = text_norm::expand_query_terms(query);
        if !query_terms.is_empty() {
            edges.sort_by(|left, right| {
                let right_score = directed_edge_query_relevance(graph, right, true, &query_terms);
                let left_score = directed_edge_query_relevance(graph, left, true, &query_terms);
                right_score
                    .cmp(&left_score)
                    .then_with(|| left.relation.cmp(&right.relation))
                    .then_with(|| left.source_id.cmp(&right.source_id))
            });
        } else {
            edges.sort_by_key(|edge| (&edge.relation, &edge.source_id));
        }
    } else {
        edges.sort_by_key(|edge| (&edge.relation, &edge.source_id));
    }
    if !full {
        edges.truncate(3);
    }
    edges
}

fn directed_edge_query_relevance(
    graph: &GraphFile,
    edge: &Edge,
    incoming: bool,
    query_terms: &[String],
) -> i64 {
    let related = if incoming {
        graph.node_by_id(&edge.source_id)
    } else {
        graph.node_by_id(&edge.target_id)
    };
    let mut score = query_overlap_score(&edge.relation, query_terms, 2)
        + query_overlap_score(&edge.properties.detail, query_terms, 2);
    if let Some(node) = related {
        score += query_overlap_score(&node.id, query_terms, 6);
        score += query_overlap_score(&node.name, query_terms, 5);
        score += query_overlap_score(&node.properties.description, query_terms, 2);
        for alias in &node.properties.alias {
            score += query_overlap_score(alias, query_terms, 4);
        }
    }
    score
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
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> Vec<ScoredNode<'a>> {
    let mut matches = find_all_matches_with_index(
        graph,
        query,
        include_features,
        include_metadata,
        mode,
        index,
        tune,
    );
    matches.truncate(limit);
    matches
}

fn find_all_matches_with_index<'a>(
    graph: &'a GraphFile,
    query: &str,
    include_features: bool,
    include_metadata: bool,
    mode: FindMode,
    index: Option<&Bm25Index>,
    tune: Option<&FindTune>,
) -> Vec<ScoredNode<'a>> {
    let context = FindQueryContext::build(graph);
    let rewritten_query = rewrite_query(query);
    let fuzzy_query = if rewritten_query.is_empty() {
        query.to_owned()
    } else {
        rewritten_query
    };
    let mut scored: Vec<ScoredNode<'a>> = match mode {
        FindMode::Fuzzy => {
            let pattern = Pattern::parse(&fuzzy_query, CaseMatching::Ignore, Normalization::Smart);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let candidates = graph
                .nodes
                .iter()
                .filter(|node| node_is_searchable(node, include_features, include_metadata))
                .filter_map(|node| {
                    score_node(&context, node, &fuzzy_query, &pattern, &mut matcher).map(|score| {
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
            &fuzzy_query,
            include_features,
            include_metadata,
            index,
        )),
        FindMode::Hybrid => compose_scores(score_hybrid_raw(
            graph,
            &context,
            &fuzzy_query,
            include_features,
            include_metadata,
            index,
            tune.copied().unwrap_or_default(),
        )),
    };

    scored.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.node.id.cmp(&right.node.id))
    });
    let mut seen_ids = HashSet::new();
    scored.retain(|item| {
        let key = crate::validate::normalize_node_id(&item.node.id).to_ascii_lowercase();
        seen_ids.insert(key)
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
    let normalized_importance = if (0.0..=1.0).contains(&node.properties.importance) {
        node.properties.importance
    } else if (1.0..=6.0).contains(&node.properties.importance) {
        (node.properties.importance - 1.0) / 5.0
    } else {
        node.properties.importance.clamp(0.0, 1.0)
    };
    let normalized = (normalized_importance - IMPORTANCE_NEUTRAL) * 2.0;
    (normalized * IMPORTANCE_MAX_ABS_BOOST).round() as i64
}

fn score_bm25_raw<'a>(
    graph: &'a GraphFile,
    context: &FindQueryContext<'a>,
    query: &str,
    include_features: bool,
    include_metadata: bool,
    index: Option<&Bm25Index>,
) -> Vec<RawCandidate<'a>> {
    let terms = text_norm::expand_query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    if let Some(idx) = index {
        let results = idx.search(&terms, graph);
        return results
            .into_iter()
            .filter_map(|(node_id, score)| {
                let node = graph.node_by_id(&node_id)?;
                if !node_is_searchable(node, include_features, include_metadata) {
                    return None;
                }
                let self_terms = node_self_document_terms(context, node);
                let neighbor_score =
                    best_neighbor_bm25_score_with_index(context, node, &terms, idx);
                let base_score = combine_bm25_components(node, score as f64, neighbor_score);
                if base_score <= 0.0 {
                    return None;
                }
                let lexical_boost = bm25_lexical_boost_with_idf(&terms, &self_terms, |term| {
                    idx.idf.get(term).copied().unwrap_or(0.0) as f64
                });
                let proximity_boost = bm25_proximity_boost(context, node, &terms);
                Some(RawCandidate {
                    node,
                    raw_relevance: base_score * 100.0
                        + lexical_boost as f64
                        + proximity_boost as f64,
                    lexical_boost: lexical_boost + proximity_boost,
                })
            })
            .collect();
    }

    let docs: Vec<(&'a Node, Vec<String>)> = graph
        .nodes
        .iter()
        .filter(|node| node_is_searchable(node, include_features, include_metadata))
        .map(|node| (node, node_self_document_terms(context, node)))
        .collect();

    if docs.is_empty() {
        return Vec::new();
    }

    let mut df: HashMap<String, usize> = HashMap::new();
    for term in &terms {
        let mut count = 0usize;
        for (_, tokens) in &docs {
            if tokens.iter().any(|t| t == term) {
                count += 1;
            }
        }
        df.insert(term.clone(), count);
    }

    let total_docs = docs.len() as f64;
    let avgdl = docs
        .iter()
        .map(|(_, tokens)| tokens.len() as f64)
        .sum::<f64>()
        / total_docs.max(1.0);

    let mut idf_by_term: HashMap<String, f64> = HashMap::new();
    for term in &terms {
        let df_t = *df.get(term).unwrap_or(&0) as f64;
        let idf = (1.0 + (total_docs - df_t + 0.5) / (df_t + 0.5)).ln();
        idf_by_term.insert(term.clone(), idf);
    }

    let mut scored = Vec::new();

    for (node, self_terms) in docs {
        let self_score = bm25_document_score(&terms, &self_terms, &idf_by_term, avgdl);
        let neighbor_score = best_neighbor_bm25_score(context, node, &terms, &idf_by_term, avgdl);
        let base_score = combine_bm25_components(node, self_score, neighbor_score);
        if base_score <= 0.0 {
            continue;
        }
        let lexical_boost = bm25_lexical_boost_with_idf(&terms, &self_terms, |term| {
            idf_by_term.get(term).copied().unwrap_or(0.0)
        });
        let proximity_boost = bm25_proximity_boost(context, node, &terms);
        scored.push(RawCandidate {
            node,
            raw_relevance: base_score * 100.0 + lexical_boost as f64 + proximity_boost as f64,
            lexical_boost: lexical_boost + proximity_boost,
        });
    }

    scored
}

fn score_hybrid_raw<'a>(
    graph: &'a GraphFile,
    context: &FindQueryContext<'a>,
    query: &str,
    include_features: bool,
    include_metadata: bool,
    index: Option<&Bm25Index>,
    tune: FindTune,
) -> Vec<RawCandidate<'a>> {
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);

    let mut fuzzy_raw = HashMap::new();
    for node in graph
        .nodes
        .iter()
        .filter(|node| node_is_searchable(node, include_features, include_metadata))
    {
        if let Some(score) = score_node(context, node, query, &pattern, &mut matcher) {
            fuzzy_raw.insert(node.id.as_str(), score as f64);
        }
    }

    let bm25_candidates = score_bm25_raw(
        graph,
        context,
        query,
        include_features,
        include_metadata,
        index,
    );
    let mut bm25_raw = HashMap::new();
    let mut lexical_boost = HashMap::new();
    for candidate in bm25_candidates {
        bm25_raw.insert(candidate.node.id.as_str(), candidate.raw_relevance);
        lexical_boost.insert(candidate.node.id.as_str(), candidate.lexical_boost);
    }

    let fuzzy_norm = normalize_raw_scores(&fuzzy_raw);
    let bm25_norm = normalize_raw_scores(&bm25_raw);
    let total_weight = (tune.bm25 + tune.fuzzy).max(0.0001);

    graph
        .nodes
        .iter()
        .filter(|node| node_is_searchable(node, include_features, include_metadata))
        .filter_map(|node| {
            let f = fuzzy_norm.get(node.id.as_str()).copied().unwrap_or(0.0);
            let b = bm25_norm.get(node.id.as_str()).copied().unwrap_or(0.0);
            let combined = ((tune.fuzzy * f) + (tune.bm25 * b)) / total_weight;
            if combined <= 0.0 {
                return None;
            }
            Some(RawCandidate {
                node,
                raw_relevance: combined * 1000.0,
                lexical_boost: lexical_boost.get(node.id.as_str()).copied().unwrap_or(0),
            })
        })
        .collect()
}

fn normalize_raw_scores<'a>(raw: &'a HashMap<&'a str, f64>) -> HashMap<&'a str, f64> {
    let max_raw = raw.values().copied().fold(0.0f64, f64::max);
    let max_log = max_raw.ln_1p();
    raw.iter()
        .map(|(id, value)| {
            let normalized = if max_log > 0.0 {
                value.ln_1p() / max_log
            } else {
                0.0
            };
            (*id, normalized.clamp(0.0, 1.0))
        })
        .collect()
}

fn node_is_searchable(node: &Node, include_features: bool, include_metadata: bool) -> bool {
    (include_features || node.r#type != "Feature") && (include_metadata || node.r#type != "^")
}

fn node_self_document_terms(context: &FindQueryContext<'_>, node: &Node) -> Vec<String> {
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
    tokens
}

fn neighbor_document_terms(neighbor: &Node) -> Vec<String> {
    let mut tokens = Vec::new();
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
    tokens
}

fn fact_volume_normalizer(node: &Node) -> f64 {
    let fact_chars = node
        .properties
        .key_facts
        .iter()
        .map(|fact| fact.chars().count())
        .sum::<usize>() as f64;
    if fact_chars <= 0.0 {
        return 1.0;
    }
    let scaled = FACT_VOLUME_BASE_CHARS.sqrt() / fact_chars.sqrt();
    scaled.clamp(FACT_VOLUME_MIN_FACTOR, 1.0)
}

fn bm25_document_score(
    query_terms: &[String],
    document_terms: &[String],
    idf_by_term: &HashMap<String, f64>,
    avgdl: f64,
) -> f64 {
    if query_terms.is_empty() || document_terms.is_empty() {
        return 0.0;
    }
    let dl = document_terms.len() as f64;
    if dl <= 0.0 {
        return 0.0;
    }
    let mut score = 0.0;
    for term in query_terms {
        let tf = document_terms.iter().filter(|token| *token == term).count() as f64;
        if tf <= 0.0 {
            continue;
        }
        let idf = idf_by_term.get(term).copied().unwrap_or(0.0);
        if idf <= 0.0 {
            continue;
        }
        let denom = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avgdl.max(1.0)));
        score += idf * (tf * (BM25_K1 + 1.0) / denom);
    }
    score
}

fn best_neighbor_bm25_score(
    context: &FindQueryContext<'_>,
    node: &Node,
    query_terms: &[String],
    idf_by_term: &HashMap<String, f64>,
    avgdl: f64,
) -> f64 {
    context
        .neighbors_for(&node.id)
        .iter()
        .map(|neighbor| {
            let neighbor_terms = neighbor_document_terms(neighbor);
            bm25_document_score(query_terms, &neighbor_terms, idf_by_term, avgdl)
        })
        .fold(0.0f64, f64::max)
}

fn best_neighbor_bm25_score_with_index(
    context: &FindQueryContext<'_>,
    node: &Node,
    query_terms: &[String],
    index: &Bm25Index,
) -> f64 {
    let avgdl = index.avg_doc_len as f64;
    context
        .neighbors_for(&node.id)
        .iter()
        .map(|neighbor| {
            let neighbor_terms = neighbor_document_terms(neighbor);
            let dl = neighbor_terms.len() as f64;
            if dl <= 0.0 {
                return 0.0;
            }
            let mut score = 0.0;
            for term in query_terms {
                let idf = index.idf.get(term).copied().unwrap_or(0.0) as f64;
                if idf <= 0.0 {
                    continue;
                }
                let tf = neighbor_terms.iter().filter(|token| *token == term).count() as f64;
                if tf <= 0.0 {
                    continue;
                }
                let denom = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avgdl.max(1.0)));
                score += idf * (tf * (BM25_K1 + 1.0) / denom);
            }
            score
        })
        .fold(0.0f64, f64::max)
}

fn combine_bm25_components(node: &Node, self_score: f64, neighbor_score: f64) -> f64 {
    let combined =
        BM25_SELF_CONTEXT_WEIGHT * self_score + BM25_NEIGHBOR_CONTEXT_WEIGHT * neighbor_score;
    combined * fact_volume_normalizer(node)
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
    text_norm::tokenize(text)
}

fn rewrite_query(query: &str) -> String {
    text_norm::expand_query_terms(query).join(" ")
}

fn bm25_lexical_boost_with_idf<F>(
    query_terms: &[String],
    document_terms: &[String],
    idf_for: F,
) -> i64
where
    F: Fn(&str) -> f64,
{
    if query_terms.is_empty() || document_terms.is_empty() {
        return 0;
    }
    if query_terms.len() > 1 && contains_token_phrase(document_terms, query_terms) {
        return BM25_PHRASE_MATCH_BOOST;
    }
    let document_vocab: HashSet<&str> = document_terms.iter().map(String::as_str).collect();
    let query_vocab: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
    let mut total_idf = 0.0;
    let mut matched_idf = 0.0;
    let mut matched_terms = 0i64;
    for term in query_vocab {
        let idf = idf_for(term).max(0.0);
        total_idf += if idf > 0.0 { idf } else { 1.0 };
        if document_vocab.contains(term) {
            matched_terms += 1;
            matched_idf += if idf > 0.0 { idf } else { 1.0 };
        }
    }
    if matched_terms == 0 {
        return 0;
    }
    ((matched_idf / total_idf.max(1.0)) * BM25_TOKEN_MATCH_BOOST as f64).round() as i64
}

fn bm25_proximity_boost(
    context: &FindQueryContext<'_>,
    node: &Node,
    query_terms: &[String],
) -> i64 {
    if query_terms.len() < 2 {
        return 0;
    }
    let mut best_span_hits = proximity_hits_in_text(&node.id, query_terms)
        .max(proximity_hits_in_text(&node.name, query_terms))
        .max(proximity_hits_in_text(
            &node.properties.description,
            query_terms,
        ));
    for alias in &node.properties.alias {
        best_span_hits = best_span_hits.max(proximity_hits_in_text(alias, query_terms));
    }
    for fact in &node.properties.key_facts {
        best_span_hits = best_span_hits.max(proximity_hits_in_text(fact, query_terms));
    }
    for note in context.notes_for(&node.id) {
        best_span_hits = best_span_hits.max(proximity_hits_in_text(&note.body, query_terms));
        for tag in &note.tags {
            best_span_hits = best_span_hits.max(proximity_hits_in_text(tag, query_terms));
        }
    }
    if best_span_hits < 2 {
        0
    } else {
        BM25_PROXIMITY_MATCH_BOOST + (best_span_hits as i64 - 2) * 20
    }
}

fn proximity_hits_in_text(value: &str, query_terms: &[String]) -> usize {
    if value.is_empty() || query_terms.len() < 2 {
        return 0;
    }
    let tokens = tokenize(value);
    if tokens.len() < 2 {
        return 0;
    }
    let query_vocab: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
    let mut best = 0usize;
    for start in 0..tokens.len() {
        let end = (start + BM25_PROXIMITY_WINDOW_TOKENS).min(tokens.len());
        let mut seen: HashSet<&str> = HashSet::new();
        for token in &tokens[start..end] {
            if query_vocab.contains(token.as_str()) {
                seen.insert(token.as_str());
            }
        }
        best = best.max(seen.len());
    }
    best
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

    let id_score = score_primary_field(query, pattern, matcher, &node.id, 5);
    if id_score > 0 {
        primary_hits += 1;
    }
    primary_score += id_score;

    let name_score = score_primary_field(query, pattern, matcher, &node.name, 4);
    if name_score > 0 {
        primary_hits += 1;
    }
    primary_score += name_score;

    for alias in &node.properties.alias {
        let alias_score = score_primary_field(query, pattern, matcher, alias, 4);
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
    let mut facts_score = 0;
    for fact in &node.properties.key_facts {
        facts_score += score_secondary_field(query, pattern, matcher, fact, FUZZY_FACT_WEIGHT);
    }
    let facts_factor = fact_volume_normalizer(node);
    contextual_score += ((facts_score as f64) * facts_factor).round() as u32;
    contextual_score += score_notes_context(context, node, query, pattern, matcher);

    let neighbor_context = score_neighbor_context(context, node, query, pattern, matcher)
        .min(FUZZY_NEIGHBOR_CONTEXT_CAP);
    contextual_score += neighbor_context / FUZZY_NEIGHBOR_CONTEXT_DIVISOR;

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
        importance: f64,
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

    fn make_edge(source_id: &str, relation: &str, target_id: &str) -> Edge {
        Edge {
            source_id: source_id.to_owned(),
            relation: relation.to_owned(),
            target_id: target_id.to_owned(),
            properties: crate::graph::EdgeProperties::default(),
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
            bm25_lexical_boost_with_idf(&query_terms, &tokenize("x smart home api y"), |_| 1.0),
            120
        );
        assert_eq!(
            bm25_lexical_boost_with_idf(&query_terms, &tokenize("smart x api y home"), |_| 1.0),
            45
        );
        assert_eq!(
            bm25_lexical_boost_with_idf(&query_terms, &tokenize("nothing here"), |_| 1.0),
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
            0.5,
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
        let empty_node = make_node("concept:gateway", "Gateway", "", &[], &[], 0.5, 0.0, 0);
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
            1.0,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:low",
            "Low",
            "smart home api",
            &[],
            &[],
            0.0,
            0.0,
            0,
        ));

        let results = find_all_matches_with_index(
            &graph,
            "smart home api",
            true,
            false,
            FindMode::Bm25,
            None,
            None,
        );
        let high_score = score_for(&results, "concept:high");
        let low_score = score_for(&results, "concept:low");
        assert!(high_score > low_score);
    }

    #[test]
    fn bm25_prefers_self_match_over_neighbor_only_match() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "concept:self_hit",
            "Batch plugin output directory",
            "",
            &["BatchPlugin OUTPUT_DIR rule in WebLogic path"],
            &[],
            0.5,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:hub",
            "Integration Hub",
            "gateway for many systems",
            &[],
            &[],
            0.5,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:neighbor_hit",
            "BatchPlugin OUTPUT_DIR in WebLogic",
            "",
            &[],
            &[],
            0.5,
            0.0,
            0,
        ));
        graph
            .edges
            .push(make_edge("concept:hub", "HAS", "concept:neighbor_hit"));

        let results = find_all_matches_with_index(
            &graph,
            "BatchPlugin OUTPUT_DIR WebLogic",
            true,
            false,
            FindMode::Bm25,
            None,
            None,
        );

        assert!(results.iter().any(|item| item.node.id == "concept:hub"));
        assert!(score_for(&results, "concept:self_hit") > score_for(&results, "concept:hub"));
    }

    #[test]
    fn link_rendering_sorts_incident_edges_by_query_relevance() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "concept:center",
            "Center",
            "",
            &[],
            &[],
            0.5,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:relevant",
            "Push notification template",
            "",
            &[],
            &[],
            0.2,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:irrelevant_a",
            "Billing ledger",
            "",
            &[],
            &[],
            0.9,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:irrelevant_b",
            "Audit trail",
            "",
            &[],
            &[],
            0.8,
            0.0,
            0,
        ));
        graph
            .edges
            .push(make_edge("concept:center", "HAS", "concept:irrelevant_a"));
        graph
            .edges
            .push(make_edge("concept:center", "HAS", "concept:irrelevant_b"));
        graph
            .edges
            .push(make_edge("concept:center", "HAS", "concept:relevant"));

        let center = graph.node_by_id("concept:center").expect("center node");
        let lines = render_node_link_lines(&graph, center, 2, Some("push notification template"));

        let first_edge = lines
            .iter()
            .find(|line| line.starts_with("-> "))
            .expect("first edge line");
        assert!(first_edge.contains("concept:relevant"));
    }

    #[test]
    fn final_score_caps_authority_boost_for_weak_relevance() {
        let weak = make_node(
            "concept:weak",
            "Weak",
            "smart home api",
            &[],
            &[],
            1.0,
            300.0,
            1,
        );
        let strong = make_node(
            "concept:strong",
            "Strong",
            "smart home api smart home api smart home api smart home api",
            &[],
            &[],
            0.5,
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
        let high_importance = make_node("concept:high", "High", "", &[], &[], 1.0, 0.0, 0);
        let low_importance = make_node("concept:low", "Low", "", &[], &[], 0.0, 0.0, 0);
        assert_eq!(importance_boost(&high_importance), 66);
        assert_eq!(importance_boost(&low_importance), -66);

        let positive = make_node("concept:pos", "Pos", "", &[], &[], 0.5, 1.0, 1);
        let negative = make_node("concept:neg", "Neg", "", &[], &[], 0.5, -2.0, 1);
        let saturated = make_node("concept:sat", "Sat", "", &[], &[], 0.5, 300.0, 1);
        assert_eq!(feedback_boost(&positive), 46);
        assert_eq!(feedback_boost(&negative), -92);
        assert_eq!(feedback_boost(&saturated), 300);
    }

    #[test]
    fn find_deduplicates_results_by_node_id_for_single_query() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "concept:rule",
            "Business Rule",
            "Rule for billing decisions",
            &["Business rule validation"],
            &["billing rule"],
            0.5,
            0.0,
            0,
        ));
        graph.nodes.push(make_node(
            "concept:rule",
            "Business Rule Duplicate",
            "Duplicate record with same id",
            &["Business rule duplicate"],
            &[],
            0.5,
            0.0,
            0,
        ));

        let results = find_all_matches_with_index(
            &graph,
            "business rule",
            true,
            false,
            FindMode::Hybrid,
            None,
            None,
        );
        let rule_hits = results
            .iter()
            .filter(|item| item.node.id == "concept:rule")
            .count();
        assert_eq!(rule_hits, 1);
    }

    #[test]
    fn hybrid_score_does_not_change_when_only_vector_weight_changes() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "concept:auth",
            "Authentication Rule",
            "Business rule for authentication",
            &["auth rule"],
            &["login policy"],
            0.5,
            0.0,
            0,
        ));

        let with_vector = find_all_matches_with_index(
            &graph,
            "authentication rule",
            true,
            false,
            FindMode::Hybrid,
            None,
            Some(&FindTune {
                bm25: 0.55,
                fuzzy: 0.35,
                vector: 1.0,
            }),
        );
        let no_vector = find_all_matches_with_index(
            &graph,
            "authentication rule",
            true,
            false,
            FindMode::Hybrid,
            None,
            Some(&FindTune {
                bm25: 0.55,
                fuzzy: 0.35,
                vector: 0.0,
            }),
        );

        assert_eq!(with_vector.len(), 1);
        assert_eq!(no_vector.len(), 1);
        assert_eq!(with_vector[0].score, no_vector[0].score);
    }

    #[test]
    fn find_hides_metadata_nodes_unless_enabled() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(make_node(
            "^:graph_info",
            "Graph Metadata",
            "Internal metadata",
            &["graph_uuid=abc123"],
            &[],
            0.5,
            0.0,
            0,
        ));
        if let Some(meta) = graph
            .nodes
            .iter_mut()
            .find(|node| node.id == "^:graph_info")
        {
            meta.r#type = "^".to_owned();
        }

        let hidden = find_all_matches_with_index(
            &graph,
            "graph uuid",
            true,
            false,
            FindMode::Hybrid,
            None,
            None,
        );
        assert!(hidden.is_empty());

        let shown = find_all_matches_with_index(
            &graph,
            "graph uuid",
            true,
            true,
            FindMode::Hybrid,
            None,
            None,
        );
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].node.id, "^:graph_info");
    }
}
