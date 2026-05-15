use std::collections::HashMap;

use crate::graph::{GraphFile, Node};
use crate::index::Bm25Index;
use crate::text_norm;

const DEFAULT_MAX_SPAN_TOKENS: usize = 4;
const DEFAULT_MIN_SCORE_ONE_TOKEN: i64 = 120;
const DEFAULT_MIN_SCORE_MULTI_TOKEN: i64 = 105;
const DEFAULT_BM25_WEIGHT: f64 = 8.0;
const DEFAULT_SHORTLIST_LIMIT: usize = 12;

const SCORE_NAME_EXACT: i64 = 150;
const SCORE_ALIAS_EXACT: i64 = 140;
const SCORE_FACT_EXACT: i64 = 130;
const SCORE_ID_EXACT: i64 = 125;
const SCORE_NAME_OVERLAP: i64 = 120;
const SCORE_ALIAS_OVERLAP: i64 = 116;
const SCORE_FACT_OVERLAP: i64 = 110;
const SCORE_ID_OVERLAP: i64 = 108;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnnotateConfig {
    pub(crate) max_span_tokens: usize,
    pub(crate) min_score_one_token: i64,
    pub(crate) min_score_multi_token: i64,
    pub(crate) bm25_weight: f64,
    pub(crate) shortlist_limit: usize,
}

impl Default for AnnotateConfig {
    fn default() -> Self {
        Self {
            max_span_tokens: DEFAULT_MAX_SPAN_TOKENS,
            min_score_one_token: DEFAULT_MIN_SCORE_ONE_TOKEN,
            min_score_multi_token: DEFAULT_MIN_SCORE_MULTI_TOKEN,
            bm25_weight: DEFAULT_BM25_WEIGHT,
            shortlist_limit: DEFAULT_SHORTLIST_LIMIT,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnnotationCandidate {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) score: i64,
    pub(crate) node_id: String,
}

#[derive(Debug, Clone, Copy)]
enum AnnotateField {
    Id,
    Name,
    Alias,
    Fact,
}

impl AnnotateField {
    fn exact_score(self) -> i64 {
        match self {
            AnnotateField::Id => SCORE_ID_EXACT,
            AnnotateField::Name => SCORE_NAME_EXACT,
            AnnotateField::Alias => SCORE_ALIAS_EXACT,
            AnnotateField::Fact => SCORE_FACT_EXACT,
        }
    }

    fn overlap_score(self) -> i64 {
        match self {
            AnnotateField::Id => SCORE_ID_OVERLAP,
            AnnotateField::Name => SCORE_NAME_OVERLAP,
            AnnotateField::Alias => SCORE_ALIAS_OVERLAP,
            AnnotateField::Fact => SCORE_FACT_OVERLAP,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FieldMatch {
    score: i64,
}

struct AnnotateContext<'a> {
    graph: &'a GraphFile,
    index: Bm25Index,
    config: AnnotateConfig,
    normalized_terms_cache: HashMap<String, String>,
}

impl<'a> AnnotateContext<'a> {
    fn new(graph: &'a GraphFile, index: Option<&Bm25Index>) -> Self {
        Self {
            graph,
            index: index.cloned().unwrap_or_else(|| Bm25Index::build(graph)),
            config: AnnotateConfig::default(),
            normalized_terms_cache: HashMap::new(),
        }
    }

    fn collect_annotations(&mut self, text: &str) -> Vec<AnnotationCandidate> {
        let spans = text_norm::tokenize_spans(text);
        if spans.is_empty() || self.graph.nodes.is_empty() {
            return Vec::new();
        }

        let mut candidates = Vec::new();

        for start in 0..spans.len() {
            let mut normalized_terms = Vec::new();
            let mut span_start = spans[start].start;

            for token in spans.iter().skip(start).take(self.config.max_span_tokens) {
                if text_norm::is_colloquial_filler(&token.normalized) {
                    break;
                }

                if normalized_terms.is_empty() {
                    span_start = token.start;
                }
                normalized_terms.push(token.normalized.clone());

                let span_end = token.end;
                let matched = &text[span_start..span_end];
                let mut query_terms = text_norm::expand_query_terms(matched);
                if query_terms.is_empty() {
                    continue;
                }
                query_terms.sort();
                query_terms.dedup();

                let mut best: Option<AnnotationCandidate> = None;
                for (rank, (node_id, bm25_score)) in self
                    .index
                    .search(&query_terms, self.graph)
                    .into_iter()
                    .take(self.config.shortlist_limit)
                    .enumerate()
                {
                    let Some(node) = self.graph.node_by_id(&node_id) else {
                        continue;
                    };
                    if let Some(score) =
                        self.score_annotation_candidate(node, &normalized_terms, bm25_score, rank)
                    {
                        let candidate = AnnotationCandidate {
                            start: span_start,
                            end: span_end,
                            score,
                            node_id: node_ref(node),
                        };
                        best = match best {
                            Some(current) if !is_better_annotation(&candidate, &current) => {
                                Some(current)
                            }
                            _ => Some(candidate),
                        };
                    }
                }

                if let Some(candidate) = best {
                    candidates.push(candidate);
                }
            }
        }

        select_non_overlapping(candidates)
    }

    fn score_annotation_candidate(
        &mut self,
        node: &Node,
        candidate_terms: &[String],
        bm25_score: f32,
        rank: usize,
    ) -> Option<i64> {
        let exact_candidate = candidate_terms.join(" ");
        let mut best: Option<FieldMatch> = None;

        for (field, value) in annotation_field_values(node) {
            let normalized = self.normalized_value(&value);
            if normalized.is_empty() {
                continue;
            }

            let field_terms: Vec<String> = normalized
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect();
            if field_terms.is_empty() {
                continue;
            }

            let match_score = if exact_candidate == normalized {
                Some(FieldMatch {
                    score: field.exact_score(),
                })
            } else if candidate_terms.len() > 1
                && contains_subsequence(&field_terms, candidate_terms)
            {
                Some(FieldMatch {
                    score: field.overlap_score(),
                })
            } else {
                None
            };

            if let Some(mut match_score) = match_score {
                match_score.score += candidate_terms.len() as i64 * 4;
                let bm25_bonus = (((bm25_score.max(0.0) as f64) * self.config.bm25_weight).round()
                    as i64)
                    .saturating_add((12_i64 - rank as i64).max(0));
                match_score.score += bm25_bonus;
                best = match best {
                    Some(current) if current.score >= match_score.score => Some(current),
                    _ => Some(match_score),
                };
            }
        }

        let best = best?;
        let min_score = if candidate_terms.len() == 1 {
            self.config.min_score_one_token
        } else {
            self.config.min_score_multi_token
        };
        if best.score < min_score {
            return None;
        }
        Some(best.score)
    }

    fn normalized_value(&mut self, value: &str) -> String {
        if let Some(cached) = self.normalized_terms_cache.get(value) {
            return cached.clone();
        }

        let normalized = text_norm::normalize_text(value);
        if !normalized.is_empty() {
            self.normalized_terms_cache
                .insert(value.to_owned(), normalized.clone());
        }
        normalized
    }
}

pub(crate) fn collect_annotations(
    graph: &GraphFile,
    text: &str,
    index: Option<&Bm25Index>,
) -> Vec<AnnotationCandidate> {
    AnnotateContext::new(graph, index).collect_annotations(text)
}

pub(crate) fn render_annotated_text(text: &str, annotations: &[AnnotationCandidate]) -> String {
    if annotations.is_empty() {
        return text.to_owned();
    }

    let mut selected = annotations.to_vec();
    selected.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });

    let mut out = String::with_capacity(text.len() + selected.len() * 32);
    let mut cursor = 0usize;
    for annotation in selected {
        if annotation.start < cursor {
            continue;
        }
        out.push_str(&text[cursor..annotation.start]);
        let matched = &text[annotation.start..annotation.end];
        out.push_str(matched);
        out.push_str(" [kg ");
        out.push_str(matched);
        out.push_str(" @");
        out.push_str(&annotation.node_id);
        out.push(']');
        cursor = annotation.end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn select_non_overlapping(mut candidates: Vec<AnnotationCandidate>) -> Vec<AnnotationCandidate> {
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| (right.end - right.start).cmp(&(left.end - left.start)))
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.node_id.cmp(&right.node_id))
    });

