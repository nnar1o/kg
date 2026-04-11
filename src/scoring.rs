use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::graph::{Edge, EdgeProperties, GraphFile};

const METADATA_NODE_TYPE: &str = "^";
use crate::text_norm;

pub(crate) struct ScoreAllConfig {
    pub min_desc_len: usize,
    pub desc_weight: f64,
    pub bundle_weight: f64,
    pub cluster_seed: u64,
    pub cluster_resolution: f64,
    pub membership_top_k: usize,
}

pub(crate) struct ScoreAllOutcome {
    pub path: PathBuf,
    pub pairs: usize,
    pub edges: usize,
    pub clusters: usize,
}

trait PairScoreCalculator {
    fn key(&self) -> &'static str;
    fn score(&self, left: &NodeProfile, right: &NodeProfile, ctx: &ScoreContext) -> f64;
}

struct DescriptionRepeatCalculator;
struct AttributeBundleCalculator;

#[derive(Debug, Default)]
struct ScoreContext {
    desc_idf: HashMap<String, f64>,
    bundle_idf: HashMap<String, f64>,
}

impl PairScoreCalculator for DescriptionRepeatCalculator {
    fn key(&self) -> &'static str {
        "C1"
    }

    fn score(&self, left: &NodeProfile, right: &NodeProfile, ctx: &ScoreContext) -> f64 {
        weighted_overlap_score(&left.desc_shingles, &right.desc_shingles, &ctx.desc_idf)
    }
}

impl PairScoreCalculator for AttributeBundleCalculator {
    fn key(&self) -> &'static str {
        "C2"
    }

    fn score(&self, left: &NodeProfile, right: &NodeProfile, ctx: &ScoreContext) -> f64 {
        weighted_overlap_score(&left.bundle_tokens, &right.bundle_tokens, &ctx.bundle_idf)
    }
}

#[derive(Debug, Default)]
struct NodeProfile {
    desc_shingles: HashSet<String>,
    bundle_tokens: HashSet<String>,
}

