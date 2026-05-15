use std::path::Path;

use anyhow::Result;

use crate::analysis::{
    compute_quality_snapshot, render_duplicates, render_duplicates_json, render_edge_gaps,
    render_edge_gaps_json, render_missing_descriptions, render_missing_descriptions_json,
    render_missing_facts, render_missing_facts_json, render_stats,
};
use crate::cli::{
    AuditArgs, BaselineArgs, CheckArgs, FeedbackLogArgs, FeedbackSummaryArgs, KqlArgs,
    QualityCommand, StatsArgs,
};
use crate::graph::GraphFile;
use crate::kql;

pub(crate) fn execute_feedback_log(cwd: &Path, args: &FeedbackLogArgs) -> Result<String> {
    crate::render_feedback_log(cwd, args)
}

pub(crate) fn execute_stats(graph: &GraphFile, args: &StatsArgs) -> String {
    render_stats(graph, args)
}

pub(crate) fn execute_check(graph: &GraphFile, cwd: &Path, args: &CheckArgs) -> String {
    crate::render_check(graph, cwd, args)
}

pub(crate) fn execute_audit(graph: &GraphFile, cwd: &Path, args: &AuditArgs) -> String {
    crate::render_audit(graph, cwd, args)
}

pub(crate) fn execute_quality(command: QualityCommand, graph: &GraphFile) -> String {
    match command {
        QualityCommand::MissingDescriptions(args) => {
            if args.json {
                render_missing_descriptions_json(graph, &args)
            } else {
                render_missing_descriptions(graph, &args)
            }
        }
        QualityCommand::MissingFacts(args) => {
            if args.json {
                render_missing_facts_json(graph, &args)
            } else {
                render_missing_facts(graph, &args)
            }
        }
        QualityCommand::Duplicates(args) => {
            if args.json {
                render_duplicates_json(graph, &args)
            } else {
                render_duplicates(graph, &args)
            }
        }
        QualityCommand::EdgeGaps(args) => {
            if args.json {
                render_edge_gaps_json(graph, &args)
            } else {
                render_edge_gaps(graph, &args)
            }
        }
    }
}

pub(crate) fn execute_missing_descriptions(
    graph: &GraphFile,
    args: &crate::cli::MissingDescriptionsArgs,
) -> String {
    if args.json {
        render_missing_descriptions_json(graph, args)
    } else {
        render_missing_descriptions(graph, args)
    }
}

pub(crate) fn execute_missing_facts(
    graph: &GraphFile,
    args: &crate::cli::MissingFactsArgs,
) -> String {
    if args.json {
        render_missing_facts_json(graph, args)
    } else {
        render_missing_facts(graph, args)
    }
}

pub(crate) fn execute_duplicates(graph: &GraphFile, args: &crate::cli::DuplicatesArgs) -> String {
    if args.json {
        render_duplicates_json(graph, args)
    } else {
        render_duplicates(graph, args)
    }
}

pub(crate) fn execute_edge_gaps(graph: &GraphFile, args: &crate::cli::EdgeGapsArgs) -> String {
    if args.json {
        render_edge_gaps_json(graph, args)
    } else {
        render_edge_gaps(graph, args)
    }
}

pub(crate) fn execute_kql(graph: &GraphFile, args: KqlArgs) -> Result<String> {
    if args.json {
        Ok(
            serde_json::to_string_pretty(&kql::query(graph, &args.query)?)
                .unwrap_or_else(|_| "{}".to_owned()),
        )
    } else {
        kql::render_query(graph, &args.query)
    }
}

/// Execute list command - convenience wrapper around KQL
pub(crate) fn execute_list(graph: &GraphFile, args: &crate::cli::ListArgs) -> Result<String> {
    // Build KQL query from list arguments
    let mut query = String::from("node");

    // Add type filter
    if let Some(ref node_type) = args.r#type {
        if !node_type.is_empty() {
            query.push_str(&format!(" type={}", node_type));
        }
    }

    // Add since filter (created_at >= date)
    if let Some(ref since) = args.since {
        if !since.is_empty() {
            query.push_str(&format!(" created_at>={}", since));
        }
    }

    // Add sorting and limit
    query.push_str(" sort=-created_at");
    if let Some(limit) = args.limit {
        query.push_str(&format!(" limit={}", limit));
    } else {
        query.push_str(" limit=50");
    }

    // Execute via KQL
    kql::render_query(graph, &query)
}

pub(crate) fn execute_feedback_summary(
    cwd: &Path,
    graph_name: &str,
    args: &FeedbackSummaryArgs,
) -> Result<String> {
    crate::render_feedback_summary_for_graph(cwd, graph_name, args)
}

pub(crate) fn execute_baseline(
    cwd: &Path,
    graph_name: &str,
    graph: &GraphFile,
    args: &BaselineArgs,
) -> Result<String> {
    let quality = compute_quality_snapshot(graph, args.include_features, 0.85);
    crate::render_baseline_report(cwd, graph_name, graph, &quality, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_list_without_filters_returns_nodes() {
        let mut graph = GraphFile::new("test");
        graph.nodes.push(crate::graph::Node {
            id: "concept:first".to_owned(),
            r#type: "Concept".to_owned(),
            name: "First".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });
        graph.nodes.push(crate::graph::Node {
            id: "concept:second".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Second".to_owned(),
            properties: Default::default(),
            source_files: Vec::new(),
        });

        let args = crate::cli::ListArgs {
            r#type: None,
            since: None,
            limit: Some(10),
            fields: None,
        };
        let rendered = execute_list(&graph, &args).expect("list should succeed");
        assert!(rendered.contains("nodes: 2 (total: 2)"));
        assert!(rendered.contains("# concept:first | First [Concept]"));
        assert!(rendered.contains("# concept:second | Second [Concept]"));
    }
}
