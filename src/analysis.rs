use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use strsim::normalized_levenshtein;

use crate::cli::{
    DuplicatesArgs, EdgeGapsArgs, MissingDescriptionsArgs, MissingFactsArgs, MissingFactsSort,
    StatsArgs,
};
use crate::graph::{GraphFile, Node};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn filtered_nodes<'a>(
    graph: &'a GraphFile,
    node_types: &[String],
    include_features: bool,
) -> Vec<&'a Node> {
    graph
        .nodes
        .iter()
        .filter(|node| include_features || node.r#type != "Feature")
        .filter(|node| node_types.is_empty() || node_types.iter().any(|t| t == &node.r#type))
        .collect()
}

pub fn edge_counts(graph: &GraphFile) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for edge in &graph.edges {
        *counts.entry(edge.source_id.as_str()).or_insert(0) += 1;
        *counts.entry(edge.target_id.as_str()).or_insert(0) += 1;
    }
    counts
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

pub fn render_stats(graph: &GraphFile, args: &StatsArgs) -> String {
    let nodes = filtered_nodes(graph, &[], args.include_features);
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut lines = vec!["= stats".to_owned()];
    lines.push(format!("nodes: {}", nodes.len()));
    lines.push(format!("edges: {}", graph.edges.len()));

    if args.by_type {
        let mut by_type = BTreeMap::<String, usize>::new();
        for node in &nodes {
            *by_type.entry(node.r#type.clone()).or_insert(0) += 1;
        }
        lines.push("types:".to_owned());
        for (node_type, count) in by_type {
            lines.push(format!("  - {node_type}: {count}"));
        }
    }

    if args.by_relation {
        let mut by_relation = BTreeMap::<String, usize>::new();
        for edge in &graph.edges {
            if node_ids.contains(edge.source_id.as_str())
                && node_ids.contains(edge.target_id.as_str())
            {
                *by_relation.entry(edge.relation.clone()).or_insert(0) += 1;
            }
        }
        lines.push("relations:".to_owned());
        for (relation, count) in by_relation {
            lines.push(format!("  - {relation}: {count}"));
        }
    }

    if args.show_sources {
        let mut sources = BTreeSet::new();
        for node in nodes {
            for source in &node.source_files {
                sources.insert(source.clone());
            }
        }
        lines.push(format!("sources: {}", sources.len()));
    }

    format!("{}\n", lines.join("\n"))
}

pub fn render_missing_descriptions(graph: &GraphFile, args: &MissingDescriptionsArgs) -> String {
    let mut missing: Vec<&Node> = filtered_nodes(graph, &args.node_types, args.include_features)
        .into_iter()
        .filter(|node| node.properties.description.trim().is_empty())
        .collect();
    missing.sort_by_key(|node| (node.r#type.clone(), node.id.clone()));

    let mut lines = vec![format!("= missing-descriptions ({})", missing.len())];
    for node in missing.into_iter().take(args.limit) {
        lines.push(format!("- {} | {} | {}", node.r#type, node.id, node.name));
    }
    format!("{}\n", lines.join("\n"))
}

pub fn render_missing_facts(graph: &GraphFile, args: &MissingFactsArgs) -> String {
    let counts = edge_counts(graph);
    let mut missing: Vec<(&Node, usize)> =
        filtered_nodes(graph, &args.node_types, args.include_features)
            .into_iter()
            .filter(|node| node.properties.key_facts.is_empty())
            .map(|node| (node, counts.get(node.id.as_str()).copied().unwrap_or(0)))
            .collect();

    match args.sort {
        MissingFactsSort::Edges => {
            missing.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
        }
        MissingFactsSort::Id => {
            missing.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        }
    }

    let mut lines = vec![format!("= missing-facts ({})", missing.len())];
    for (node, edge_count) in missing.into_iter().take(args.limit) {
        lines.push(format!(
            "- {} | {} | {} | edges:{}",
            node.r#type, node.id, node.name, edge_count
        ));
    }
    format!("{}\n", lines.join("\n"))
}

pub fn render_duplicates(graph: &GraphFile, args: &DuplicatesArgs) -> String {
    let nodes = filtered_nodes(graph, &args.node_types, args.include_features);
    let mut by_type = BTreeMap::<String, Vec<&Node>>::new();
    for node in nodes {
        by_type.entry(node.r#type.clone()).or_default().push(node);
    }

    let mut candidates = Vec::new();
    for (node_type, nodes) in by_type {
        for (idx, left) in nodes.iter().enumerate() {
            let left_name = left.name.to_lowercase();
            for right in nodes.iter().skip(idx + 1) {
                let right_name = right.name.to_lowercase();
                let similarity = normalized_levenshtein(&left_name, &right_name);
                if left_name.contains(&right_name)
                    || right_name.contains(&left_name)
                    || similarity >= args.threshold
                {
                    candidates.push((
                        node_type.clone(),
                        left.id.clone(),
                        left.name.clone(),
                        right.id.clone(),
                        right.name.clone(),
                        similarity,
                    ));
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.5.total_cmp(&a.5).then_with(|| a.1.cmp(&b.1)));

    let mut lines = vec![format!("= duplicates ({})", candidates.len())];
    for (node_type, left_id, left_name, right_id, right_name, similarity) in
        candidates.into_iter().take(args.limit)
    {
        lines.push(format!(
            "- {} | {} <-> {} | {:.2} | {} <> {}",
            node_type, left_id, right_id, similarity, left_name, right_name
        ));
    }
    format!("{}\n", lines.join("\n"))
}

pub fn render_edge_gaps(graph: &GraphFile, args: &EdgeGapsArgs) -> String {
    let mut lines = vec!["= edge-gaps".to_owned()];
    let nodes = filtered_nodes(graph, &args.node_types, true);
    let relation_filter = args.relation.as_deref();

    let mut datastore_gaps = Vec::new();
    let mut process_gaps = Vec::new();

    for node in nodes {
        if node.r#type == "DataStore" {
            let has_stored_in = graph.edges.iter().any(|edge| {
                edge.target_id == node.id && edge.relation == relation_filter.unwrap_or("STORED_IN")
            });
            if !has_stored_in {
                datastore_gaps.push((node.id.clone(), node.name.clone()));
            }
        }
        if node.r#type == "Process" {
            let has_incoming = graph.edges.iter().any(|edge| {
                edge.target_id == node.id
                    && relation_filter.map(|r| r == edge.relation).unwrap_or(true)
            });
            if !has_incoming {
                process_gaps.push((node.id.clone(), node.name.clone()));
            }
        }
    }

    datastore_gaps.sort();
    process_gaps.sort();

    lines.push(format!(
        "datastore-missing-stored-in: {}",
        datastore_gaps.len()
    ));
    for (id, name) in datastore_gaps.into_iter().take(args.limit) {
        lines.push(format!("- DataStore | {} | {}", id, name));
    }
    lines.push(format!("process-missing-incoming: {}", process_gaps.len()));
    for (id, name) in process_gaps.into_iter().take(args.limit) {
        lines.push(format!("- Process | {} | {}", id, name));
    }

    format!("{}\n", lines.join("\n"))
}
