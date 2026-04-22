mod access_log;
mod analysis;
mod app;
mod cache_paths;
mod cli;
mod config;
mod event_log;
mod export_html;
pub mod graph;
mod graph_lock;
mod import_csv;
mod import_markdown;
mod index;
mod init;
mod kg_sidecar;
mod kql;
mod ops;
pub mod output;
mod schema;
mod scoring;
mod storage;
mod text_norm;
mod validate;
mod vectors;

// Re-export the core graph types for embedding (e.g. kg-mcp).
pub use cache_paths::cache_root_for_cwd;
pub use graph::{Edge, EdgeProperties, GraphFile, Metadata, Node, NodeProperties, Note};
pub use graph_lock::acquire_for_graph as acquire_graph_write_lock;
pub use output::FindMode;

// Re-export validation constants for schema tools.
pub use validate::{
    EDGE_TYPE_RULES, TYPE_TO_PREFIX, VALID_RELATIONS, VALID_TYPES, canonicalize_node_id_for_type,
    edge_type_rule, format_edge_source_type_error, format_edge_target_type_error,
    is_valid_node_type, is_valid_relation, normalize_node_id,
};

// Re-export BM25 index for embedding and benchmarks.
pub use index::Bm25Index;

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use cli::{
    AsOfArgs, AuditArgs, BaselineArgs, CheckArgs, Cli, ClusterSkill, ClustersArgs, Command,
    DiffAsOfArgs, EdgeCommand, ExportDotArgs, ExportGraphmlArgs, ExportMdArgs, ExportMermaidArgs,
    FeedbackLogArgs, FeedbackSummaryArgs, FindMode as CliFindMode, GraphCommand, HistoryArgs,
    ImportCsvArgs, ImportMarkdownArgs, MergeStrategy, NodeCommand, NoteAddArgs, NoteCommand,
    NoteListArgs, ScoreAllArgs, SplitArgs, TemporalSource, TimelineArgs, VectorCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
// (graph types are re-exported above)
use storage::{GraphStore, graph_store};

use app::graph_node_edge::{GraphCommandContext, execute_edge, execute_node};
use app::graph_note::{GraphNoteContext, execute_note};
use app::graph_query_quality::{
    execute_audit, execute_baseline, execute_check, execute_duplicates, execute_edge_gaps,
    execute_feedback_log, execute_feedback_summary, execute_kql, execute_list, execute_missing_descriptions,
    execute_missing_facts, execute_quality, execute_stats,
};
use app::graph_transfer_temporal::{
    GraphTransferContext, execute_access_log, execute_access_paths, execute_access_stats, execute_as_of,
    execute_diff_as_of, execute_export_dot, execute_export_graphml, execute_export_html,
    execute_export_json, execute_export_md, execute_export_mermaid, execute_history,
    execute_import_csv, execute_import_json, execute_import_markdown, execute_split,
    execute_timeline, execute_vector,
};

use schema::{GraphSchema, SchemaViolation};
use validate::validate_graph;

static EVENT_LOG_MODE: AtomicU8 = AtomicU8::new(0);

// ---------------------------------------------------------------------------
// Schema validation helpers
// ---------------------------------------------------------------------------

fn format_schema_violations(violations: &[SchemaViolation]) -> String {
    let mut lines = Vec::new();
    lines.push("schema violations:".to_owned());
    for v in violations {
        lines.push(format!("  - {}", v.message));
    }
    lines.join("\n")
}

pub(crate) fn bail_on_schema_violations(violations: &[SchemaViolation]) -> Result<()> {
    if !violations.is_empty() {
        anyhow::bail!("{}", format_schema_violations(violations));
    }
    Ok(())
}

pub fn validate_node_add_with_schema(cwd: &Path, node: &Node) -> Result<()> {
    let schema = GraphSchema::discover(cwd)
        .with_context(|| format!("failed to discover schema from {}", cwd.display()))?
        .map(|(_, schema)| schema);
    if let Some(schema) = schema.as_ref() {
        let violations = schema.validate_node_add(node);
        bail_on_schema_violations(&violations)?;
    }
    Ok(())
}

fn validate_graph_with_schema(graph: &GraphFile, schema: &GraphSchema) -> Vec<SchemaViolation> {
    let mut all_violations = Vec::new();
    for node in &graph.nodes {
        all_violations.extend(schema.validate_node_add(node));
    }
    let node_type_map: std::collections::HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.r#type.as_str()))
        .collect();
    for edge in &graph.edges {
        if let (Some(src_type), Some(tgt_type)) = (
            node_type_map.get(edge.source_id.as_str()),
            node_type_map.get(edge.target_id.as_str()),
        ) {
            all_violations.extend(schema.validate_edge_add(
                &edge.source_id,
                src_type,
                &edge.relation,
                &edge.target_id,
                tgt_type,
            ));
        }
    }
    all_violations.extend(schema.validate_uniqueness(&graph.nodes));
    all_violations
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run kg with CLI arguments, printing the result to stdout.
///
/// This is the main entry point for the kg binary.
pub fn run<I>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = OsString>,
{
    let rendered = run_args(args, cwd)?;
    if should_colorize_stdout() {
        print!("{}", colorize_cli_output(&rendered));
    } else {
        print!("{rendered}");
    }
    Ok(())
}

fn should_colorize_stdout() -> bool {
    let force = std::env::var("CLICOLOR_FORCE")
        .map(|value| value != "0")
        .unwrap_or(false);
    if force {
        return true;
    }
    if !std::io::stdout().is_terminal() {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::env::var("CLICOLOR")
        .map(|value| value != "0")
        .unwrap_or(true)
}

fn colorize_cli_output(rendered: &str) -> String {
    if looks_like_json(rendered) {
        return rendered.to_owned();
    }
    rendered
        .lines()
        .map(colorize_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn looks_like_json(rendered: &str) -> bool {
    let trimmed = rendered.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn colorize_line(line: &str) -> String {
    const RESET: &str = "\x1b[0m";
    const BOLD_CYAN: &str = "\x1b[1;36m";
    const BOLD_YELLOW: &str = "\x1b[1;33m";
    const BOLD_GREEN: &str = "\x1b[1;32m";
    const BOLD_MAGENTA: &str = "\x1b[1;35m";
    const BLUE: &str = "\x1b[34m";

    if line.starts_with("# ") {
        return format!("{BOLD_CYAN}{line}{RESET}");
    }
    if line.starts_with("? ") {
        return format!("{BOLD_YELLOW}{line}{RESET}");
    }
    if line.starts_with("= ") || line.starts_with("+ ") {
        return format!("{BOLD_GREEN}{line}{RESET}");
    }
    if line.starts_with("score:") {
        return format!("{BOLD_MAGENTA}{line}{RESET}");
    }
    if line.starts_with("-> ") || line.starts_with("<- ") {
        return format!("{BLUE}{line}{RESET}");
    }
    line.to_owned()
}

pub fn format_error_chain(err: &anyhow::Error) -> String {
    let mut rendered = err.to_string();
    let mut causes = err.chain().skip(1).peekable();
    if causes.peek().is_some() {
        rendered.push_str("\ncaused by:");
        for cause in causes {
            let _ = write!(rendered, "\n  - {cause}");
        }
    }
    rendered
}

/// Run kg with CLI arguments, returning the rendered output as a string.
///
/// This is useful for embedding kg in other applications.
pub fn run_args<I>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = OsString>,
{
    let cli = Cli::parse_from(normalize_args(args));
    let graph_root = default_graph_root(cwd);
    execute(cli, cwd, &graph_root)
}

/// Run kg with CLI arguments, returning errors as Result instead of exiting.
///
/// Unlike `run_args`, this does not exit on parse errors - it returns them
/// as `Err` results. Useful for testing and embedding scenarios.
pub fn run_args_safe<I>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = OsString>,
{
    let cli = Cli::try_parse_from(normalize_args(args)).map_err(|err| anyhow!(err.to_string()))?;
    let graph_root = default_graph_root(cwd);
    execute(cli, cwd, &graph_root)
}

// ---------------------------------------------------------------------------
// Arg normalisation: `kg fridge ...` -> `kg graph fridge ...`
// ---------------------------------------------------------------------------

fn normalize_args<I>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = OsString>,
{
    let collected: Vec<OsString> = args.into_iter().collect();
    if collected.len() <= 1 {
        return collected;
    }
    let first = collected[1].to_string_lossy();
    if first.starts_with('-')
        || first == "init"
        || first == "create"
        || first == "diff"
        || first == "merge"
        || first == "graph"
        || first == "list"
        || first == "feedback-log"
        || first == "feedback-summary"
    {
        return collected;
    }
    let mut normalized = Vec::with_capacity(collected.len() + 1);
    normalized.push(collected[0].clone());
    normalized.push(OsString::from("graph"));
    normalized.extend(collected.into_iter().skip(1));
    normalized
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

fn execute(cli: Cli, cwd: &Path, graph_root: &Path) -> Result<String> {
    configure_event_log_mode(cli.event_log);
    match cli.command {
        Command::Init(args) => Ok(init::render_init(&args)),
        Command::Create { graph_name } => {
            let store = graph_store(cwd, graph_root, false)?;
            let path = store.create_graph(&graph_name)?;
            let graph_file = store.load_graph(&path)?;
            append_event_snapshot(&path, "graph.create", Some(graph_name.clone()), &graph_file)?;
            Ok(format!("+ created {}\n", path.display()))
        }
        Command::Diff { left, right, json } => {
            let store = graph_store(cwd, graph_root, false)?;
            if json {
                render_graph_diff_json(store.as_ref(), &left, &right)
            } else {
                render_graph_diff(store.as_ref(), &left, &right)
            }
        }
        Command::Merge {
            target,
            source,
            strategy,
        } => {
            let store = graph_store(cwd, graph_root, false)?;
            merge_graphs(store.as_ref(), &target, &source, strategy)
        }
        Command::List(args) => {
            let store = graph_store(cwd, graph_root, false)?;
            if args.json {
                render_graph_list_json(store.as_ref())
            } else {
                render_graph_list(store.as_ref(), args.full)
            }
        }
        Command::FeedbackLog(args) => execute_feedback_log(cwd, &args),
        Command::Graph {
            graph,
            legacy,
            command,
        } => {
            let store = graph_store(cwd, graph_root, legacy)?;
            let path = store.resolve_graph_path(&graph)?;
            let _graph_write_lock = if graph_command_mutates(&command) {
                Some(graph_lock::acquire_for_graph(&path)?)
            } else {
                None
            };
            let mut graph_file = store.load_graph(&path)?;
            let schema = GraphSchema::discover(cwd).ok().flatten().map(|(_, s)| s);
            let user_short_uid = config::ensure_user_short_uid(cwd);

            match command {
                GraphCommand::Node { command } => execute_node(
                    command,
                    GraphCommandContext {
                        graph_name: &graph,
                        path: &path,
                        user_short_uid: &user_short_uid,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                ),

                GraphCommand::Edge { command } => execute_edge(
                    command,
                    GraphCommandContext {
                        graph_name: &graph,
                        path: &path,
                        user_short_uid: &user_short_uid,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                ),

                GraphCommand::Note { command } => execute_note(
                    command,
                    GraphNoteContext {
                        path: &path,
                        graph_file: &mut graph_file,
                        store: store.as_ref(),
                        _schema: schema.as_ref(),
                    },
                ),

                GraphCommand::Stats(args) => Ok(execute_stats(&graph_file, &args)),
                GraphCommand::List(args) => execute_list(&graph_file, &args),
                GraphCommand::Check(args) => Ok(execute_check(&graph_file, cwd, &args)),
                GraphCommand::Audit(args) => Ok(execute_audit(&graph_file, cwd, &args)),

                GraphCommand::Quality { command } => Ok(execute_quality(command, &graph_file)),

                // Short aliases (e.g. `kg graph fridge missing-descriptions`)
                GraphCommand::MissingDescriptions(args) => {
                    Ok(execute_missing_descriptions(&graph_file, &args))
                }
                GraphCommand::MissingFacts(args) => Ok(execute_missing_facts(&graph_file, &args)),
                GraphCommand::Duplicates(args) => Ok(execute_duplicates(&graph_file, &args)),
                GraphCommand::EdgeGaps(args) => Ok(execute_edge_gaps(&graph_file, &args)),
                GraphCommand::Clusters(args) => execute_clusters(&graph_file, &path, &args),

                GraphCommand::ExportHtml(args) => execute_export_html(&graph, &graph_file, args),

                GraphCommand::AccessLog(args) => execute_access_log(&path, args),

                GraphCommand::AccessStats(_) => execute_access_stats(&path),
                GraphCommand::AccessPaths(args) => execute_access_paths(&path, args),
                GraphCommand::ImportCsv(args) => execute_import_csv(
                    GraphTransferContext {
                        cwd,
                        graph_name: &graph,
                        path: &path,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                    args,
                ),
                GraphCommand::ImportMarkdown(args) => execute_import_markdown(
                    GraphTransferContext {
                        cwd,
                        graph_name: &graph,
                        path: &path,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                    args,
                ),
                GraphCommand::Kql(args) => execute_kql(&graph_file, args),
                GraphCommand::ExportJson(args) => execute_export_json(&graph, &graph_file, args),
                GraphCommand::ImportJson(args) => {
                    execute_import_json(&path, &graph, store.as_ref(), args)
                }
                GraphCommand::ExportDot(args) => execute_export_dot(&graph, &graph_file, args),
                GraphCommand::ExportMermaid(args) => {
                    execute_export_mermaid(&graph, &graph_file, args)
                }
                GraphCommand::ExportGraphml(args) => {
                    execute_export_graphml(&graph, &graph_file, args)
                }
                GraphCommand::ExportMd(args) => execute_export_md(
                    GraphTransferContext {
                        cwd,
                        graph_name: &graph,
                        path: &path,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                    args,
                ),
                GraphCommand::Split(args) => execute_split(&graph, &graph_file, args),
                GraphCommand::Vector { command } => execute_vector(
                    GraphTransferContext {
                        cwd,
                        graph_name: &graph,
                        path: &path,
                        graph_file: &mut graph_file,
                        schema: schema.as_ref(),
                        store: store.as_ref(),
                    },
                    command,
                ),
                GraphCommand::AsOf(args) => execute_as_of(&path, &graph, args),
                GraphCommand::History(args) => execute_history(&path, &graph, args),
                GraphCommand::Timeline(args) => execute_timeline(&path, &graph, args),
                GraphCommand::DiffAsOf(args) => execute_diff_as_of(&path, &graph, args),
                GraphCommand::FeedbackSummary(args) => {
                    Ok(execute_feedback_summary(cwd, &graph, &args)?)
                }
                GraphCommand::Baseline(args) => {
                    Ok(execute_baseline(cwd, &graph, &graph_file, &args)?)
                }
                GraphCommand::ScoreAll(args) => execute_score_all(&graph_file, &path, &args),
            }
        }
    }
}

fn render_graph_list(store: &dyn GraphStore, full: bool) -> Result<String> {
    let graphs = store.list_graphs()?;

    let mut lines = vec![format!("= graphs ({})", graphs.len())];
    for (name, path) in graphs {
        if full {
            lines.push(format!("- {name} | {}", path.display()));
        } else {
            lines.push(format!("- {name}"));
        }
    }
    Ok(format!("{}\n", lines.join("\n")))
}

fn graph_command_mutates(command: &GraphCommand) -> bool {
    match command {
        GraphCommand::Node { command } => node_command_mutates(command),
        GraphCommand::Edge { command } => edge_command_mutates(command),
        GraphCommand::Note { command } => note_command_mutates(command),
        GraphCommand::ImportCsv(_)
        | GraphCommand::ImportMarkdown(_)
        | GraphCommand::ImportJson(_)
        | GraphCommand::Vector {
            command: VectorCommand::Import(_),
        } => true,
        GraphCommand::Stats(_)
        | GraphCommand::Check(_)
        | GraphCommand::Audit(_)
        | GraphCommand::Quality { .. }
        | GraphCommand::MissingDescriptions(_)
        | GraphCommand::MissingFacts(_)
        | GraphCommand::Duplicates(_)
        | GraphCommand::EdgeGaps(_)
        | GraphCommand::Clusters(_)
        | GraphCommand::ExportHtml(_)
        | GraphCommand::AccessLog(_)
        | GraphCommand::AccessStats(_)
        | GraphCommand::List(_)
        | GraphCommand::AccessPaths(_)
        | GraphCommand::Kql(_)
        | GraphCommand::ExportJson(_)
        | GraphCommand::ExportDot(_)
        | GraphCommand::ExportMermaid(_)
        | GraphCommand::ExportGraphml(_)
        | GraphCommand::ExportMd(_)
        | GraphCommand::Split(_)
        | GraphCommand::Vector {
            command: VectorCommand::Stats(_),
        }
        | GraphCommand::AsOf(_)
        | GraphCommand::History(_)
        | GraphCommand::Timeline(_)
        | GraphCommand::DiffAsOf(_)
        | GraphCommand::FeedbackSummary(_)
        | GraphCommand::Baseline(_)
        | GraphCommand::ScoreAll(_) => false,
    }
}

fn execute_score_all(graph: &GraphFile, path: &Path, args: &ScoreAllArgs) -> Result<String> {
    let outcome = scoring::compute_all_pair_scores_to_cache(
        graph,
        path,
        &scoring::ScoreAllConfig {
            min_desc_len: args.min_desc_len,
            desc_weight: args.desc_weight,
            bundle_weight: args.bundle_weight,
            cluster_seed: args.cluster_seed,
            cluster_resolution: args.cluster_resolution,
            membership_top_k: args.membership_top_k,
        },
    )?;

    Ok(format!(
        "= score-all\n- pairs: {}\n- edges: {}\n- clusters: {}\n- output: {}\n",
        outcome.pairs,
        outcome.edges,
        outcome.clusters,
        outcome.path.display()
    ))
}

fn execute_clusters(graph: &GraphFile, path: &Path, args: &ClustersArgs) -> Result<String> {
    let source_graph = resolve_cluster_source_graph(graph, path)?;
    Ok(render_clusters(&source_graph, args))
}

fn resolve_cluster_source_graph(graph: &GraphFile, path: &Path) -> Result<GraphFile> {
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

fn find_latest_score_snapshot(path: &Path) -> Result<Option<PathBuf>> {
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

#[derive(Debug, Serialize)]
struct ClusterView {
    id: String,
    size: usize,
    relevance: f64,
    members: Vec<(String, f64)>,
}

fn render_clusters(graph: &GraphFile, args: &ClustersArgs) -> String {
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

fn node_command_mutates(command: &NodeCommand) -> bool {
    matches!(
        command,
        NodeCommand::Add(_)
            | NodeCommand::Modify(_)
            | NodeCommand::Rename { .. }
            | NodeCommand::Remove { .. }
    )
}

fn edge_command_mutates(command: &EdgeCommand) -> bool {
    matches!(
        command,
        EdgeCommand::Add(_) | EdgeCommand::AddBatch(_) | EdgeCommand::Remove(_)
    )
}

fn note_command_mutates(command: &NoteCommand) -> bool {
    matches!(command, NoteCommand::Add(_) | NoteCommand::Remove { .. })
}

#[derive(Debug, Serialize)]
struct GraphListEntry {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct GraphListResponse {
    graphs: Vec<GraphListEntry>,
}

fn render_graph_list_json(store: &dyn GraphStore) -> Result<String> {
    let graphs = store.list_graphs()?;
    let entries = graphs
        .into_iter()
        .map(|(name, path)| GraphListEntry {
            name,
            path: path.display().to_string(),
        })
        .collect();
    let payload = GraphListResponse { graphs: entries };
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()))
}

#[derive(Debug, Serialize)]
struct FindQueryResult {
    query: String,
    count: usize,
    nodes: Vec<ScoredFindNode>,
}

#[derive(Debug, Serialize)]
struct ScoredFindNode {
    score: i64,
    node: Node,
    #[serde(skip_serializing_if = "Option::is_none")]
    score_breakdown: Option<ScoredFindBreakdown>,
}

#[derive(Debug, Serialize)]
struct ScoredFindBreakdown {
    raw_relevance: f64,
    normalized_relevance: i64,
    lexical_boost: i64,
    feedback_boost: i64,
    importance_boost: i64,
    authority_raw: i64,
    authority_applied: i64,
    authority_cap: i64,
}

#[derive(Debug, Serialize)]
struct FindResponse {
    total: usize,
    queries: Vec<FindQueryResult>,
}

pub(crate) fn render_find_json_with_index(
    graph: &GraphFile,
    queries: &[String],
    limit: usize,
    include_metadata: bool,
    mode: output::FindMode,
    debug_score: bool,
    index: Option<&Bm25Index>,
    tune: Option<&output::FindTune>,
) -> String {
    let mut total = 0usize;
    let mut results = Vec::new();
    for query in queries {
        let (count, scored_nodes) = output::find_scored_nodes_and_total_with_index_tuned(
            graph,
            query,
            limit,
            true,
            include_metadata,
            mode,
            index,
            tune,
        );
        total += count;
        let nodes = scored_nodes
            .into_iter()
            .map(|entry| ScoredFindNode {
                score: entry.score,
                node: entry.node,
                score_breakdown: debug_score.then_some(ScoredFindBreakdown {
                    raw_relevance: entry.breakdown.raw_relevance,
                    normalized_relevance: entry.breakdown.normalized_relevance,
                    lexical_boost: entry.breakdown.lexical_boost,
                    feedback_boost: entry.breakdown.feedback_boost,
                    importance_boost: entry.breakdown.importance_boost,
                    authority_raw: entry.breakdown.authority_raw,
                    authority_applied: entry.breakdown.authority_applied,
                    authority_cap: entry.breakdown.authority_cap,
                }),
            })
            .collect();
        results.push(FindQueryResult {
            query: query.clone(),
            count,
            nodes,
        });
    }
    let payload = FindResponse {
        total,
        queries: results,
    };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

#[derive(Debug, Serialize)]
struct NodeGetResponse {
    node: Node,
}

pub(crate) fn render_node_json(node: &Node) -> String {
    let payload = NodeGetResponse { node: node.clone() };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

fn render_graph_diff(store: &dyn GraphStore, left: &str, right: &str) -> Result<String> {
    let left_path = store.resolve_graph_path(left)?;
    let right_path = store.resolve_graph_path(right)?;
    let left_graph = store.load_graph(&left_path)?;
    let right_graph = store.load_graph(&right_path)?;
    Ok(render_graph_diff_from_files(
        left,
        right,
        &left_graph,
        &right_graph,
    ))
}

fn render_graph_diff_json(store: &dyn GraphStore, left: &str, right: &str) -> Result<String> {
    let left_path = store.resolve_graph_path(left)?;
    let right_path = store.resolve_graph_path(right)?;
    let left_graph = store.load_graph(&left_path)?;
    let right_graph = store.load_graph(&right_path)?;
    Ok(render_graph_diff_json_from_files(
        left,
        right,
        &left_graph,
        &right_graph,
    ))
}

#[derive(Debug, Serialize)]
struct DiffEntry {
    path: String,
    left: Value,
    right: Value,
}

#[derive(Debug, Serialize)]
struct EntityDiff {
    id: String,
    diffs: Vec<DiffEntry>,
}

#[derive(Debug, Serialize)]
struct GraphDiffResponse {
    left: String,
    right: String,
    added_nodes: Vec<String>,
    removed_nodes: Vec<String>,
    changed_nodes: Vec<EntityDiff>,
    added_edges: Vec<String>,
    removed_edges: Vec<String>,
    changed_edges: Vec<EntityDiff>,
    added_notes: Vec<String>,
    removed_notes: Vec<String>,
    changed_notes: Vec<EntityDiff>,
}

fn render_graph_diff_json_from_files(
    left: &str,
    right: &str,
    left_graph: &GraphFile,
    right_graph: &GraphFile,
) -> String {
    use std::collections::{HashMap, HashSet};

    let left_nodes: HashSet<String> = left_graph.nodes.iter().map(|n| n.id.clone()).collect();
    let right_nodes: HashSet<String> = right_graph.nodes.iter().map(|n| n.id.clone()).collect();

    let left_node_map: HashMap<String, &Node> =
        left_graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_node_map: HashMap<String, &Node> = right_graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let left_edges: HashSet<String> = left_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();
    let right_edges: HashSet<String> = right_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();

    let left_edge_map: HashMap<String, &Edge> = left_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();
    let right_edge_map: HashMap<String, &Edge> = right_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();

    let left_notes: HashSet<String> = left_graph.notes.iter().map(|n| n.id.clone()).collect();
    let right_notes: HashSet<String> = right_graph.notes.iter().map(|n| n.id.clone()).collect();

    let left_note_map: HashMap<String, &Note> =
        left_graph.notes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_note_map: HashMap<String, &Note> = right_graph
        .notes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let mut added_nodes: Vec<String> = right_nodes.difference(&left_nodes).cloned().collect();
    let mut removed_nodes: Vec<String> = left_nodes.difference(&right_nodes).cloned().collect();
    let mut added_edges: Vec<String> = right_edges.difference(&left_edges).cloned().collect();
    let mut removed_edges: Vec<String> = left_edges.difference(&right_edges).cloned().collect();
    let mut added_notes: Vec<String> = right_notes.difference(&left_notes).cloned().collect();
    let mut removed_notes: Vec<String> = left_notes.difference(&right_notes).cloned().collect();

    let mut changed_nodes: Vec<String> = left_nodes
        .intersection(&right_nodes)
        .filter_map(|id| {
            let left_node = left_node_map.get(id.as_str())?;
            let right_node = right_node_map.get(id.as_str())?;
            if eq_serialized(*left_node, *right_node) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();
    let mut changed_edges: Vec<String> = left_edges
        .intersection(&right_edges)
        .filter_map(|key| {
            let left_edge = left_edge_map.get(key.as_str())?;
            let right_edge = right_edge_map.get(key.as_str())?;
            if eq_serialized(*left_edge, *right_edge) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect();
    let mut changed_notes: Vec<String> = left_notes
        .intersection(&right_notes)
        .filter_map(|id| {
            let left_note = left_note_map.get(id.as_str())?;
            let right_note = right_note_map.get(id.as_str())?;
            if eq_serialized(*left_note, *right_note) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    added_nodes.sort();
    removed_nodes.sort();
    added_edges.sort();
    removed_edges.sort();
    added_notes.sort();
    removed_notes.sort();
    changed_nodes.sort();
    changed_edges.sort();
    changed_notes.sort();

    let changed_nodes = changed_nodes
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_node_map
                .get(id.as_str())
                .zip(right_node_map.get(id.as_str()))
                .map(|(left_node, right_node)| diff_serialized_values_json(*left_node, *right_node))
                .unwrap_or_default(),
            id,
        })
        .collect();
    let changed_edges = changed_edges
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_edge_map
                .get(id.as_str())
                .zip(right_edge_map.get(id.as_str()))
                .map(|(left_edge, right_edge)| diff_serialized_values_json(*left_edge, *right_edge))
                .unwrap_or_default(),
            id,
        })
        .collect();
    let changed_notes = changed_notes
        .into_iter()
        .map(|id| EntityDiff {
            diffs: left_note_map
                .get(id.as_str())
                .zip(right_note_map.get(id.as_str()))
                .map(|(left_note, right_note)| diff_serialized_values_json(*left_note, *right_note))
                .unwrap_or_default(),
            id,
        })
        .collect();

    let payload = GraphDiffResponse {
        left: left.to_owned(),
        right: right.to_owned(),
        added_nodes,
        removed_nodes,
        changed_nodes,
        added_edges,
        removed_edges,
        changed_edges,
        added_notes,
        removed_notes,
        changed_notes,
    };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

fn render_graph_diff_from_files(
    left: &str,
    right: &str,
    left_graph: &GraphFile,
    right_graph: &GraphFile,
) -> String {
    use std::collections::{HashMap, HashSet};

    let left_nodes: HashSet<String> = left_graph.nodes.iter().map(|n| n.id.clone()).collect();
    let right_nodes: HashSet<String> = right_graph.nodes.iter().map(|n| n.id.clone()).collect();

    let left_node_map: HashMap<String, &Node> =
        left_graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_node_map: HashMap<String, &Node> = right_graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let left_edges: HashSet<String> = left_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();
    let right_edges: HashSet<String> = right_graph
        .edges
        .iter()
        .map(|e| format!("{} {} {}", e.source_id, e.relation, e.target_id))
        .collect();

    let left_edge_map: HashMap<String, &Edge> = left_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();
    let right_edge_map: HashMap<String, &Edge> = right_graph
        .edges
        .iter()
        .map(|e| (format!("{} {} {}", e.source_id, e.relation, e.target_id), e))
        .collect();

    let left_notes: HashSet<String> = left_graph.notes.iter().map(|n| n.id.clone()).collect();
    let right_notes: HashSet<String> = right_graph.notes.iter().map(|n| n.id.clone()).collect();

    let left_note_map: HashMap<String, &Note> =
        left_graph.notes.iter().map(|n| (n.id.clone(), n)).collect();
    let right_note_map: HashMap<String, &Note> = right_graph
        .notes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    let mut added_nodes: Vec<String> = right_nodes.difference(&left_nodes).cloned().collect();
    let mut removed_nodes: Vec<String> = left_nodes.difference(&right_nodes).cloned().collect();
    let mut added_edges: Vec<String> = right_edges.difference(&left_edges).cloned().collect();
    let mut removed_edges: Vec<String> = left_edges.difference(&right_edges).cloned().collect();
    let mut added_notes: Vec<String> = right_notes.difference(&left_notes).cloned().collect();
    let mut removed_notes: Vec<String> = left_notes.difference(&right_notes).cloned().collect();

    let mut changed_nodes: Vec<String> = left_nodes
        .intersection(&right_nodes)
        .filter_map(|id| {
            let left_node = left_node_map.get(id.as_str())?;
            let right_node = right_node_map.get(id.as_str())?;
            if eq_serialized(*left_node, *right_node) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    let mut changed_edges: Vec<String> = left_edges
        .intersection(&right_edges)
        .filter_map(|key| {
            let left_edge = left_edge_map.get(key.as_str())?;
            let right_edge = right_edge_map.get(key.as_str())?;
            if eq_serialized(*left_edge, *right_edge) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect();

    let mut changed_notes: Vec<String> = left_notes
        .intersection(&right_notes)
        .filter_map(|id| {
            let left_note = left_note_map.get(id.as_str())?;
            let right_note = right_note_map.get(id.as_str())?;
            if eq_serialized(*left_note, *right_note) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();

    added_nodes.sort();
    removed_nodes.sort();
    added_edges.sort();
    removed_edges.sort();
    added_notes.sort();
    removed_notes.sort();
    changed_nodes.sort();
    changed_edges.sort();
    changed_notes.sort();

    let mut lines = vec![format!("= diff {left} -> {right}")];
    lines.push(format!("+ nodes ({})", added_nodes.len()));
    for id in added_nodes {
        lines.push(format!("+ node {id}"));
    }
    lines.push(format!("- nodes ({})", removed_nodes.len()));
    for id in removed_nodes {
        lines.push(format!("- node {id}"));
    }
    lines.push(format!("~ nodes ({})", changed_nodes.len()));
    for id in changed_nodes {
        if let (Some(left_node), Some(right_node)) = (
            left_node_map.get(id.as_str()),
            right_node_map.get(id.as_str()),
        ) {
            lines.extend(render_entity_diff_lines("node", &id, left_node, right_node));
        } else {
            lines.push(format!("~ node {id}"));
        }
    }
    lines.push(format!("+ edges ({})", added_edges.len()));
    for edge in added_edges {
        lines.push(format!("+ edge {edge}"));
    }
    lines.push(format!("- edges ({})", removed_edges.len()));
    for edge in removed_edges {
        lines.push(format!("- edge {edge}"));
    }
    lines.push(format!("~ edges ({})", changed_edges.len()));
    for edge in changed_edges {
        if let (Some(left_edge), Some(right_edge)) = (
            left_edge_map.get(edge.as_str()),
            right_edge_map.get(edge.as_str()),
        ) {
            lines.extend(render_entity_diff_lines(
                "edge", &edge, left_edge, right_edge,
            ));
        } else {
            lines.push(format!("~ edge {edge}"));
        }
    }
    lines.push(format!("+ notes ({})", added_notes.len()));
    for note_id in added_notes {
        lines.push(format!("+ note {note_id}"));
    }
    lines.push(format!("- notes ({})", removed_notes.len()));
    for note_id in removed_notes {
        lines.push(format!("- note {note_id}"));
    }
    lines.push(format!("~ notes ({})", changed_notes.len()));
    for note_id in changed_notes {
        if let (Some(left_note), Some(right_note)) = (
            left_note_map.get(note_id.as_str()),
            right_note_map.get(note_id.as_str()),
        ) {
            lines.extend(render_entity_diff_lines(
                "note", &note_id, left_note, right_note,
            ));
        } else {
            lines.push(format!("~ note {note_id}"));
        }
    }

    format!("{}\n", lines.join("\n"))
}

fn eq_serialized<T: Serialize>(left: &T, right: &T) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => left_value == right_value,
        _ => false,
    }
}

fn render_entity_diff_lines<T: Serialize>(
    kind: &str,
    id: &str,
    left: &T,
    right: &T,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("~ {kind} {id}"));
    for diff in diff_serialized_values(left, right) {
        lines.push(format!("  ~ {diff}"));
    }
    lines
}

fn diff_serialized_values<T: Serialize>(left: &T, right: &T) -> Vec<String> {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => {
            let mut diffs = Vec::new();
            collect_value_diffs("", &left_value, &right_value, &mut diffs);
            diffs
        }
        _ => vec!["<serialization failed>".to_owned()],
    }
}

fn diff_serialized_values_json<T: Serialize>(left: &T, right: &T) -> Vec<DiffEntry> {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left_value), Ok(right_value)) => {
            let mut diffs = Vec::new();
            collect_value_diffs_json("", &left_value, &right_value, &mut diffs);
            diffs
        }
        _ => Vec::new(),
    }
}

fn collect_value_diffs_json(path: &str, left: &Value, right: &Value, out: &mut Vec<DiffEntry>) {
    if left == right {
        return;
    }
    match (left, right) {
        (Value::Object(left_obj), Value::Object(right_obj)) => {
            use std::collections::BTreeSet;

            let mut keys: BTreeSet<&str> = BTreeSet::new();
            for key in left_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in right_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in keys {
                let left_value = left_obj.get(key).unwrap_or(&Value::Null);
                let right_value = right_obj.get(key).unwrap_or(&Value::Null);
                let next_path = if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                collect_value_diffs_json(&next_path, left_value, right_value, out);
            }
        }
        (Value::Array(_), Value::Array(_)) => {
            let label = if path.is_empty() {
                "<root>[]".to_owned()
            } else {
                format!("{path}[]")
            };
            out.push(DiffEntry {
                path: label,
                left: left.clone(),
                right: right.clone(),
            });
        }
        _ => {
            let label = if path.is_empty() { "<root>" } else { path };
            out.push(DiffEntry {
                path: label.to_owned(),
                left: left.clone(),
                right: right.clone(),
            });
        }
    }
}

fn collect_value_diffs(path: &str, left: &Value, right: &Value, out: &mut Vec<String>) {
    if left == right {
        return;
    }
    match (left, right) {
        (Value::Object(left_obj), Value::Object(right_obj)) => {
            use std::collections::BTreeSet;

            let mut keys: BTreeSet<&str> = BTreeSet::new();
            for key in left_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in right_obj.keys() {
                keys.insert(key.as_str());
            }
            for key in keys {
                let left_value = left_obj.get(key).unwrap_or(&Value::Null);
                let right_value = right_obj.get(key).unwrap_or(&Value::Null);
                let next_path = if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                collect_value_diffs(&next_path, left_value, right_value, out);
            }
        }
        (Value::Array(_), Value::Array(_)) => {
            let label = if path.is_empty() {
                "<root>[]".to_owned()
            } else {
                format!("{path}[]")
            };
            out.push(format!(
                "{label}: {} -> {}",
                format_value(left),
                format_value(right)
            ));
        }
        _ => {
            let label = if path.is_empty() { "<root>" } else { path };
            out.push(format!(
                "{label}: {} -> {}",
                format_value(left),
                format_value(right)
            ));
        }
    }
}

fn format_value(value: &Value) -> String {
    let mut rendered =
        serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_owned());
    rendered = rendered.replace('\n', "\\n");
    truncate_value(rendered, 160)
}

fn truncate_value(mut value: String, limit: usize) -> String {
    if value.len() <= limit {
        return value;
    }
    value.truncate(limit.saturating_sub(3));
    value.push_str("...");
    value
}

fn merge_graphs(
    store: &dyn GraphStore,
    target: &str,
    source: &str,
    strategy: MergeStrategy,
) -> Result<String> {
    use std::collections::HashMap;

    let target_path = store.resolve_graph_path(target)?;
    let _target_write_lock = graph_lock::acquire_for_graph(&target_path)?;
    let source_path = store.resolve_graph_path(source)?;
    let mut target_graph = store.load_graph(&target_path)?;
    let source_graph = store.load_graph(&source_path)?;

    let mut node_index: HashMap<String, usize> = HashMap::new();
    for (idx, node) in target_graph.nodes.iter().enumerate() {
        node_index.insert(node.id.clone(), idx);
    }

    let mut node_added = 0usize;
    let mut node_updated = 0usize;
    for node in &source_graph.nodes {
        if let Some(&idx) = node_index.get(&node.id) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.nodes[idx] = node.clone();
                node_updated += 1;
            }
        } else {
            target_graph.nodes.push(node.clone());
            node_index.insert(node.id.clone(), target_graph.nodes.len() - 1);
            node_added += 1;
        }
    }

    let mut edge_index: HashMap<String, usize> = HashMap::new();
    for (idx, edge) in target_graph.edges.iter().enumerate() {
        let key = format!("{} {} {}", edge.source_id, edge.relation, edge.target_id);
        edge_index.insert(key, idx);
    }

    let mut edge_added = 0usize;
    let mut edge_updated = 0usize;
    for edge in &source_graph.edges {
        let key = format!("{} {} {}", edge.source_id, edge.relation, edge.target_id);
        if let Some(&idx) = edge_index.get(&key) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.edges[idx] = edge.clone();
                edge_updated += 1;
            }
        } else {
            target_graph.edges.push(edge.clone());
            edge_index.insert(key, target_graph.edges.len() - 1);
            edge_added += 1;
        }
    }

    let mut note_index: HashMap<String, usize> = HashMap::new();
    for (idx, note) in target_graph.notes.iter().enumerate() {
        note_index.insert(note.id.clone(), idx);
    }

    let mut note_added = 0usize;
    let mut note_updated = 0usize;
    for note in &source_graph.notes {
        if let Some(&idx) = note_index.get(&note.id) {
            if matches!(strategy, MergeStrategy::PreferNew) {
                target_graph.notes[idx] = note.clone();
                note_updated += 1;
            }
        } else {
            target_graph.notes.push(note.clone());
            note_index.insert(note.id.clone(), target_graph.notes.len() - 1);
            note_added += 1;
        }
    }

    store.save_graph(&target_path, &target_graph)?;
    append_event_snapshot(
        &target_path,
        "graph.merge",
        Some(format!("{source} -> {target} ({strategy:?})")),
        &target_graph,
    )?;

    let mut lines = vec![format!("+ merged {source} -> {target}")];
    lines.push(format!("nodes: +{node_added} ~{node_updated}"));
    lines.push(format!("edges: +{edge_added} ~{edge_updated}"));
    lines.push(format!("notes: +{note_added} ~{note_updated}"));

    Ok(format!("{}\n", lines.join("\n")))
}

pub(crate) fn export_graph_as_of(path: &Path, graph: &str, args: &AsOfArgs) -> Result<String> {
    match resolve_temporal_source(path, args.source)? {
        TemporalSource::EventLog => export_graph_as_of_event_log(path, graph, args),
        _ => export_graph_as_of_backups(path, graph, args),
    }
}

fn export_graph_as_of_backups(path: &Path, graph: &str, args: &AsOfArgs) -> Result<String> {
    let backups = list_graph_backups(path)?;
    if backups.is_empty() {
        bail!("no backups found for graph: {graph}");
    }
    let target_ts = args.ts_ms / 1000;
    let mut selected = None;
    for (ts, backup_path) in backups {
        if ts <= target_ts {
            selected = Some((ts, backup_path));
        }
    }
    let Some((ts, backup_path)) = selected else {
        bail!("no backup at or before ts_ms={}", args.ts_ms);
    };

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{graph}.asof.{}.json", args.ts_ms));
    let raw = read_gz_to_string(&backup_path)?;
    std::fs::write(&output_path, raw)?;
    Ok(format!("+ exported {output_path} (as-of {ts})\n"))
}

fn export_graph_as_of_event_log(path: &Path, graph: &str, args: &AsOfArgs) -> Result<String> {
    let entries = event_log::read_log(path)?;
    if entries.is_empty() {
        bail!("no event log entries found for graph: {graph}");
    }
    let selected = select_event_at_or_before(&entries, args.ts_ms)
        .ok_or_else(|| anyhow!("no event log entry at or before ts_ms={}", args.ts_ms))?;
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{graph}.asof.{}.json", args.ts_ms));
    let mut snapshot = selected.graph.clone();
    snapshot.refresh_counts();
    let raw = serde_json::to_string_pretty(&snapshot).context("failed to serialize graph")?;
    std::fs::write(&output_path, raw)?;
    Ok(format!(
        "+ exported {output_path} (as-of {})\n",
        selected.ts_ms
    ))
}

fn list_graph_backups(path: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid graph filename"))?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
    let prefixes = [format!("{stem}.{ext}.bck."), format!("{stem}.bck.")];
    let suffix = ".gz";

    let mut backups = Vec::new();
    let mut dirs = vec![cache_paths::cache_root_for_graph(path)];
    if let Some(parent) = path.parent() {
        dirs.push(parent.to_path_buf());
    }

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(suffix) {
                continue;
            }
            for prefix in &prefixes {
                if !name.starts_with(prefix) {
                    continue;
                }
                let ts_part = &name[prefix.len()..name.len() - suffix.len()];
                if let Ok(ts) = ts_part.parse::<u64>() {
                    backups.push((ts, entry.path()));
                }
            }
        }
    }
    backups.sort_by_key(|(ts, _)| *ts);
    Ok(backups)
}

fn read_gz_to_string(path: &Path) -> Result<String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let data = std::fs::read(path)?;
    let mut decoder = GzDecoder::new(&data[..]);
    let mut out = String::new();
    decoder.read_to_string(&mut out)?;
    Ok(out)
}

pub(crate) fn append_event_snapshot(
    path: &Path,
    action: &str,
    detail: Option<String>,
    graph: &GraphFile,
) -> Result<()> {
    if !event_log_enabled() {
        return Ok(());
    }
    event_log::append_snapshot(path, action, detail, graph)
}

fn configure_event_log_mode(cli_switch_enabled: bool) {
    if cli_switch_enabled {
        EVENT_LOG_MODE.store(2, Ordering::Relaxed);
        return;
    }
    EVENT_LOG_MODE.store(0, Ordering::Relaxed);
}

fn event_log_enabled() -> bool {
    match EVENT_LOG_MODE.load(Ordering::Relaxed) {
        2 => true,
        1 => false,
        _ => {
            let raw = std::env::var("KG_EVENT_LOG").unwrap_or_default();
            matches!(raw.as_str(), "1" | "true" | "TRUE" | "yes" | "on")
        }
    }
}

pub(crate) fn export_graph_json(
    graph: &str,
    graph_file: &GraphFile,
    output: Option<&str>,
) -> Result<String> {
    let output_path = output
        .map(|value| value.to_owned())
        .unwrap_or_else(|| format!("{graph}.export.json"));
    let raw = serde_json::to_string_pretty(graph_file).context("failed to serialize graph")?;
    std::fs::write(&output_path, raw)?;
    Ok(format!("+ exported {output_path}\n"))
}

pub(crate) fn import_graph_json(
    path: &Path,
    graph: &str,
    input: &str,
    store: &dyn GraphStore,
) -> Result<String> {
    let raw = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read import file: {input}"))?;
    let mut imported: GraphFile =
        serde_json::from_str(&raw).with_context(|| format!("invalid JSON: {input}"))?;
    imported.metadata.name = graph.to_owned();
    imported.refresh_counts();
    store.save_graph(path, &imported)?;
    append_event_snapshot(path, "graph.import", Some(input.to_owned()), &imported)?;
    Ok(format!("+ imported {input} -> {graph}\n"))
}

pub(crate) fn import_graph_csv(
    path: &Path,
    graph: &str,
    graph_file: &mut GraphFile,
    store: &dyn GraphStore,
    args: &ImportCsvArgs,
    schema: Option<&GraphSchema>,
) -> Result<String> {
    if args.nodes.is_none() && args.edges.is_none() && args.notes.is_none() {
        bail!("expected at least one of --nodes/--edges/--notes");
    }
    let strategy = match args.strategy {
        MergeStrategy::PreferNew => import_csv::CsvStrategy::PreferNew,
        MergeStrategy::PreferOld => import_csv::CsvStrategy::PreferOld,
    };
    let summary = import_csv::import_csv_into_graph(
        graph_file,
        import_csv::CsvImportArgs {
            nodes_path: args.nodes.as_deref(),
            edges_path: args.edges.as_deref(),
            notes_path: args.notes.as_deref(),
            strategy,
        },
    )?;
    if let Some(schema) = schema {
        let all_violations = validate_graph_with_schema(graph_file, schema);
        bail_on_schema_violations(&all_violations)?;
    }
    store.save_graph(path, graph_file)?;
    append_event_snapshot(path, "graph.import-csv", None, graph_file)?;
    let mut lines = vec![format!("+ imported csv into {graph}")];
    lines.extend(import_csv::merge_summary_lines(&summary));
    Ok(format!("{}\n", lines.join("\n")))
}

pub(crate) fn import_graph_markdown(
    path: &Path,
    graph: &str,
    graph_file: &mut GraphFile,
    store: &dyn GraphStore,
    args: &ImportMarkdownArgs,
    schema: Option<&GraphSchema>,
) -> Result<String> {
    let strategy = match args.strategy {
        MergeStrategy::PreferNew => import_markdown::MarkdownStrategy::PreferNew,
        MergeStrategy::PreferOld => import_markdown::MarkdownStrategy::PreferOld,
    };
    let summary = import_markdown::import_markdown_into_graph(
        graph_file,
        import_markdown::MarkdownImportArgs {
            path: &args.path,
            notes_as_nodes: args.notes_as_nodes,
            strategy,
        },
    )?;
    if let Some(schema) = schema {
        let all_violations = validate_graph_with_schema(graph_file, schema);
        bail_on_schema_violations(&all_violations)?;
    }
    store.save_graph(path, graph_file)?;
    append_event_snapshot(path, "graph.import-md", Some(args.path.clone()), graph_file)?;
    let mut lines = vec![format!("+ imported markdown into {graph}")];
    lines.extend(import_csv::merge_summary_lines(&summary));
    Ok(format!("{}\n", lines.join("\n")))
}

pub(crate) fn export_graph_dot(
    graph: &str,
    graph_file: &GraphFile,
    args: &ExportDotArgs,
) -> Result<String> {
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{graph}.dot"));
    let (nodes, edges) = select_subgraph(
        graph_file,
        args.focus.as_deref(),
        args.depth,
        &args.node_types,
    )?;
    let mut lines = Vec::new();
    lines.push("digraph kg {".to_owned());
    for node in &nodes {
        let label = format!("{}\\n{}", node.id, node.name);
        lines.push(format!(
            "  \"{}\" [label=\"{}\"];",
            escape_dot(&node.id),
            escape_dot(&label)
        ));
    }
    for edge in &edges {
        lines.push(format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];",
            escape_dot(&edge.source_id),
            escape_dot(&edge.target_id),
            escape_dot(&edge.relation)
        ));
    }
    lines.push("}".to_owned());
    std::fs::write(&output_path, format!("{}\n", lines.join("\n")))?;
    Ok(format!("+ exported {output_path}\n"))
}

pub(crate) fn export_graph_mermaid(
    graph: &str,
    graph_file: &GraphFile,
    args: &ExportMermaidArgs,
) -> Result<String> {
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{graph}.mmd"));
    let (nodes, edges) = select_subgraph(
        graph_file,
        args.focus.as_deref(),
        args.depth,
        &args.node_types,
    )?;
    let mut lines = Vec::new();
    lines.push("graph TD".to_owned());
    for node in &nodes {
        let label = format!("{}\\n{}", node.id, node.name);
        lines.push(format!(
            "  {}[\"{}\"]",
            sanitize_mermaid_id(&node.id),
            escape_mermaid(&label)
        ));
    }
    for edge in &edges {
        lines.push(format!(
            "  {} -- \"{}\" --> {}",
            sanitize_mermaid_id(&edge.source_id),
            escape_mermaid(&edge.relation),
            sanitize_mermaid_id(&edge.target_id)
        ));
    }
    std::fs::write(&output_path, format!("{}\n", lines.join("\n")))?;
    Ok(format!("+ exported {output_path}\n"))
}

pub(crate) fn export_graph_graphml(
    graph: &str,
    graph_file: &GraphFile,
    args: &ExportGraphmlArgs,
) -> Result<String> {
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{graph}.graphml"));
    let (nodes, edges) = select_subgraph(
        graph_file,
        args.focus.as_deref(),
        args.depth,
        &args.node_types,
    )?;

    let mut lines = Vec::new();
    lines.push(r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string());
    lines.push(r#"<graphml xmlns="http://graphml.graphdrawing.org/xmlns" "#.to_string());
    lines.push(r#"  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#.to_string());
    lines.push(r#"  xsi:schemaLocation="http://graphml.graphdrawing.org/xmlns"#.to_string());
    lines.push(r#"  http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd">"#.to_string());
    lines.push(r#"  <key id="d0" for="node" attr.name="name" attr.type="string"/>"#.to_string());
    lines.push(r#"  <key id="d1" for="node" attr.name="type" attr.type="string"/>"#.to_string());
    lines.push(
        r#"  <key id="d2" for="node" attr.name="description" attr.type="string"/>"#.to_string(),
    );
    lines
        .push(r#"  <key id="d3" for="edge" attr.name="relation" attr.type="string"/>"#.to_string());
    lines.push(r#"  <key id="d4" for="edge" attr.name="detail" attr.type="string"/>"#.to_string());
    lines.push(format!(
        r#"  <graph id="{}" edgedefault="directed">"#,
        escape_xml(graph)
    ));

    for node in &nodes {
        lines.push(format!(r#"    <node id="{}">"#, escape_xml(&node.id)));
        lines.push(format!(
            r#"      <data key="d0">{}</data>"#,
            escape_xml(&node.name)
        ));
        lines.push(format!(
            r#"      <data key="d1">{}</data>"#,
            escape_xml(&node.r#type)
        ));
        lines.push(format!(
            r#"      <data key="d2">{}</data>"#,
            escape_xml(&node.properties.description)
        ));
        lines.push("    </node>".to_string());
    }

    for edge in &edges {
        lines.push(format!(
            r#"    <edge source="{}" target="{}">"#,
            escape_xml(&edge.source_id),
            escape_xml(&edge.target_id)
        ));
        lines.push(format!(
            r#"      <data key="d3">{}</data>"#,
            escape_xml(&edge.relation)
        ));
        lines.push(format!(
            r#"      <data key="d4">{}</data>"#,
            escape_xml(&edge.properties.detail)
        ));
        lines.push("    </edge>".to_string());
    }

    lines.push("  </graph>".to_string());
    lines.push("</graphml>".to_string());

    std::fs::write(&output_path, lines.join("\n"))?;
    Ok(format!("+ exported {output_path}\n"))
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn export_graph_md(
    graph: &str,
    graph_file: &GraphFile,
    args: &ExportMdArgs,
    _cwd: &Path,
) -> Result<String> {
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{}-md", graph));

    let (nodes, edges) = select_subgraph(
        graph_file,
        args.focus.as_deref(),
        args.depth,
        &args.node_types,
    )?;

    std::fs::create_dir_all(&output_dir)?;

    let mut index_lines = format!("# {}\n\nNodes: {}\n\n## Index\n", graph, nodes.len());

    for node in &nodes {
        let safe_name = sanitize_filename(&node.id);
        let filename = format!("{}.md", safe_name);
        let filepath = Path::new(&output_dir).join(&filename);

        let mut content = String::new();
        content.push_str(&format!("# {}\n\n", node.name));
        content.push_str(&format!("**ID:** `{}`\n\n", node.id));
        content.push_str(&format!("**Type:** {}\n\n", node.r#type));

        if !node.properties.description.is_empty() {
            content.push_str(&format!(
                "## Description\n\n{}\n\n",
                node.properties.description
            ));
        }

        if !node.properties.key_facts.is_empty() {
            content.push_str("## Facts\n\n");
            for fact in &node.properties.key_facts {
                content.push_str(&format!("- {}\n", fact));
            }
            content.push('\n');
        }

        if !node.properties.alias.is_empty() {
            content.push_str(&format!(
                "**Aliases:** {}\n\n",
                node.properties.alias.join(", ")
            ));
        }

        content.push_str("## Relations\n\n");
        for edge in &edges {
            if edge.source_id == node.id {
                content.push_str(&format!(
                    "- [[{}]] --({})--> [[{}]]\n",
                    node.id, edge.relation, edge.target_id
                ));
            } else if edge.target_id == node.id {
                content.push_str(&format!(
                    "- [[{}]] <--({})-- [[{}]]\n",
                    edge.source_id, edge.relation, node.id
                ));
            }
        }
        content.push('\n');

        content.push_str("## Backlinks\n\n");
        let backlinks: Vec<_> = edges.iter().filter(|e| e.target_id == node.id).collect();
        if backlinks.is_empty() {
            content.push_str("_No backlinks_\n");
        } else {
            for edge in backlinks {
                content.push_str(&format!("- [[{}]] ({})\n", edge.source_id, edge.relation));
            }
        }

        std::fs::write(&filepath, content)?;

        index_lines.push_str(&format!(
            "- [[{}]] - {} [{}]\n",
            node.id, node.name, node.r#type
        ));
    }

    std::fs::write(Path::new(&output_dir).join("index.md"), index_lines)?;

    Ok(format!(
        "+ exported {}/ ({} nodes)\n",
        output_dir,
        nodes.len()
    ))
}

fn sanitize_filename(name: &str) -> String {
    name.replace([':', '/', '\\', ' '], "_").replace('&', "and")
}

pub(crate) fn split_graph(graph: &str, graph_file: &GraphFile, args: &SplitArgs) -> Result<String> {
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{}-split", graph));

    let nodes_dir = Path::new(&output_dir).join("nodes");
    let edges_dir = Path::new(&output_dir).join("edges");
    let notes_dir = Path::new(&output_dir).join("notes");
    let meta_dir = Path::new(&output_dir).join("metadata");

    std::fs::create_dir_all(&nodes_dir)?;
    std::fs::create_dir_all(&edges_dir)?;
    std::fs::create_dir_all(&notes_dir)?;
    std::fs::create_dir_all(&meta_dir)?;

    let meta_json = serde_json::to_string_pretty(&graph_file.metadata)?;
    std::fs::write(meta_dir.join("metadata.json"), meta_json)?;

    let mut node_count = 0;
    for node in &graph_file.nodes {
        let safe_id = sanitize_filename(&node.id);
        let filepath = nodes_dir.join(format!("{}.json", safe_id));
        let node_json = serde_json::to_string_pretty(node)?;
        std::fs::write(filepath, node_json)?;
        node_count += 1;
    }

    let mut edge_count = 0;
    for edge in &graph_file.edges {
        let edge_key = format!(
            "{}___{}___{}",
            sanitize_filename(&edge.source_id),
            sanitize_filename(&edge.relation),
            sanitize_filename(&edge.target_id)
        );
        let filepath = edges_dir.join(format!("{}.json", edge_key));
        let edge_json = serde_json::to_string_pretty(edge)?;
        std::fs::write(filepath, edge_json)?;
        edge_count += 1;
    }

    let mut note_count = 0;
    for note in &graph_file.notes {
        let safe_id = sanitize_filename(&note.id);
        let filepath = notes_dir.join(format!("{}.json", safe_id));
        let note_json = serde_json::to_string_pretty(note)?;
        std::fs::write(filepath, note_json)?;
        note_count += 1;
    }

    let manifest = format!(
        r#"# {} Split Manifest

This directory contains a git-friendly split representation of the graph.

## Structure

- `metadata/metadata.json` - Graph metadata
- `nodes/` - One JSON file per node (filename = sanitized node id)
- `edges/` - One JSON file per edge (filename = source___relation___target)
- `notes/` - One JSON file per note

## Stats

- Nodes: {}
- Edges: {}
- Notes: {}

## Usage

To reassemble into a single JSON file, use `kg {} import-json`.
"#,
        graph, node_count, edge_count, note_count, graph
    );
    std::fs::write(Path::new(&output_dir).join("MANIFEST.md"), manifest)?;

    Ok(format!(
        "+ split {} into {}/ (nodes: {}, edges: {}, notes: {})\n",
        graph, output_dir, node_count, edge_count, note_count
    ))
}

fn select_subgraph<'a>(
    graph_file: &'a GraphFile,
    focus: Option<&'a str>,
    depth: usize,
    node_types: &'a [String],
) -> Result<(Vec<&'a Node>, Vec<&'a Edge>)> {
    use std::collections::{HashSet, VecDeque};

    let mut selected: HashSet<String> = HashSet::new();
    if let Some(focus_id) = focus {
        if graph_file.node_by_id(focus_id).is_none() {
            bail!("focus node not found: {focus_id}");
        }
        selected.insert(focus_id.to_owned());
        let mut frontier = VecDeque::new();
        frontier.push_back((focus_id.to_owned(), 0usize));
        while let Some((current, dist)) = frontier.pop_front() {
            if dist >= depth {
                continue;
            }
            for edge in &graph_file.edges {
                let next = if edge.source_id == current {
                    Some(edge.target_id.clone())
                } else if edge.target_id == current {
                    Some(edge.source_id.clone())
                } else {
                    None
                };
                if let Some(next_id) = next {
                    if selected.insert(next_id.clone()) {
                        frontier.push_back((next_id, dist + 1));
                    }
                }
            }
        }
    } else {
        for node in &graph_file.nodes {
            selected.insert(node.id.clone());
        }
    }

    let type_filter: Vec<String> = node_types.iter().map(|t| t.to_lowercase()).collect();
    let has_filter = !type_filter.is_empty();
    let mut nodes: Vec<&Node> = graph_file
        .nodes
        .iter()
        .filter(|node| selected.contains(&node.id))
        .filter(|node| {
            if let Some(focus_id) = focus {
                if node.id == focus_id {
                    return true;
                }
            }
            !has_filter || type_filter.contains(&node.r#type.to_lowercase())
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let node_set: HashSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
    let mut edges: Vec<&Edge> = graph_file
        .edges
        .iter()
        .filter(|edge| node_set.contains(&edge.source_id) && node_set.contains(&edge.target_id))
        .collect();
    edges.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.relation.cmp(&b.relation))
            .then_with(|| a.target_id.cmp(&b.target_id))
    });

    Ok((nodes, edges))
}

fn escape_dot(value: &str) -> String {
    value.replace('"', "\\\"").replace('\n', "\\n")
}

fn escape_mermaid(value: &str) -> String {
    value.replace('"', "\\\"").replace('\n', "\\n")
}

fn sanitize_mermaid_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "node".to_owned()
    } else {
        out
    }
}

pub(crate) fn render_graph_history(path: &Path, graph: &str, args: &HistoryArgs) -> Result<String> {
    let backups = list_graph_backups(path)?;
    let total = backups.len();
    let snapshots: Vec<(u64, PathBuf)> = backups.into_iter().rev().take(args.limit).collect();

    if args.json {
        let payload = GraphHistoryResponse {
            graph: graph.to_owned(),
            total,
            snapshots: snapshots
                .iter()
                .map(|(ts, backup_path)| GraphHistorySnapshot {
                    ts: *ts,
                    path: backup_path.display().to_string(),
                })
                .collect(),
        };
        let rendered =
            serde_json::to_string_pretty(&payload).context("failed to render history as JSON")?;
        return Ok(format!("{rendered}\n"));
    }

    let mut lines = vec![format!("= history {graph} ({total})")];
    for (ts, backup_path) in snapshots {
        lines.push(format!("- {ts} | {}", backup_path.display()));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

pub(crate) fn render_graph_timeline(
    path: &Path,
    graph: &str,
    args: &TimelineArgs,
) -> Result<String> {
    let entries = event_log::read_log(path)?;
    let total = entries.len();
    let filtered: Vec<&event_log::EventLogEntry> = entries
        .iter()
        .filter(|entry| {
            let after_since = args
                .since_ts_ms
                .map(|since| entry.ts_ms >= since)
                .unwrap_or(true);
            let before_until = args
                .until_ts_ms
                .map(|until| entry.ts_ms <= until)
                .unwrap_or(true);
            after_since && before_until
        })
        .collect();
    let recent: Vec<&event_log::EventLogEntry> =
        filtered.into_iter().rev().take(args.limit).collect();

    if args.json {
        let payload = GraphTimelineResponse {
            graph: graph.to_owned(),
            total,
            filtered: recent.len(),
            since_ts_ms: args.since_ts_ms,
            until_ts_ms: args.until_ts_ms,
            entries: recent
                .iter()
                .map(|entry| GraphTimelineEntry {
                    ts_ms: entry.ts_ms,
                    action: entry.action.clone(),
                    detail: entry.detail.clone(),
                    node_count: entry.graph.nodes.len(),
                    edge_count: entry.graph.edges.len(),
                    note_count: entry.graph.notes.len(),
                })
                .collect(),
        };
        let rendered =
            serde_json::to_string_pretty(&payload).context("failed to render timeline as JSON")?;
        return Ok(format!("{rendered}\n"));
    }

    let mut lines = vec![format!("= timeline {graph} ({total})")];
    if args.since_ts_ms.is_some() || args.until_ts_ms.is_some() {
        lines.push(format!(
            "range: {} -> {}",
            args.since_ts_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-inf".to_owned()),
            args.until_ts_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "+inf".to_owned())
        ));
        lines.push(format!("showing: {}", recent.len()));
    }
    for entry in recent {
        let detail = entry
            .detail
            .as_deref()
            .map(|value| format!(" | {value}"))
            .unwrap_or_default();
        lines.push(format!(
            "- {} | {}{} | nodes: {} | edges: {} | notes: {}",
            entry.ts_ms,
            entry.action,
            detail,
            entry.graph.nodes.len(),
            entry.graph.edges.len(),
            entry.graph.notes.len()
        ));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

#[derive(Debug, Serialize)]
struct GraphHistorySnapshot {
    ts: u64,
    path: String,
}

#[derive(Debug, Serialize)]
struct GraphHistoryResponse {
    graph: String,
    total: usize,
    snapshots: Vec<GraphHistorySnapshot>,
}

#[derive(Debug, Serialize)]
struct GraphTimelineEntry {
    ts_ms: u64,
    action: String,
    detail: Option<String>,
    node_count: usize,
    edge_count: usize,
    note_count: usize,
}

#[derive(Debug, Serialize)]
struct GraphTimelineResponse {
    graph: String,
    total: usize,
    filtered: usize,
    since_ts_ms: Option<u64>,
    until_ts_ms: Option<u64>,
    entries: Vec<GraphTimelineEntry>,
}

pub(crate) fn render_graph_diff_as_of(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    match resolve_temporal_source(path, args.source)? {
        TemporalSource::EventLog => render_graph_diff_as_of_event_log(path, graph, args),
        _ => render_graph_diff_as_of_backups(path, graph, args),
    }
}

pub(crate) fn render_graph_diff_as_of_json(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    match resolve_temporal_source(path, args.source)? {
        TemporalSource::EventLog => render_graph_diff_as_of_event_log_json(path, graph, args),
        _ => render_graph_diff_as_of_backups_json(path, graph, args),
    }
}

fn render_graph_diff_as_of_backups(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    let backups = list_graph_backups(path)?;
    if backups.is_empty() {
        bail!("no backups found for graph: {graph}");
    }
    let from_ts = args.from_ts_ms / 1000;
    let to_ts = args.to_ts_ms / 1000;
    let from_backup = select_backup_at_or_before(&backups, from_ts)
        .ok_or_else(|| anyhow!("no backup at or before from_ts_ms={}", args.from_ts_ms))?;
    let to_backup = select_backup_at_or_before(&backups, to_ts)
        .ok_or_else(|| anyhow!("no backup at or before to_ts_ms={}", args.to_ts_ms))?;

    let from_graph = load_graph_from_backup(&from_backup.1)?;
    let to_graph = load_graph_from_backup(&to_backup.1)?;
    let left_label = format!("{graph}@{}", args.from_ts_ms);
    let right_label = format!("{graph}@{}", args.to_ts_ms);
    Ok(render_graph_diff_from_files(
        &left_label,
        &right_label,
        &from_graph,
        &to_graph,
    ))
}

fn render_graph_diff_as_of_backups_json(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    let backups = list_graph_backups(path)?;
    if backups.is_empty() {
        bail!("no backups found for graph: {graph}");
    }
    let from_ts = args.from_ts_ms / 1000;
    let to_ts = args.to_ts_ms / 1000;
    let from_backup = select_backup_at_or_before(&backups, from_ts)
        .ok_or_else(|| anyhow!("no backup at or before from_ts_ms={}", args.from_ts_ms))?;
    let to_backup = select_backup_at_or_before(&backups, to_ts)
        .ok_or_else(|| anyhow!("no backup at or before to_ts_ms={}", args.to_ts_ms))?;

    let from_graph = load_graph_from_backup(&from_backup.1)?;
    let to_graph = load_graph_from_backup(&to_backup.1)?;
    let left_label = format!("{graph}@{}", args.from_ts_ms);
    let right_label = format!("{graph}@{}", args.to_ts_ms);
    Ok(render_graph_diff_json_from_files(
        &left_label,
        &right_label,
        &from_graph,
        &to_graph,
    ))
}

fn render_graph_diff_as_of_event_log(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    let entries = event_log::read_log(path)?;
    if entries.is_empty() {
        bail!("no event log entries found for graph: {graph}");
    }
    let from_entry = select_event_at_or_before(&entries, args.from_ts_ms).ok_or_else(|| {
        anyhow!(
            "no event log entry at or before from_ts_ms={}",
            args.from_ts_ms
        )
    })?;
    let to_entry = select_event_at_or_before(&entries, args.to_ts_ms)
        .ok_or_else(|| anyhow!("no event log entry at or before to_ts_ms={}", args.to_ts_ms))?;

    let left_label = format!("{graph}@{}", args.from_ts_ms);
    let right_label = format!("{graph}@{}", args.to_ts_ms);
    Ok(render_graph_diff_from_files(
        &left_label,
        &right_label,
        &from_entry.graph,
        &to_entry.graph,
    ))
}

fn render_graph_diff_as_of_event_log_json(
    path: &Path,
    graph: &str,
    args: &DiffAsOfArgs,
) -> Result<String> {
    let entries = event_log::read_log(path)?;
    if entries.is_empty() {
        bail!("no event log entries found for graph: {graph}");
    }
    let from_entry = select_event_at_or_before(&entries, args.from_ts_ms).ok_or_else(|| {
        anyhow!(
            "no event log entry at or before from_ts_ms={}",
            args.from_ts_ms
        )
    })?;
    let to_entry = select_event_at_or_before(&entries, args.to_ts_ms)
        .ok_or_else(|| anyhow!("no event log entry at or before to_ts_ms={}", args.to_ts_ms))?;

    let left_label = format!("{graph}@{}", args.from_ts_ms);
    let right_label = format!("{graph}@{}", args.to_ts_ms);
    Ok(render_graph_diff_json_from_files(
        &left_label,
        &right_label,
        &from_entry.graph,
        &to_entry.graph,
    ))
}

fn resolve_temporal_source(path: &Path, source: TemporalSource) -> Result<TemporalSource> {
    if matches!(source, TemporalSource::Auto) {
        let has_events = event_log::has_log(path);
        return Ok(if has_events {
            TemporalSource::EventLog
        } else {
            TemporalSource::Backups
        });
    }
    Ok(source)
}

fn select_event_at_or_before(
    entries: &[event_log::EventLogEntry],
    target_ts_ms: u64,
) -> Option<&event_log::EventLogEntry> {
    let mut selected = None;
    for entry in entries {
        if entry.ts_ms <= target_ts_ms {
            selected = Some(entry);
        }
    }
    selected
}

fn select_backup_at_or_before(
    backups: &[(u64, PathBuf)],
    target_ts: u64,
) -> Option<(u64, PathBuf)> {
    let mut selected = None;
    for (ts, path) in backups {
        if *ts <= target_ts {
            selected = Some((*ts, path.clone()));
        }
    }
    selected
}

fn load_graph_from_backup(path: &Path) -> Result<GraphFile> {
    let raw = read_gz_to_string(path)?;
    let graph: GraphFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse backup: {}", path.display()))?;
    Ok(graph)
}

pub(crate) fn render_note_list(graph: &GraphFile, args: &NoteListArgs) -> String {
    let mut notes: Vec<&Note> = graph
        .notes
        .iter()
        .filter(|note| args.node.as_ref().is_none_or(|node| note.node_id == *node))
        .collect();

    notes.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });

    let total = notes.len();
    let visible: Vec<&Note> = notes.into_iter().take(args.limit).collect();

    let mut lines = vec![format!("= notes ({total})")];
    for note in &visible {
        let mut line = format!(
            "- {} | {} | {} | {}",
            note.id,
            note.node_id,
            note.created_at,
            truncate_note(&escape_cli_text(&note.body), 80)
        );
        if !note.tags.is_empty() {
            line.push_str(" | tags: ");
            line.push_str(
                &note
                    .tags
                    .iter()
                    .map(|tag| escape_cli_text(tag))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !note.author.is_empty() {
            line.push_str(" | by: ");
            line.push_str(&escape_cli_text(&note.author));
        }
        lines.push(line);
    }
    let omitted = total.saturating_sub(visible.len());
    if omitted > 0 {
        lines.push(format!("... {omitted} more notes omitted"));
    }

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn build_note(graph: &GraphFile, args: NoteAddArgs) -> Result<Note> {
    if graph.node_by_id(&args.node_id).is_none() {
        bail!("node not found: {}", args.node_id);
    }
    let ts = now_ms();
    let id = args.id.unwrap_or_else(|| format!("note:{ts}"));
    let created_at = args.created_at.unwrap_or_else(|| ts.to_string());
    Ok(Note {
        id,
        node_id: args.node_id,
        body: args.text,
        tags: args.tag,
        author: args.author.unwrap_or_default(),
        created_at,
        provenance: args.provenance.unwrap_or_default(),
        source_files: args.source,
    })
}

fn truncate_note(value: &str, max_len: usize) -> String {
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

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn map_find_mode(mode: CliFindMode) -> output::FindMode {
    match mode {
        CliFindMode::Fuzzy => output::FindMode::Fuzzy,
        CliFindMode::Bm25 => output::FindMode::Bm25,
        CliFindMode::Hybrid => output::FindMode::Hybrid,
        CliFindMode::Vector => output::FindMode::Fuzzy,
    }
}

pub(crate) fn render_feedback_log(cwd: &Path, args: &FeedbackLogArgs) -> Result<String> {
    let path = first_existing_feedback_log_path(cwd);
    if !path.exists() {
        return Ok(String::from("= feedback-log\nempty: no entries yet\n"));
    }

    let content = std::fs::read_to_string(&path)?;
    let mut entries: Vec<FeedbackLogEntry> = Vec::new();
    for line in content.lines() {
        if let Some(entry) = FeedbackLogEntry::parse(line) {
            if let Some(ref uid) = args.uid {
                if &entry.uid != uid {
                    continue;
                }
            }
            if let Some(ref graph) = args.graph {
                if &entry.graph != graph {
                    continue;
                }
            }
            entries.push(entry);
        }
    }

    entries.reverse();
    let shown: Vec<&FeedbackLogEntry> = entries.iter().take(args.limit).collect();

    let mut output = vec![String::from("= feedback-log")];
    output.push(format!("total_entries: {}", entries.len()));
    output.push(format!("showing: {}", shown.len()));
    output.push(String::from("recent_entries:"));
    for e in shown {
        let pick = e.pick.as_deref().unwrap_or("-");
        let selected = e.selected.as_deref().unwrap_or("-");
        let graph = if e.graph.is_empty() { "-" } else { &e.graph };
        let queries = if e.queries.is_empty() {
            "-"
        } else {
            &e.queries
        };
        output.push(format!(
            "- {} | {} | {} | pick={} | selected={} | graph={} | {}",
            e.ts_ms, e.uid, e.action, pick, selected, graph, queries
        ));
    }

    Ok(format!("{}\n", output.join("\n")))
}

pub(crate) fn handle_vector_command(
    path: &Path,
    _graph: &str,
    graph_file: &GraphFile,
    command: &VectorCommand,
    _cwd: &Path,
) -> Result<String> {
    match command {
        VectorCommand::Import(args) => {
            let vector_path = path
                .parent()
                .map(|p| p.join(".kg.vectors.json"))
                .unwrap_or_else(|| PathBuf::from(".kg.vectors.json"));
            let store =
                vectors::VectorStore::import_jsonl(std::path::Path::new(&args.input), graph_file)?;
            store.save(&vector_path)?;
            Ok(format!(
                "+ imported {} vectors (dim={}) to {}\n",
                store.vectors.len(),
                store.dimension,
                vector_path.display()
            ))
        }
        VectorCommand::Stats(_args) => {
            let vector_path = path
                .parent()
                .map(|p| p.join(".kg.vectors.json"))
                .unwrap_or_else(|| PathBuf::from(".kg.vectors.json"));
            if !vector_path.exists() {
                return Ok(String::from("= vectors\nnot initialized\n"));
            }
            let store = vectors::VectorStore::load(&vector_path)?;
            let node_ids: Vec<_> = store.vectors.keys().cloned().collect();
            let in_graph = node_ids
                .iter()
                .filter(|id| graph_file.node_by_id(id).is_some())
                .count();
            Ok(format!(
                "= vectors\ndimension: {}\ntotal: {}\nin_graph: {}\n",
                store.dimension,
                store.vectors.len(),
                in_graph
            ))
        }
    }
}

fn render_feedback_summary(cwd: &Path, args: &FeedbackSummaryArgs) -> Result<String> {
    use std::collections::HashMap;

    let path = first_existing_feedback_log_path(cwd);
    if !path.exists() {
        return Ok(String::from("= feedback-summary\nNo feedback yet.\n"));
    }

    let content = std::fs::read_to_string(&path)?;
    let mut entries: Vec<FeedbackLogEntry> = Vec::new();
    for line in content.lines() {
        if let Some(entry) = FeedbackLogEntry::parse(line) {
            if let Some(ref graph) = args.graph {
                if &entry.graph != graph {
                    continue;
                }
            }
            entries.push(entry);
        }
    }

    entries.reverse();
    let _shown = entries.iter().take(args.limit).collect::<Vec<_>>();

    let mut lines = vec![String::from("= feedback-summary")];
    lines.push(format!("Total entries: {}", entries.len()));

    let mut by_action: HashMap<&str, usize> = HashMap::new();
    let mut nil_queries: Vec<&str> = Vec::new();
    let mut yes_count = 0;
    let mut no_count = 0;
    let mut pick_map: HashMap<&str, usize> = HashMap::new();
    let mut query_counts: HashMap<&str, usize> = HashMap::new();

    for e in &entries {
        *by_action.entry(&e.action).or_insert(0) += 1;

        match e.action.as_str() {
            "NIL" => {
                if !e.queries.is_empty() {
                    nil_queries.push(&e.queries);
                }
            }
            "YES" => yes_count += 1,
            "NO" => no_count += 1,
            "PICK" => {
                if let Some(ref sel) = e.selected {
                    *pick_map.entry(sel).or_insert(0) += 1;
                }
            }
            _ => {}
        }

        if !e.queries.is_empty() {
            *query_counts.entry(&e.queries).or_insert(0) += 1;
        }
    }

    lines.push(String::from("\n### By response"));
    lines.push(format!(
        "YES:  {} ({:.0}%)",
        yes_count,
        if !entries.is_empty() {
            (yes_count as f64 / entries.len() as f64) * 100.0
        } else {
            0.0
        }
    ));
    lines.push(format!("NO:   {}", no_count));
    lines.push(format!("PICK: {}", by_action.get("PICK").unwrap_or(&0)));
    lines.push(format!("NIL:  {} (no results)", nil_queries.len()));

    if !nil_queries.is_empty() {
        lines.push(String::from("\n### Brakujące node'y (NIL queries)"));
        for q in nil_queries.iter().take(10) {
            lines.push(format!("- \"{}\"", q));
        }
        if nil_queries.len() > 10 {
            lines.push(format!("  ... i {} więcej", nil_queries.len() - 10));
        }
    }

    if !pick_map.is_empty() {
        lines.push(String::from("\n### Najczęściej wybierane node'y (PICK)"));
        let mut sorted: Vec<_> = pick_map.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (node, count) in sorted.iter().take(10) {
            lines.push(format!("- {} ({}x)", node, count));
        }
    }

    if !query_counts.is_empty() {
        lines.push(String::from("\n### Top wyszukiwane terminy"));
        let mut sorted: Vec<_> = query_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (query, count) in sorted.iter().take(10) {
            lines.push(format!("- \"{}\" ({})", query, count));
        }
    }

    if yes_count == 0 && no_count == 0 && nil_queries.is_empty() {
        lines.push(String::from(
            "\n(Wpływy za mało na wnioski - potrzeba więcej feedbacku)",
        ));
    } else if yes_count > no_count * 3 {
        lines.push(String::from(
            "\n✓ Feedback pozytywny - wyszukiwania działają dobrze.",
        ));
    } else if no_count > yes_count {
        lines.push(String::from(
            "\n⚠ Dużo NO - sprawdź jakość aliasów i dopasowań.",
        ));
    }

    Ok(format!("{}\n", lines.join("\n")))
}

pub fn feedback_log_path(cwd: &Path) -> PathBuf {
    cache_paths::cache_root_for_cwd(cwd).join("kg-mcp.feedback.log")
}

fn legacy_feedback_log_path(cwd: &Path) -> PathBuf {
    cwd.join("kg-mcp.feedback.log")
}

pub fn first_existing_feedback_log_path(cwd: &Path) -> PathBuf {
    let preferred = feedback_log_path(cwd);
    if preferred.exists() {
        return preferred;
    }
    let legacy = legacy_feedback_log_path(cwd);
    if legacy.exists() {
        return legacy;
    }
    preferred
}

pub(crate) fn render_feedback_summary_for_graph(
    cwd: &Path,
    graph: &str,
    args: &FeedbackSummaryArgs,
) -> Result<String> {
    let mut args = args.clone();
    args.graph = Some(graph.to_string());
    render_feedback_summary(cwd, &args)
}

#[derive(Debug, Serialize)]
struct BaselineFeedbackMetrics {
    entries: usize,
    yes: usize,
    no: usize,
    pick: usize,
    nil: usize,
    yes_rate: f64,
    no_rate: f64,
    nil_rate: f64,
}

#[derive(Debug, Serialize)]
struct BaselineCostMetrics {
    find_operations: usize,
    feedback_events: usize,
    feedback_events_per_1000_find_ops: f64,
    token_cost_estimate: Option<f64>,
    token_cost_note: &'static str,
}

#[derive(Debug, Serialize)]
struct GoldenSetMetrics {
    cases: usize,
    hits_any: usize,
    top1_hits: usize,
    hit_rate: f64,
    top1_rate: f64,
    mrr: f64,
    ndcg_at_k: f64,
}

#[derive(Debug, Serialize)]
struct BaselineQualityScore {
    description_coverage: f64,
    facts_coverage: f64,
    duplicate_penalty: f64,
    edge_gap_penalty: f64,
    score_0_100: f64,
}

#[derive(Debug, Serialize)]
struct BaselineReport {
    graph: String,
    quality: crate::analysis::QualitySnapshot,
    quality_score: BaselineQualityScore,
    feedback: BaselineFeedbackMetrics,
    cost: BaselineCostMetrics,
    golden: Option<GoldenSetMetrics>,
}

#[derive(Debug, Deserialize)]
struct GoldenSetCase {
    query: String,
    expected: Vec<String>,
}

fn parse_feedback_entries(cwd: &Path, graph_name: &str) -> Result<Vec<FeedbackLogEntry>> {
    let path = first_existing_feedback_log_path(cwd);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for line in content.lines() {
        if let Some(entry) = FeedbackLogEntry::parse(line) {
            if entry.graph == graph_name {
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

fn parse_find_operations(graph_path: &Path) -> Result<usize> {
    let Some(path) = access_log::first_existing_access_log_path(graph_path) else {
        return Ok(0);
    };

    let content = std::fs::read_to_string(path)?;
    let mut find_ops = 0usize;
    for line in content.lines() {
        let mut parts = line.split('\t');
        let _ts = parts.next();
        if let Some(op) = parts.next() {
            if op == "FIND" {
                find_ops += 1;
            }
        }
    }
    Ok(find_ops)
}

fn compute_feedback_metrics(entries: &[FeedbackLogEntry]) -> BaselineFeedbackMetrics {
    let mut yes = 0usize;
    let mut no = 0usize;
    let mut pick = 0usize;
    let mut nil = 0usize;
    for entry in entries {
        match entry.action.as_str() {
            "YES" => yes += 1,
            "NO" => no += 1,
            "PICK" => pick += 1,
            "NIL" => nil += 1,
            _ => {}
        }
    }
    let total = entries.len() as f64;
    BaselineFeedbackMetrics {
        entries: entries.len(),
        yes,
        no,
        pick,
        nil,
        yes_rate: if total > 0.0 { yes as f64 / total } else { 0.0 },
        no_rate: if total > 0.0 { no as f64 / total } else { 0.0 },
        nil_rate: if total > 0.0 { nil as f64 / total } else { 0.0 },
    }
}

fn compute_quality_score(snapshot: &crate::analysis::QualitySnapshot) -> BaselineQualityScore {
    let total_nodes = snapshot.total_nodes as f64;
    let description_coverage = if total_nodes > 0.0 {
        (snapshot
            .total_nodes
            .saturating_sub(snapshot.missing_descriptions)) as f64
            / total_nodes
    } else {
        1.0
    };
    let facts_coverage = if total_nodes > 0.0 {
        (snapshot.total_nodes.saturating_sub(snapshot.missing_facts)) as f64 / total_nodes
    } else {
        1.0
    };

    let duplicate_penalty = if snapshot.total_nodes > 1 {
        let max_pairs = (snapshot.total_nodes * (snapshot.total_nodes - 1) / 2) as f64;
        (snapshot.duplicate_pairs as f64 / max_pairs).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let edge_candidates = snapshot.edge_gaps.total_candidates();
    let edge_gap_penalty = if edge_candidates > 0 {
        (snapshot.edge_gaps.total_missing() as f64 / edge_candidates as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let score = 100.0
        * (0.35 * description_coverage
            + 0.35 * facts_coverage
            + 0.15 * (1.0 - duplicate_penalty)
            + 0.15 * (1.0 - edge_gap_penalty));

    BaselineQualityScore {
        description_coverage,
        facts_coverage,
        duplicate_penalty,
        edge_gap_penalty,
        score_0_100: score,
    }
}

fn eval_golden_set(graph: &GraphFile, args: &BaselineArgs) -> Result<Option<GoldenSetMetrics>> {
    if matches!(args.mode, CliFindMode::Vector) {
        anyhow::bail!("baseline does not support --mode vector");
    }

    let Some(path) = args.golden.as_ref() else {
        return Ok(None);
    };

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read golden set: {path}"))?;
    let cases: Vec<GoldenSetCase> =
        serde_json::from_str(&raw).with_context(|| format!("invalid golden set JSON: {path}"))?;

    if cases.is_empty() {
        return Ok(Some(GoldenSetMetrics {
            cases: 0,
            hits_any: 0,
            top1_hits: 0,
            hit_rate: 0.0,
            top1_rate: 0.0,
            mrr: 0.0,
            ndcg_at_k: 0.0,
        }));
    }

    let mode = map_find_mode(args.mode);
    let mut hits_any = 0usize;
    let mut top1_hits = 0usize;
    let mut mrr_sum = 0.0;
    let mut ndcg_sum = 0.0;

    for case in &cases {
        let results = output::find_nodes(
            graph,
            &case.query,
            args.find_limit,
            args.include_features,
            false,
            mode,
        );

        let mut first_rank: Option<usize> = None;
        for (idx, node) in results.iter().enumerate() {
            if case.expected.iter().any(|id| id == &node.id) {
                first_rank = Some(idx + 1);
                break;
            }
        }

        if let Some(rank) = first_rank {
            hits_any += 1;
            if rank == 1 {
                top1_hits += 1;
            }
            mrr_sum += 1.0 / rank as f64;
        }

        let mut dcg = 0.0;
        for (idx, node) in results.iter().enumerate() {
            if case.expected.iter().any(|id| id == &node.id) {
                let denom = (idx as f64 + 2.0).log2();
                dcg += 1.0 / denom;
            }
        }
        let ideal_hits = case.expected.len().min(results.len());
        let mut idcg = 0.0;
        for rank in 0..ideal_hits {
            let denom = (rank as f64 + 2.0).log2();
            idcg += 1.0 / denom;
        }
        if idcg > 0.0 {
            ndcg_sum += dcg / idcg;
        }
    }

    let total = cases.len() as f64;
    Ok(Some(GoldenSetMetrics {
        cases: cases.len(),
        hits_any,
        top1_hits,
        hit_rate: hits_any as f64 / total,
        top1_rate: top1_hits as f64 / total,
        mrr: mrr_sum / total,
        ndcg_at_k: ndcg_sum / total,
    }))
}

pub(crate) fn render_baseline_report(
    cwd: &Path,
    graph_name: &str,
    graph: &GraphFile,
    quality: &crate::analysis::QualitySnapshot,
    args: &BaselineArgs,
) -> Result<String> {
    let feedback_entries = parse_feedback_entries(cwd, graph_name)?;
    let feedback = compute_feedback_metrics(&feedback_entries);

    let graph_root = default_graph_root(cwd);
    let graph_path = resolve_graph_path(cwd, &graph_root, graph_name)?;
    let find_operations = parse_find_operations(&graph_path)?;

    let cost = BaselineCostMetrics {
        find_operations,
        feedback_events: feedback.entries,
        feedback_events_per_1000_find_ops: if find_operations > 0 {
            (feedback.entries as f64 / find_operations as f64) * 1000.0
        } else {
            0.0
        },
        token_cost_estimate: None,
        token_cost_note: "token cost unavailable in current logs (instrumentation pending)",
    };

    let quality_score = compute_quality_score(quality);
    let golden = eval_golden_set(graph, args)?;

    let report = BaselineReport {
        graph: graph_name.to_owned(),
        quality: crate::analysis::QualitySnapshot {
            total_nodes: quality.total_nodes,
            missing_descriptions: quality.missing_descriptions,
            missing_facts: quality.missing_facts,
            duplicate_pairs: quality.duplicate_pairs,
            edge_gaps: crate::analysis::EdgeGapSnapshot {
                datastore_candidates: quality.edge_gaps.datastore_candidates,
                datastore_missing_stored_in: quality.edge_gaps.datastore_missing_stored_in,
                process_candidates: quality.edge_gaps.process_candidates,
                process_missing_incoming: quality.edge_gaps.process_missing_incoming,
            },
        },
        quality_score,
        feedback,
        cost,
        golden,
    };

    if args.json {
        let rendered = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned());
        return Ok(format!("{rendered}\n"));
    }

    let mut lines = vec![String::from("= baseline")];
    lines.push(format!("graph: {}", report.graph));
    lines.push(format!(
        "quality_score_0_100: {:.1}",
        report.quality_score.score_0_100
    ));
    lines.push(String::from("quality:"));
    lines.push(format!("- total_nodes: {}", report.quality.total_nodes));
    lines.push(format!(
        "- missing_descriptions: {} ({:.1}%)",
        report.quality.missing_descriptions,
        report
            .quality_score
            .description_coverage
            .mul_add(-100.0, 100.0)
    ));
    lines.push(format!(
        "- missing_facts: {} ({:.1}%)",
        report.quality.missing_facts,
        report.quality_score.facts_coverage.mul_add(-100.0, 100.0)
    ));
    lines.push(format!(
        "- duplicate_pairs: {}",
        report.quality.duplicate_pairs
    ));
    lines.push(format!(
        "- edge_gaps: {} / {}",
        report.quality.edge_gaps.total_missing(),
        report.quality.edge_gaps.total_candidates()
    ));

    lines.push(String::from("feedback:"));
    lines.push(format!("- entries: {}", report.feedback.entries));
    lines.push(format!(
        "- YES/NO/NIL/PICK: {}/{}/{}/{}",
        report.feedback.yes, report.feedback.no, report.feedback.nil, report.feedback.pick
    ));
    lines.push(format!(
        "- yes_rate: {:.1}%",
        report.feedback.yes_rate * 100.0
    ));
    lines.push(format!(
        "- no_rate: {:.1}%",
        report.feedback.no_rate * 100.0
    ));

    lines.push(String::from("cost:"));
    lines.push(format!(
        "- find_operations: {}",
        report.cost.find_operations
    ));
    lines.push(format!(
        "- feedback_events: {}",
        report.cost.feedback_events
    ));
    lines.push(format!(
        "- feedback_events_per_1000_find_ops: {:.1}",
        report.cost.feedback_events_per_1000_find_ops
    ));
    lines.push(format!("- token_cost: {}", report.cost.token_cost_note));

    if let Some(golden) = report.golden {
        lines.push(String::from("golden_set:"));
        lines.push(format!("- cases: {}", golden.cases));
        lines.push(format!("- hit_rate: {:.1}%", golden.hit_rate * 100.0));
        lines.push(format!("- top1_rate: {:.1}%", golden.top1_rate * 100.0));
        lines.push(format!("- mrr: {:.3}", golden.mrr));
        lines.push(format!("- ndcg@k: {:.3}", golden.ndcg_at_k));
    }

    Ok(format!("{}\n", lines.join("\n")))
}

#[derive(Debug, Clone)]
struct FeedbackLogEntry {
    ts_ms: String,
    uid: String,
    action: String,
    pick: Option<String>,
    selected: Option<String>,
    graph: String,
    queries: String,
}

impl FeedbackLogEntry {
    fn parse(line: &str) -> Option<Self> {
        // Expected (tab-separated):
        // ts_ms=...\tuid=...\taction=...\tpick=...\tselected=...\tgraph=...\tqueries=...
        let mut ts_ms: Option<String> = None;
        let mut uid: Option<String> = None;
        let mut action: Option<String> = None;
        let mut pick: Option<String> = None;
        let mut selected: Option<String> = None;
        let mut graph: Option<String> = None;
        let mut queries: Option<String> = None;

        for part in line.split('\t') {
            let (k, v) = part.split_once('=')?;
            let v = v.trim();
            match k {
                "ts_ms" => ts_ms = Some(v.to_owned()),
                "uid" => uid = Some(v.to_owned()),
                "action" => action = Some(v.to_owned()),
                "pick" => {
                    if v != "-" {
                        pick = Some(v.to_owned());
                    }
                }
                "selected" => {
                    if v != "-" {
                        selected = Some(v.to_owned());
                    }
                }
                "graph" => {
                    if v != "-" {
                        graph = Some(v.to_owned());
                    }
                }
                "queries" => {
                    if v != "-" {
                        queries = Some(v.to_owned());
                    }
                }
                _ => {}
            }
        }

        Some(Self {
            ts_ms: ts_ms?,
            uid: uid?,
            action: action?,
            pick,
            selected,
            graph: graph.unwrap_or_default(),
            queries: queries.unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Graph lifecycle helpers
// ---------------------------------------------------------------------------

/// Returns the default graph root directory for this environment.
///
/// This is primarily exposed for embedding use-cases (e.g. kg-mcp), so they
/// can resolve graph paths consistently with the CLI.
pub fn default_graph_root(cwd: &Path) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from));
    graph_root_from(home.as_deref(), cwd)
}

fn graph_root_from(home: Option<&Path>, cwd: &Path) -> PathBuf {
    match home {
        Some(home) => home.join(".kg").join("graphs"),
        None => cwd.join(".kg").join("graphs"),
    }
}

/// Resolve a graph identifier/path to an on-disk JSON file.
///
/// This is primarily exposed for embedding use-cases (e.g. kg-mcp), so they
/// can resolve graph paths consistently with the CLI.
pub fn resolve_graph_path(cwd: &Path, graph_root: &Path, graph: &str) -> Result<PathBuf> {
    let store = graph_store(cwd, graph_root, false)?;
    store.resolve_graph_path(graph)
}

/// Load the MCP nudge probability from `.kg.toml`, defaulting to 20.
pub fn feedback_nudge_percent(cwd: &Path) -> Result<u8> {
    Ok(config::KgConfig::discover(cwd)?
        .map(|(_, config)| config.nudge_percent())
        .unwrap_or(config::DEFAULT_NUDGE_PERCENT))
}

/// Resolve and (if needed) persist `user_short_uid` for sidecar logging.
pub fn sidecar_user_short_uid(cwd: &Path) -> String {
    config::ensure_user_short_uid(cwd)
}

/// Best-effort append of an `F` feedback record to `<graph>.kglog`.
pub fn append_kg_feedback(graph_path: &Path, user_short_uid: &str, node_id: &str, feedback: &str) {
    let _ = kg_sidecar::append_feedback_with_uid(graph_path, user_short_uid, node_id, feedback);
}

// ---------------------------------------------------------------------------
// Validation renderers (check vs audit differ in header only)
// ---------------------------------------------------------------------------

pub(crate) fn render_check(graph: &GraphFile, cwd: &Path, args: &CheckArgs) -> String {
    let report = validate_graph(graph, cwd, args.deep, args.base_dir.as_deref());
    format_validation_report(
        "check",
        &report.errors,
        &report.warnings,
        args.errors_only,
        args.warnings_only,
        args.limit,
    )
}

pub(crate) fn render_audit(graph: &GraphFile, cwd: &Path, args: &AuditArgs) -> String {
    let report = validate_graph(graph, cwd, args.deep, args.base_dir.as_deref());
    format_validation_report(
        "audit",
        &report.errors,
        &report.warnings,
        args.errors_only,
        args.warnings_only,
        args.limit,
    )
}

fn format_validation_report(
    header: &str,
    errors: &[String],
    warnings: &[String],
    errors_only: bool,
    warnings_only: bool,
    limit: usize,
) -> String {
    let mut lines = vec![format!("= {header}")];
    lines.push(format!(
        "status: {}",
        if errors.is_empty() {
            "VALID"
        } else {
            "INVALID"
        }
    ));
    lines.push(format!("errors: {}", errors.len()));
    lines.push(format!("warnings: {}", warnings.len()));
    if !warnings_only {
        lines.push("error-list:".to_owned());
        for error in errors.iter().take(limit) {
            lines.push(format!("- {error}"));
        }
    }
    if !errors_only {
        lines.push("warning-list:".to_owned());
        for warning in warnings.iter().take(limit) {
            lines.push(format!("- {warning}"));
        }
    }
    format!("{}\n", lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture_graph() -> GraphFile {
        serde_json::from_str(include_str!("../graph-example-fridge.json")).expect("fixture graph")
    }

    fn exec_safe(args: &[&str], cwd: &Path) -> Result<String> {
        run_args_safe(args.iter().map(OsString::from), cwd)
    }

    #[test]
    fn graph_root_prefers_home_directory() {
        let cwd = Path::new("/tmp/workspace");
        let home = Path::new("/tmp/home");
        assert_eq!(
            graph_root_from(Some(home), cwd),
            PathBuf::from("/tmp/home/.kg/graphs")
        );
        assert_eq!(
            graph_root_from(None, cwd),
            PathBuf::from("/tmp/workspace/.kg/graphs")
        );
    }

    #[test]
    fn get_renders_compact_symbolic_view() {
        let graph = fixture_graph();
        let node = graph.node_by_id("concept:refrigerator").expect("node");
        let rendered = output::render_node(&graph, node, false);
        assert!(rendered.contains("# concept:refrigerator | Lodowka"));
        assert!(rendered.contains("aka: Chlodziarka, Fridge"));
        assert!(rendered.contains("-> HAS | concept:cooling_chamber | Komora Chlodzenia"));
        assert!(rendered.contains("-> HAS | concept:temperature | Temperatura"));
    }

    #[test]
    fn help_lists_mvp_commands() {
        let help = Cli::try_parse_from(["kg", "--help"]).expect_err("help exits");
        let rendered = help.to_string();
        assert!(!rendered.contains("▓ ▄▄"));
        assert!(rendered.contains("create"));
        assert!(rendered.contains("list"));
        assert!(rendered.contains("feedback-log"));
        assert!(rendered.contains("fridge node"));
        assert!(rendered.contains("edge"));
        assert!(rendered.contains("quality"));
        assert!(rendered.contains("kg graph fridge stats"));
    }

    #[test]
    fn run_args_safe_returns_error_instead_of_exiting() {
        let dir = tempdir().expect("tempdir");
        let err = exec_safe(&["kg", "create"], dir.path()).expect_err("parse error");
        let rendered = err.to_string();
        assert!(rendered.contains("required arguments were not provided"));
        assert!(rendered.contains("<GRAPH_NAME>"));
    }

    #[test]
    fn colorize_cli_output_styles_key_lines() {
        let rendered = "? weather (1)\nscore: 1000\n# concept:rain | Rain [Concept]\n-> DEPENDS_ON | process:forecast | Forecast\n";
        let colored = colorize_cli_output(rendered);
        assert!(colored.contains("\x1b[1;33m? weather (1)\x1b[0m"));
        assert!(colored.contains("\x1b[1;35mscore: 1000\x1b[0m"));
        assert!(colored.contains("\x1b[1;36m# concept:rain | Rain [Concept]\x1b[0m"));
        assert!(colored.contains("\x1b[34m-> DEPENDS_ON | process:forecast | Forecast\x1b[0m"));
    }

    #[test]
    fn colorize_cli_output_leaves_json_unchanged() {
        let rendered = "{\n  \"nodes\": []\n}\n";
        assert_eq!(colorize_cli_output(rendered), rendered);
    }

    #[test]
    fn execute_clusters_sorts_by_relevance_then_size() {
        let mut graph = GraphFile::new("score");
        graph.nodes.push(Node {
            id: "@:cluster_0001".to_owned(),
            r#type: "@".to_owned(),
            name: "Cluster 1".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.nodes.push(Node {
            id: "@:cluster_0002".to_owned(),
            r#type: "@".to_owned(),
            name: "Cluster 2".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        for id in ["concept:a", "concept:b", "concept:c", "concept:d"] {
            graph.nodes.push(Node {
                id: id.to_owned(),
                r#type: "Concept".to_owned(),
                name: id.to_owned(),
                properties: NodeProperties::default(),
                source_files: vec![],
            });
        }
        graph.edges.push(Edge {
            source_id: "@:cluster_0001".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:a".to_owned(),
            properties: EdgeProperties {
                detail: "0.95".to_owned(),
                ..Default::default()
            },
        });
        graph.edges.push(Edge {
            source_id: "@:cluster_0001".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:b".to_owned(),
            properties: EdgeProperties {
                detail: "0.85".to_owned(),
                ..Default::default()
            },
        });
        graph.edges.push(Edge {
            source_id: "@:cluster_0002".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:c".to_owned(),
            properties: EdgeProperties {
                detail: "0.70".to_owned(),
                ..Default::default()
            },
        });
        graph.edges.push(Edge {
            source_id: "@:cluster_0002".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:d".to_owned(),
            properties: EdgeProperties {
                detail: "0.65".to_owned(),
                ..Default::default()
            },
        });

        let rendered = render_clusters(
            &graph,
            &ClustersArgs {
                limit: 10,
                json: false,
                skill: None,
            },
        );
        let first = rendered.find("@:cluster_0001").expect("cluster 1 present");
        let second = rendered.find("@:cluster_0002").expect("cluster 2 present");
        assert!(first < second);
    }

    #[test]
    fn execute_clusters_gardener_mode_emits_actions() {
        let mut graph = GraphFile::new("score");
        graph.nodes.push(Node {
            id: "@:cluster_0001".to_owned(),
            r#type: "@".to_owned(),
            name: "Cluster 1".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.nodes.push(Node {
            id: "concept:a".to_owned(),
            r#type: "Concept".to_owned(),
            name: "A".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![],
        });
        graph.edges.push(Edge {
            source_id: "@:cluster_0001".to_owned(),
            relation: "HAS".to_owned(),
            target_id: "concept:a".to_owned(),
            properties: EdgeProperties {
                detail: "0.9".to_owned(),
                ..Default::default()
            },
        });

        let rendered = render_clusters(
            &graph,
            &ClustersArgs {
                limit: 5,
                json: false,
                skill: Some(ClusterSkill::Gardener),
            },
        );
        assert!(rendered.contains("= gardener clusters"));
        assert!(rendered.contains("action: review cluster"));
    }

    #[test]
    fn find_latest_score_snapshot_picks_newest_timestamp() {
        let dir = tempdir().expect("tempdir");
        let graph_path = dir.path().join("fridge.kg");
        std::fs::write(&graph_path, "").expect("graph file");
        let cache_dir = crate::cache_paths::cache_root_for_graph(&graph_path);
        std::fs::create_dir_all(&cache_dir).expect("cache dir");
        let older = cache_dir.join("fridge.score.100.kg");
        let newer = cache_dir.join("fridge.score.200.kg");
        std::fs::write(&older, "").expect("older");
        std::fs::write(&newer, "").expect("newer");

        let latest = find_latest_score_snapshot(&graph_path)
            .expect("latest")
            .expect("some path");
        assert_eq!(latest, newer);
    }

    #[test]
    fn baseline_rejects_vector_mode() {
        let graph = fixture_graph();
        let err = eval_golden_set(
            &graph,
            &BaselineArgs {
                find_limit: 5,
                include_features: true,
                mode: CliFindMode::Vector,
                golden: None,
                json: false,
            },
        )
        .expect_err("vector mode should be rejected for baseline");
        assert!(
            err.to_string()
                .contains("baseline does not support --mode vector")
        );
    }
}