    let mut selected = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|existing: &AnnotationCandidate| overlaps(existing, &candidate))
        {
            continue;
        }
        selected.push(candidate);
    }

    selected.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });
    selected
}

fn overlaps(left: &AnnotationCandidate, right: &AnnotationCandidate) -> bool {
    left.start < right.end && right.start < left.end
}

fn is_better_annotation(left: &AnnotationCandidate, right: &AnnotationCandidate) -> bool {
    left.score > right.score
        || (left.score == right.score && (left.end - left.start) > (right.end - right.start))
        || (left.score == right.score
            && (left.end - left.start) == (right.end - right.start)
            && left.node_id < right.node_id)
}

fn contains_subsequence(haystack: &[String], needle: &[String]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn annotation_field_values(node: &Node) -> Vec<(AnnotateField, String)> {
    let mut values = Vec::new();
    values.push((AnnotateField::Name, node.name.clone()));
    if let Some((_, suffix)) = node.id.split_once(':') {
        values.push((AnnotateField::Id, suffix.to_owned()));
    } else {
        values.push((AnnotateField::Id, node.id.clone()));
    }
    for alias in &node.properties.alias {
        values.push((AnnotateField::Alias, alias.clone()));
    }
    for fact in &node.properties.key_facts {
        values.push((AnnotateField::Fact, fact.clone()));
    }
    values
}

fn node_ref(node: &Node) -> String {
    let code = node_type_code(&node.r#type);
    if let Some((_, suffix)) = node.id.split_once(':') {
        format!("{code}:{suffix}")
    } else {
        format!("{code}:{}", node.id)
    }
}

fn node_type_code(node_type: &str) -> &str {
    match node_type {
        "Feature" => "F",
        "Concept" => "K",
        "Interface" => "I",
        "Process" => "P",
        "DataStore" => "D",
        "Attribute" => "A",
        "Entity" => "Y",
        "Note" => "N",
        "Rule" => "R",
        "Convention" => "C",
        "Bug" => "B",
        "Decision" => "Z",
        "OpenQuestion" => "O",
        "Claim" => "Q",
        "Insight" => "W",
        "Reference" => "M",
        "Term" => "T",
        "Status" => "S",
        "Doubt" => "L",
        _ => node_type,
    }
}