pub(crate) fn compute_all_pair_scores_to_cache(
    source_graph: &GraphFile,
    source_path: &Path,
    config: &ScoreAllConfig,
) -> Result<ScoreAllOutcome> {
    let profiles = build_profiles(source_graph, config.min_desc_len);
    let context = build_score_context(&profiles, source_graph.nodes.len());
    let calculators: Vec<Box<dyn PairScoreCalculator>> = vec![
        Box::new(DescriptionRepeatCalculator),
        Box::new(AttributeBundleCalculator),
    ];
    let weights = HashMap::from([
        ("C1", clamp_01(config.desc_weight)),
        ("C2", clamp_01(config.bundle_weight)),
    ]);

    let mut result = GraphFile::new(&format!("{}.score", source_graph.metadata.name));
    result.metadata.description =
        "Similarity scores and derived clusters (normalization=v2)".to_owned();
    result.nodes = source_graph.nodes.clone();
    result.notes.clear();
    result.edges.clear();

    let mut pair_count = 0usize;
    let candidate_nodes: Vec<&crate::graph::Node> = source_graph
        .nodes
        .iter()
        .filter(|node| node.r#type != METADATA_NODE_TYPE)
        .collect();

    for i in 0..candidate_nodes.len() {
        for j in (i + 1)..candidate_nodes.len() {
            pair_count += 1;
            let left = candidate_nodes[i];
            let right = candidate_nodes[j];
            let left_profile = profiles.get(left.id.as_str()).expect("profile exists");
            let right_profile = profiles.get(right.id.as_str()).expect("profile exists");

            let mut raw_scores: HashMap<&'static str, f64> = HashMap::new();
            for calc in &calculators {
                raw_scores.insert(
                    calc.key(),
                    clamp_01(calc.score(left_profile, right_profile, &context)),
                );
            }

            let score = weighted_score(&raw_scores, &weights);
            let mut score_components = BTreeMap::new();
            for (key, value) in &raw_scores {
                score_components.insert((*key).to_owned(), *value);
            }
            let (source_id, target_id) = canonical_pair(&left.id, &right.id);

            result.edges.push(Edge {
                source_id,
                relation: "~".to_owned(),
                target_id,
                properties: EdgeProperties {
                    detail: format_score(score),
                    bidirectional: true,
                    score_components,
                    ..EdgeProperties::default()
                },
            });
        }
    }

    let cluster_count = append_cluster_nodes_and_edges(
        &mut result,
        config.cluster_seed,
        config.cluster_resolution,
        config.membership_top_k,
    );

    result.refresh_counts();

    let out_path = cache_score_path(source_path);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    result.save(&out_path)?;

    Ok(ScoreAllOutcome {
        path: out_path,
        pairs: pair_count,
        edges: result.edges.len(),
        clusters: cluster_count,
    })
}

fn append_cluster_nodes_and_edges(
    graph: &mut GraphFile,
    seed: u64,
    resolution: f64,
    membership_top_k: usize,
) -> usize {
    let node_ids: Vec<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    let mut pair_weights: HashMap<(String, String), f64> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    for edge in &graph.edges {
        if !edge.properties.bidirectional || edge.relation != "~" {
            continue;
        }
        let score = edge.properties.detail.trim().parse::<f64>().unwrap_or(0.0);
        if score <= 0.0 {
            continue;
        }
        let score = clamp_01(score);
        let (left, right) = canonical_pair(&edge.source_id, &edge.target_id);
        pair_weights.insert((left.clone(), right.clone()), score);
        adjacency
            .entry(left.clone())
            .or_default()
            .push((right.clone(), score));
        adjacency.entry(right).or_default().push((left, score));
    }

    let communities = detect_communities(&node_ids, &adjacency, seed, resolution);
    let mut grouped: HashMap<usize, Vec<String>> = HashMap::new();
    for node_id in &node_ids {
        if let Some(label) = communities.get(node_id) {
            grouped.entry(*label).or_default().push(node_id.clone());
        }
    }

    let mut clusters: Vec<Vec<String>> = grouped
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|mut members| {
            members.sort();
            members
        })
        .collect();
    clusters.sort_by(|a, b| a[0].cmp(&b[0]));

    let top_k = membership_top_k.max(1);
    for (idx, members) in clusters.iter().enumerate() {
        let cluster_id = format!("@:cluster_{:04}", idx + 1);
        graph.nodes.push(crate::Node {
            id: cluster_id.clone(),
            r#type: "@".to_owned(),
            name: format!("Cluster {:04}", idx + 1),
            properties: crate::NodeProperties {
                description: format!(
                    "Derived similarity cluster v1 (size={}, seed={}, resolution={:.3})",
                    members.len(),
                    seed,
                    resolution
                ),
                domain_area: "derived_cluster_v1".to_owned(),
                provenance: "A".to_owned(),
                importance: 1.0,
                ..Default::default()
            },
            source_files: vec!["DOC .kg/cache/derived-clusters".to_owned()],
        });

        for member in members {
            let strength = membership_strength(member, members, &pair_weights, top_k);
            graph.edges.push(Edge {
                source_id: cluster_id.clone(),
                relation: "HAS".to_owned(),
                target_id: member.clone(),
                properties: EdgeProperties {
                    detail: format_score(strength),
                    ..EdgeProperties::default()
                },
            });
        }
    }

    clusters.len()
}

