use std::path::PathBuf;

use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::cache_paths;
use crate::cli::{ClusterSkill, ClustersArgs};
use crate::graph::GraphFile;

#[derive(Debug, Serialize)]
pub(crate) struct ClusterView {
    id: String,
    size: usize,
    relevance: f64,
    members: Vec<(String, f64)>,
}

pub(crate) fn execute_clusters(
    graph: &GraphFile,
    path: &std::path::Path,
    args: &ClustersArgs,
) -> Result<String> {
    let source_graph = resolve_cluster_source_graph(graph, path)?;
    Ok(render_clusters(&source_graph, args))
}

fn resolve_cluster_source_graph(graph: &GraphFile, path: &std::path::Path) -> Result<GraphFile> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if filename.contains(".score.") {
        return Ok(graph.clone());
    }

    let latest = find_latest_score_snapshot(path)?.ok_or_else(|| {
        anyhow!(
            "no score cache found for '{}'; run `kg graph {} score-all` first",
            path.display(),
            graph.metadata.name
        )
    })?;
    GraphFile::load(&latest)
}

pub(crate) fn find_latest_score_snapshot(path: &std::path::Path) -> Result<Option<PathBuf>> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid graph filename"))?;
    let prefix = format!("{stem}.score.");
    let suffix = ".kg";
    let mut latest: Option<(u128, PathBuf)> = None;

    let cache_dir = cache_paths::cache_root_for_graph(path);
    let Ok(entries) = std::fs::read_dir(&cache_dir) else {
        return Ok(None);
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) || !name.ends_with(suffix) {
            continue;
        }
        let ts_part = &name[prefix.len()..name.len() - suffix.len()];
        let Ok(ts) = ts_part.parse::<u128>() else {
            continue;
        };
        if latest.as_ref().map(|(curr, _)| ts > *curr).unwrap_or(true) {
            latest = Some((ts, entry.path()));
        }
    }

    Ok(latest.map(|(_, path)| path))
}

pub(crate) fn render_clusters(graph: &GraphFile, args: &ClustersArgs) -> String {
    let mut clusters: Vec<ClusterView> = graph
        .nodes
        .iter()
        .filter(|node| node.r#type == "@" && node.id.starts_with("@:cluster_"))
        .map(|cluster| {
            let mut members: Vec<(String, f64)> = graph
                .edges
                .iter()
                .filter(|edge| edge.source_id == cluster.id && edge.relation == "HAS")
                .map(|edge| {
                    (
                        edge.target_id.clone(),
                        edge.properties.detail.parse::<f64>().unwrap_or(0.0),
                    )
                })
                .collect();
            members.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let relevance = if members.is_empty() {
                0.0
            } else {
                members.iter().map(|(_, v)| *v).sum::<f64>() / members.len() as f64
            };
            ClusterView {
                id: cluster.id.clone(),
                size: members.len(),
                relevance,
                members,
            }
        })
        .collect();

    clusters.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| a.id.cmp(&b.id))
    });
    clusters.truncate(args.limit);

    if args.json {
        return serde_json::to_string_pretty(&clusters).unwrap_or_else(|_| "[]".to_owned());
    }

    if matches!(args.skill, Some(ClusterSkill::Gardener)) {
        let mut lines = vec![format!("= gardener clusters ({})", clusters.len())];
        for cluster in &clusters {
            let top = cluster
                .members
                .iter()
                .take(3)
                .map(|(id, score)| format!("{id} ({score:.3})"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "- {} | relevance {:.3} | size {} | top: {}",
                cluster.id, cluster.relevance, cluster.size, top
            ));
            lines.push(format!(
                "- action: review cluster {}, merge aliases/facts, then keep strongest node as canonical",
                cluster.id
            ));
        }
        return format!("{}\n", lines.join("\n"));
    }

    let mut lines = vec![format!("= clusters ({})", clusters.len())];
    for cluster in &clusters {
        let top = cluster
            .members
            .iter()
            .take(5)
            .map(|(id, score)| format!("{id}:{score:.3}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "- {} | relevance {:.3} | size {} | top {}",
            cluster.id, cluster.relevance, cluster.size, top
        ));
    }
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};

    fn cluster_graph() -> GraphFile {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(Node {
            id: "@:cluster_a".to_owned(),
            r#type: "@".to_owned(),
            name: "Cluster A".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.nodes.push(Node {
            id: "n:1".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Normal".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.edges.push(Edge {
            source_id: "@:cluster_a".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "n:1".to_owned(),
            properties: EdgeProperties {
                detail: "0.85".to_owned(),
                ..EdgeProperties::default()
            },
        });
        graph
    }

    #[test]
    fn render_clusters_no_clusters() {
        let graph = GraphFile::new("test");
        let args = ClustersArgs { limit: 10, json: false, skill: None };
        let out = render_clusters(&graph, &args);
        assert_eq!(out, "= clusters (0)\n");
    }

    #[test]
    fn render_clusters_with_members() {
        let graph = cluster_graph();
        let args = ClustersArgs { limit: 10, json: false, skill: None };
        let out = render_clusters(&graph, &args);
        assert!(out.contains("cluster_a"));
        assert!(out.contains("n:1:0.850"));
    }

    #[test]
    fn render_clusters_json_output() {
        let graph = cluster_graph();
        let args = ClustersArgs { limit: 10, json: true, skill: None };
        let out = render_clusters(&graph, &args);
        assert!(out.contains("\"id\": \"@:cluster_a\""));
    }

    #[test]
    fn render_clusters_gardener_mode_emits_actions() {
        let graph = cluster_graph();
        let args = ClustersArgs { limit: 10, json: false, skill: Some(crate::cli::ClusterSkill::Gardener) };
        let out = render_clusters(&graph, &args);
        assert!(out.contains("action:"));
        assert!(out.contains("cluster_a"));
    }
}
