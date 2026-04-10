use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::graph::{Edge, EdgeProperties, GraphFile};

pub(crate) struct ScoreAllConfig {
    pub min_desc_len: usize,
    pub desc_weight: f64,
    pub bundle_weight: f64,
}

pub(crate) struct ScoreAllOutcome {
    pub path: PathBuf,
    pub pairs: usize,
    pub edges: usize,
}

trait PairScoreCalculator {
    fn key(&self) -> &'static str;
    fn score(&self, left: &NodeProfile, right: &NodeProfile) -> f64;
}

struct DescriptionRepeatCalculator;
struct AttributeBundleCalculator;

impl PairScoreCalculator for DescriptionRepeatCalculator {
    fn key(&self) -> &'static str {
        "C1"
    }

    fn score(&self, left: &NodeProfile, right: &NodeProfile) -> f64 {
        overlap_score(&left.desc_shingles, &right.desc_shingles)
    }
}

impl PairScoreCalculator for AttributeBundleCalculator {
    fn key(&self) -> &'static str {
        "C2"
    }

    fn score(&self, left: &NodeProfile, right: &NodeProfile) -> f64 {
        overlap_score(&left.bundle_tokens, &right.bundle_tokens)
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
    let calculators: Vec<Box<dyn PairScoreCalculator>> = vec![
        Box::new(DescriptionRepeatCalculator),
        Box::new(AttributeBundleCalculator),
    ];
    let weights = HashMap::from([
        ("C1", clamp_01(config.desc_weight)),
        ("C2", clamp_01(config.bundle_weight)),
    ]);

    let mut result = GraphFile::new(&format!("{}.score", source_graph.metadata.name));
    result.nodes = source_graph.nodes.clone();
    result.notes.clear();
    result.edges.clear();

    let mut pair_count = 0usize;
    for i in 0..source_graph.nodes.len() {
        for j in (i + 1)..source_graph.nodes.len() {
            pair_count += 1;
            let left = &source_graph.nodes[i];
            let right = &source_graph.nodes[j];
            let left_profile = profiles.get(left.id.as_str()).expect("profile exists");
            let right_profile = profiles.get(right.id.as_str()).expect("profile exists");

            let mut raw_scores: HashMap<&'static str, f64> = HashMap::new();
            for calc in &calculators {
                raw_scores.insert(
                    calc.key(),
                    clamp_01(calc.score(left_profile, right_profile)),
                );
            }

            let score = weighted_score(&raw_scores, &weights);
            let mut score_components = BTreeMap::new();
            for (key, value) in &raw_scores {
                score_components.insert((*key).to_owned(), *value);
            }
            let (source_id, target_id) = if left.id <= right.id {
                (left.id.clone(), right.id.clone())
            } else {
                (right.id.clone(), left.id.clone())
            };

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
    })
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

        let normalized_desc = normalize_text(&node.properties.description);
        if normalized_desc.len() >= min_desc_len {
            profile.desc_shingles = word_shingles(&normalized_desc, 3);
        }

        profile
            .bundle_tokens
            .insert(format!("type:{}", normalize_text(&node.r#type)));
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
                normalize_text(&edge.relation),
                normalize_text(target_type),
                normalize_text(&edge.target_id)
            ));
        }
        for edge in graph.edges.iter().filter(|edge| edge.target_id == node.id) {
            let source_type = node_type_by_id
                .get(edge.source_id.as_str())
                .copied()
                .unwrap_or("unknown");
            profile.bundle_tokens.insert(format!(
                "in:{}:{}:{}",
                normalize_text(&edge.relation),
                normalize_text(source_type),
                normalize_text(&edge.source_id)
            ));
        }

        out.insert(node.id.clone(), profile);
    }

    out
}

fn insert_value_tokens(tokens: &mut HashSet<String>, key: &str, raw: &str) {
    let normalized = normalize_text(raw);
    if normalized.is_empty() {
        return;
    }
    tokens.insert(format!("{}:{}", key, normalized));
}

fn normalize_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_sep = true;
    for ch in raw.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_sep = false;
        } else if !prev_sep {
            out.push(' ');
            prev_sep = true;
        }
    }
    out.trim().to_owned()
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
        ScoreAllConfig, compute_all_pair_scores_to_cache, overlap_score, weighted_score,
        word_shingles,
    };
    use crate::graph::{GraphFile, Node, NodeProperties};
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
            },
        )
        .expect("score all");

        assert_eq!(outcome.pairs, 1);
        let scored = GraphFile::load(&outcome.path).expect("load scored graph");
        assert_eq!(scored.edges.len(), 1);
        let edge = &scored.edges[0];
        assert!(edge.properties.score_components.contains_key("C1"));
        assert!(edge.properties.score_components.contains_key("C2"));
        assert!(!edge.properties.detail.is_empty());
    }
}