fn detect_communities(
    node_ids: &[String],
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    seed: u64,
    resolution: f64,
) -> HashMap<String, usize> {
    let mut sorted = node_ids.to_vec();
    sorted.sort();
    let mut labels: HashMap<String, usize> = sorted
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.clone(), idx))
        .collect();
    if sorted.is_empty() {
        return labels;
    }

    let shift = (seed as usize) % sorted.len();
    let mut order = sorted.clone();
    order.rotate_left(shift);
    let gamma = resolution.max(0.05);

    for _ in 0..32 {
        let mut changed = false;
        for node in &order {
            let neighbors = adjacency.get(node).cloned().unwrap_or_default();
            if neighbors.is_empty() {
                continue;
            }
            let current = labels.get(node).copied().unwrap_or(0);
            let mut community_weight: HashMap<usize, f64> = HashMap::new();
            for (neighbor, score) in neighbors {
                if let Some(label) = labels.get(&neighbor).copied() {
                    *community_weight.entry(label).or_insert(0.0) += score.powf(gamma);
                }
            }

            let mut best_label = current;
            let mut best_score = *community_weight.get(&current).unwrap_or(&0.0);
            for (label, score) in community_weight {
                let better = score > best_score + 1e-12
                    || ((score - best_score).abs() <= 1e-12 && label < best_label);
                if better {
                    best_label = label;
                    best_score = score;
                }
            }
            if best_label != current {
                labels.insert(node.clone(), best_label);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    labels
}

fn membership_strength(
    node_id: &str,
    members: &[String],
    pair_weights: &HashMap<(String, String), f64>,
    top_k: usize,
) -> f64 {
    let mut values: Vec<f64> = members
        .iter()
        .filter(|other| other.as_str() != node_id)
        .map(|other| {
            let (left, right) = canonical_pair(node_id, other);
            pair_weights
                .get(&(left, right))
                .copied()
                .unwrap_or_default()
        })
        .collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    values.truncate(top_k.min(values.len()));
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        clamp_01(values[mid])
    } else {
        clamp_01((values[mid - 1] + values[mid]) / 2.0)
    }
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

fn build_profiles(graph: &GraphFile, min_desc_len: usize) -> HashMap<String, NodeProfile> {
    let node_type_by_id: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.r#type.as_str()))
        .collect();
    let mut out = HashMap::new();

    for node in &graph.nodes {
        let mut profile = NodeProfile::default();

        let normalized_desc = text_norm::normalize_text(&node.properties.description);
        if normalized_desc.len() >= min_desc_len {
            profile.desc_shingles = word_shingles(&normalized_desc, 3);
        }

        profile
            .bundle_tokens
            .insert(format!("type:{}", text_norm::normalize_text(&node.r#type)));
        insert_value_tokens(&mut profile.bundle_tokens, "name", &node.name);
        insert_value_tokens(
            &mut profile.bundle_tokens,
            "domain",
            &node.properties.domain_area,
        );
        insert_value_tokens(
            &mut profile.bundle_tokens,
            "prov",
            &node.properties.provenance,
        );

        for alias in &node.properties.alias {
            insert_value_tokens(&mut profile.bundle_tokens, "alias", alias);
        }
        for fact in &node.properties.key_facts {
            insert_value_tokens(&mut profile.bundle_tokens, "fact", fact);
        }
        for source in &node.source_files {
            insert_value_tokens(&mut profile.bundle_tokens, "src", source);
        }

        for edge in graph.edges.iter().filter(|edge| edge.source_id == node.id) {
            let target_type = node_type_by_id
                .get(edge.target_id.as_str())
                .copied()
                .unwrap_or("unknown");
            profile.bundle_tokens.insert(format!(
                "out:{}:{}:{}",
                text_norm::normalize_text(&edge.relation),
                text_norm::normalize_text(target_type),
                text_norm::normalize_text(&edge.target_id)
            ));
        }
        for edge in graph.edges.iter().filter(|edge| edge.target_id == node.id) {
            let source_type = node_type_by_id
                .get(edge.source_id.as_str())
                .copied()
                .unwrap_or("unknown");
            profile.bundle_tokens.insert(format!(
                "in:{}:{}:{}",
                text_norm::normalize_text(&edge.relation),
                text_norm::normalize_text(source_type),
                text_norm::normalize_text(&edge.source_id)
            ));
        }

        out.insert(node.id.clone(), profile);
    }

    out
}

fn build_score_context(profiles: &HashMap<String, NodeProfile>, doc_count: usize) -> ScoreContext {
    let desc_sets: Vec<&HashSet<String>> = profiles.values().map(|p| &p.desc_shingles).collect();
    let bundle_sets: Vec<&HashSet<String>> = profiles.values().map(|p| &p.bundle_tokens).collect();
    ScoreContext {
        desc_idf: build_idf(&desc_sets, doc_count),
        bundle_idf: build_idf(&bundle_sets, doc_count),
    }
}

fn build_idf(token_sets: &[&HashSet<String>], doc_count: usize) -> HashMap<String, f64> {
    let mut df: HashMap<String, usize> = HashMap::new();
    for set in token_sets {
        for token in *set {
            *df.entry(token.clone()).or_insert(0) += 1;
        }
    }
    let n = doc_count.max(1) as f64;
    let mut idf = HashMap::new();
    for (token, count) in df {
        let c = count as f64;
        let value = ((n + 1.0) / (c + 1.0)).ln() + 1.0;
        idf.insert(token, value.max(0.05));
    }
    idf
}

fn insert_value_tokens(tokens: &mut HashSet<String>, key: &str, raw: &str) {
    let normalized = text_norm::normalize_text(raw);
    if normalized.is_empty() {
        return;
    }
    for token in normalized.split_whitespace() {
        tokens.insert(format!("{}:{}", key, token));
    }
}

fn word_shingles(text: &str, width: usize) -> HashSet<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return HashSet::new();
    }
    if words.len() < width {
        return HashSet::from([words.join(" ")]);
    }
    let mut out = HashSet::new();
    for i in 0..=(words.len() - width) {
        out.insert(words[i..(i + width)].join(" "));
    }
    out
}

#[cfg(test)]
fn overlap_score(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersect = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    let min_size = left.len().min(right.len()) as f64;
    let jaccard = if union == 0.0 { 0.0 } else { intersect / union };
    let containment = if min_size == 0.0 {
        0.0
    } else {
        intersect / min_size
    };
    clamp_01(0.35 * jaccard + 0.65 * containment)
}

fn weighted_overlap_score(
    left: &HashSet<String>,
    right: &HashSet<String>,
    token_weights: &HashMap<String, f64>,
) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let weight = |token: &String| token_weights.get(token).copied().unwrap_or(1.0);
    let intersect: f64 = left.intersection(right).map(weight).sum();
    let union: f64 = left.union(right).map(weight).sum();
    let left_sum: f64 = left.iter().map(weight).sum();
    let right_sum: f64 = right.iter().map(weight).sum();
    let min_sum = left_sum.min(right_sum);

    let jaccard = if union == 0.0 { 0.0 } else { intersect / union };
    let containment = if min_sum == 0.0 {
        0.0
    } else {
        intersect / min_sum
    };
    clamp_01(0.35 * jaccard + 0.65 * containment)
}

fn weighted_score(
    scores: &HashMap<&'static str, f64>,
    weights: &HashMap<&'static str, f64>,
) -> f64 {
    let mut sum_weights = 0.0;
    let mut weighted = 0.0;
    for (key, score) in scores {
        let weight = weights.get(key).copied().unwrap_or(0.0);
        if weight > 0.0 {
            weighted += weight * clamp_01(*score);
            sum_weights += weight;
        }
    }
    if sum_weights == 0.0 {
        return 0.0;
    }
    clamp_01(weighted / sum_weights)
}

fn format_score(score: f64) -> String {
    format!("{:.6}", clamp_01(score))
}

fn clamp_01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn cache_score_path(source_graph_path: &Path) -> PathBuf {
    let cache_dir = crate::cache_paths::cache_root_for_graph(source_graph_path);
    let stem = source_graph_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("graph");
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    cache_dir.join(format!("{stem}.score.{ts_ms}.kg"))
}

#[cfg(test)]
mod tests {
    use super::{
        ScoreAllConfig, append_cluster_nodes_and_edges, compute_all_pair_scores_to_cache,
        detect_communities, overlap_score, weighted_score, word_shingles,
    };
    use crate::graph::{EdgeProperties, GraphFile, Node, NodeProperties};
    use crate::text_norm;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn overlap_score_prefers_subset_match() {
        let left = HashSet::from(["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        let right = HashSet::from(["a".to_owned(), "b".to_owned()]);
        let score = overlap_score(&left, &right);
        assert!(score > 0.7, "score={score}");
    }

    #[test]
    fn weighted_score_normalizes_weights() {
        let scores = HashMap::from([("C1", 0.8), ("C2", 0.2)]);
        let weights = HashMap::from([("C1", 4.0), ("C2", 1.0)]);
        let score = weighted_score(&scores, &weights);
        assert!(score > 0.65 && score < 0.75, "score={score}");
    }

    #[test]
    fn shingles_fallback_to_single_fragment_for_short_text() {
        let shingles = word_shingles("short text", 3);
        assert_eq!(shingles, HashSet::from(["short text".to_owned()]));
    }

    #[test]
    fn normalize_text_removes_stopwords_and_numbers() {
        let normalized = text_norm::normalize_text("The 123 cooling and 45 heating system");
        assert_eq!(normalized, "cool heat system");
    }

    #[test]
    fn normalize_text_handles_polish_stopwords() {
        let normalized = text_norm::normalize_text("to jest test i analiza danych");
        assert_eq!(normalized, "test analiza danych");
    }

    #[test]
    fn normalize_text_keeps_contextual_alphanumeric_numbers() {
        let normalized = text_norm::normalize_text("API v2 on S3 with latency 10ms and 42");
        assert_eq!(normalized, "api v2 s3 latency 10m");
    }

    #[test]
    fn normalize_text_maps_default_synonyms() {
        let normalized = text_norm::normalize_text("Auth login for fridge database");
        assert_eq!(
            normalized,
            "authentication authentication refrigeration data_store"
        );
    }

    #[test]
    fn score_all_writes_component_scores_and_final_score() {
        let mut graph = GraphFile::new("demo");
        graph.nodes.push(Node {
            id: "concept:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Alpha Device".to_owned(),
            properties: NodeProperties {
                description: "This is a long cooling appliance description with many repeated words for similarity".to_owned(),
                provenance: "U".to_owned(),
                importance: 0.5,
                ..Default::default()
            },
            source_files: vec!["docs/a.md".to_owned()],
        });
        graph.nodes.push(Node {
            id: "concept:b".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Alpha Cooler".to_owned(),
            properties: NodeProperties {
                description: "This is a long cooling appliance description with many shared words for similarity".to_owned(),
                provenance: "U".to_owned(),
                importance: 0.5,
                ..Default::default()
            },
            source_files: vec!["docs/b.md".to_owned()],
        });

        let dir = tempfile::tempdir().expect("tempdir");
        let source_path = dir.path().join("demo.kg");
        let outcome = compute_all_pair_scores_to_cache(
            &graph,
            &source_path,
            &ScoreAllConfig {
                min_desc_len: 20,
                desc_weight: 0.45,
                bundle_weight: 0.55,
                cluster_seed: 7,
                cluster_resolution: 1.0,
                membership_top_k: 3,
            },
        )
        .expect("score all");

        assert_eq!(outcome.pairs, 1);
        let scored = GraphFile::load(&outcome.path).expect("load scored graph");
        assert!(scored.edges.iter().any(|edge| edge.relation == "~"));
        let edge = scored
            .edges
            .iter()
            .find(|edge| edge.relation == "~")
            .expect("similarity edge");
        assert!(edge.properties.score_components.contains_key("C1"));
        assert!(edge.properties.score_components.contains_key("C2"));
        assert!(!edge.properties.detail.is_empty());
        assert!(scored.nodes.iter().any(|node| node.r#type == "@"));
        assert!(
            scored
                .edges
                .iter()
                .any(|edge| edge.relation == "HAS" && edge.source_id.starts_with("@:cluster_"))
        );
    }

    #[test]
    fn detect_communities_is_deterministic_for_fixed_seed() {
        let node_ids = vec![
            "n:a".to_owned(),
            "n:b".to_owned(),
            "n:c".to_owned(),
            "n:d".to_owned(),
        ];
        let adjacency = HashMap::from([
            (
                "n:a".to_owned(),
                vec![
                    ("n:b".to_owned(), 0.95),
                    ("n:c".to_owned(), 0.10),
                    ("n:d".to_owned(), 0.05),
                ],
            ),
            (
                "n:b".to_owned(),
                vec![
                    ("n:a".to_owned(), 0.95),
                    ("n:c".to_owned(), 0.05),
                    ("n:d".to_owned(), 0.10),
                ],
            ),
            (
                "n:c".to_owned(),
                vec![
                    ("n:d".to_owned(), 0.93),
                    ("n:a".to_owned(), 0.10),
                    ("n:b".to_owned(), 0.05),
                ],
            ),
            (
                "n:d".to_owned(),
                vec![
                    ("n:c".to_owned(), 0.93),
                    ("n:a".to_owned(), 0.05),
                    ("n:b".to_owned(), 0.10),
                ],
            ),
        ]);

        let first = detect_communities(&node_ids, &adjacency, 17, 1.0);
        let second = detect_communities(&node_ids, &adjacency, 17, 1.0);
        assert_eq!(first, second);
    }

    #[test]
    fn append_clusters_ignores_singletons() {
        let mut graph = GraphFile::new("clusters");
        for id in ["concept:a", "concept:b", "concept:c"] {
            graph.nodes.push(Node {
                id: id.to_owned(),
                r#type: "Concept".to_owned(),
                name: id.to_owned(),
                properties: NodeProperties {
                    description: "desc".to_owned(),
                    provenance: "U".to_owned(),
                    importance: 1.0,
                    ..Default::default()
                },
                source_files: vec!["DOC docs/x.md".to_owned()],
            });
        }
        graph.edges.push(crate::Edge {
            source_id: "concept:a".to_owned(),
            relation: "~".to_owned(),
            target_id: "concept:b".to_owned(),
            properties: EdgeProperties {
                detail: "0.92".to_owned(),
                bidirectional: true,
                ..Default::default()
            },
        });

        let count = append_cluster_nodes_and_edges(&mut graph, 42, 1.0, 3);
        assert_eq!(count, 1);
        assert!(
            !graph
                .edges
                .iter()
                .any(|edge| edge.relation == "HAS" && edge.target_id == "concept:c")
        );
    }
}
