mod analysis;
mod cli;
mod config;
mod export_html;
mod graph;
mod ops;
mod output;
mod validate;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use cli::{
    AddEdgeArgs, AddNodeArgs, AuditArgs, CheckArgs, Cli, Command, DuplicatesArgs, EdgeCommand,
    EdgeGapsArgs, ExportHtmlArgs, GraphCommand, MissingDescriptionsArgs, MissingFactsArgs,
    ModifyNodeArgs, NodeCommand, QualityCommand, RemoveEdgeArgs, StatsArgs,
};
use config::KgConfig;
use graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};

use analysis::{
    render_duplicates, render_edge_gaps, render_missing_descriptions, render_missing_facts,
    render_stats,
};
use ops::{add_edge, add_node, modify_node, remove_edge, remove_node};
use validate::validate_graph;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn run<I>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = OsString>,
{
    let cli = Cli::parse_from(normalize_args(args));
    let graph_root = default_graph_root(cwd);
    let rendered = execute(cli, cwd, &graph_root)?;
    print!("{rendered}");
    Ok(())
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
    if first.starts_with('-') || first == "create" || first == "graph" {
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
    match cli.command {
        Command::Create { graph_name } => {
            let path = create_graph(graph_root, &graph_name)?;
            Ok(format!("+ created {}\n", path.display()))
        }
        Command::Graph { graph, command } => {
            let path = resolve_graph_path(cwd, graph_root, &graph)?;
            let mut graph_file = GraphFile::load(&path)?;

            match command {
                GraphCommand::Node { command } => match command {
                    NodeCommand::Find {
                        queries,
                        limit,
                        include_features,
                        full,
                    } => Ok(output::render_find(
                        &graph_file,
                        &queries,
                        limit,
                        include_features,
                        full,
                    )),

                    NodeCommand::Get {
                        id,
                        include_features,
                        full,
                    } => {
                        let node = graph_file
                            .node_by_id(&id)
                            .ok_or_else(|| anyhow!("node not found: {id}"))?;
                        if !include_features && node.r#type == "Feature" {
                            bail!("feature nodes are hidden by default; use --include-features");
                        }
                        Ok(output::render_node(&graph_file, node, full))
                    }

                    NodeCommand::Add(AddNodeArgs {
                        id,
                        node_type,
                        name,
                        description,
                        domain_area,
                        provenance,
                        confidence,
                        created_at,
                        fact,
                        alias,
                        source,
                    }) => {
                        add_node(
                            &mut graph_file,
                            Node {
                                id,
                                r#type: node_type,
                                name,
                                properties: NodeProperties {
                                    description,
                                    domain_area,
                                    provenance,
                                    confidence,
                                    created_at,
                                    key_facts: fact,
                                    alias,
                                },
                                source_files: source,
                            },
                        )?;
                        graph_file.save(&path)?;
                        Ok(format!(
                            "+ node {}\n",
                            graph_file.nodes.last().expect("node").id
                        ))
                    }

                    NodeCommand::Modify(ModifyNodeArgs {
                        id,
                        node_type,
                        name,
                        description,
                        domain_area,
                        provenance,
                        confidence,
                        created_at,
                        fact,
                        alias,
                        source,
                    }) => {
                        modify_node(
                            &mut graph_file,
                            &id,
                            node_type,
                            name,
                            description,
                            domain_area,
                            provenance,
                            confidence,
                            created_at,
                            fact,
                            alias,
                            source,
                        )?;
                        graph_file.save(&path)?;
                        Ok(format!("~ node {id}\n"))
                    }

                    NodeCommand::Remove { id } => {
                        let removed_edges = remove_node(&mut graph_file, &id)?;
                        graph_file.save(&path)?;
                        Ok(format!("- node {id} ({removed_edges} edges removed)\n"))
                    }
                },

                GraphCommand::Edge { command } => match command {
                    EdgeCommand::Add(AddEdgeArgs {
                        source_id,
                        relation,
                        target_id,
                        detail,
                    }) => {
                        add_edge(
                            &mut graph_file,
                            Edge {
                                source_id: source_id.clone(),
                                relation: relation.clone(),
                                target_id: target_id.clone(),
                                properties: EdgeProperties { detail },
                            },
                        )?;
                        graph_file.save(&path)?;
                        Ok(format!("+ edge {source_id} {relation} {target_id}\n"))
                    }
                    EdgeCommand::Remove(RemoveEdgeArgs {
                        source_id,
                        relation,
                        target_id,
                    }) => {
                        remove_edge(&mut graph_file, &source_id, &relation, &target_id)?;
                        graph_file.save(&path)?;
                        Ok(format!("- edge {source_id} {relation} {target_id}\n"))
                    }
                },

                GraphCommand::Stats(args) => Ok(render_stats(&graph_file, &args)),
                GraphCommand::Check(args) => Ok(render_check(&graph_file, cwd, &args)),
                GraphCommand::Audit(args) => Ok(render_audit(&graph_file, cwd, &args)),

                GraphCommand::Quality { command } => match command {
                    QualityCommand::MissingDescriptions(args) => {
                        Ok(render_missing_descriptions(&graph_file, &args))
                    }
                    QualityCommand::MissingFacts(args) => {
                        Ok(render_missing_facts(&graph_file, &args))
                    }
                    QualityCommand::Duplicates(args) => Ok(render_duplicates(&graph_file, &args)),
                    QualityCommand::EdgeGaps(args) => Ok(render_edge_gaps(&graph_file, &args)),
                },

                // Short aliases (e.g. `kg graph fridge missing-descriptions`)
                GraphCommand::MissingDescriptions(args) => {
                    Ok(render_missing_descriptions(&graph_file, &args))
                }
                GraphCommand::MissingFacts(args) => Ok(render_missing_facts(&graph_file, &args)),
                GraphCommand::Duplicates(args) => Ok(render_duplicates(&graph_file, &args)),
                GraphCommand::EdgeGaps(args) => Ok(render_edge_gaps(&graph_file, &args)),

                GraphCommand::ExportHtml(ExportHtmlArgs { output, title }) => {
                    export_html::export_html(
                        &graph_file,
                        &graph,
                        export_html::ExportHtmlOptions {
                            output: output.as_deref(),
                            title: title.as_deref(),
                        },
                    )
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Graph lifecycle helpers
// ---------------------------------------------------------------------------

fn default_graph_root(cwd: &Path) -> PathBuf {
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

fn create_graph(graph_root: &Path, graph_name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(graph_root)?;
    let path = graph_root.join(format!("{graph_name}.json"));
    if path.exists() {
        bail!("graph already exists: {}", path.display());
    }
    let graph = GraphFile::new(graph_name);
    graph.save(&path)?;
    Ok(path)
}

fn resolve_graph_path(cwd: &Path, graph_root: &Path, graph: &str) -> Result<PathBuf> {
    if let Some((config_path, config)) = KgConfig::discover(cwd)? {
        if let Some(path) = config.graph_path(&config_path, graph) {
            if path.exists() {
                return Ok(path);
            }
        }
        if let Some(config_graph_dir) = config.graph_dir(&config_path) {
            let direct = config_graph_dir.join(graph);
            let json = config_graph_dir.join(format!("{graph}.json"));
            if direct.exists() {
                return Ok(direct);
            }
            if json.exists() {
                return Ok(json);
            }
        }
    }

    let raw = PathBuf::from(graph);
    let candidates = [
        raw.clone(),
        cwd.join(graph),
        cwd.join(format!("{graph}.json")),
        graph_root.join(graph),
        graph_root.join(format!("{graph}.json")),
        cwd.join(format!("graph-example-{graph}.json")),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("graph not found: {graph}");
}

// ---------------------------------------------------------------------------
// Validation renderers (check vs audit differ in header only)
// ---------------------------------------------------------------------------

fn render_check(graph: &GraphFile, cwd: &Path, args: &CheckArgs) -> String {
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

fn render_audit(graph: &GraphFile, cwd: &Path, args: &AuditArgs) -> String {
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

    fn test_graph_root(cwd: &Path) -> PathBuf {
        cwd.join(".kg").join("graphs")
    }

    fn write_fixture(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).expect("create graph root");
        let path = dir.join("fridge.json");
        std::fs::write(&path, include_str!("../graph-example-fridge.json")).expect("write fixture");
        path
    }

    fn write_config(cwd: &Path, body: &str) {
        std::fs::write(cwd.join(".kg.toml"), body).expect("write config");
    }

    fn write_graph(path: &Path, graph: &GraphFile) {
        graph.save(path).expect("save graph");
    }

    fn exec_ok(args: &[&str], cwd: &Path) -> String {
        let cli = Cli::try_parse_from(normalize_args(args.iter().map(OsString::from)))
            .expect("parse args");
        execute(cli, cwd, &test_graph_root(cwd)).expect("execute")
    }

    #[test]
    fn create_graph_writes_empty_graph_file() {
        let dir = tempdir().expect("tempdir");
        let output = exec_ok(&["kg", "create", "fridge"], dir.path());
        assert!(output.contains("+ created"));
        assert!(output.contains(".kg/graphs/fridge.json"));
        let graph = GraphFile::load(&test_graph_root(dir.path()).join("fridge.json"))
            .expect("load created graph");
        assert_eq!(graph.metadata.name, "fridge");
        assert_eq!(graph.metadata.node_count, 0);
        assert!(graph.nodes.is_empty());
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
    fn find_supports_multiple_queries() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &["kg", "fridge", "node", "find", "lodowka", "smart"],
            dir.path(),
        );
        assert!(output.contains("? lodowka ("));
        assert!(output.contains("# concept:refrigerator | Lodowka"));
        assert!(output.contains("? smart ("));
        assert!(output.contains("# interface:smart_api | Smart Home API (REST)"));
    }

    #[test]
    fn find_uses_fuzzy_matching_for_imperfect_queries() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(&["kg", "fridge", "node", "find", "smrt api"], dir.path());
        assert!(output.contains("? smrt api ("));
        assert!(output.contains("# interface:smart_api | Smart Home API (REST)"));
        assert!(!output.contains("# process:diagnostics | Autodiagnostyka"));
    }

    #[test]
    fn graph_stats_reports_counts() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &[
                "kg",
                "graph",
                "fridge",
                "stats",
                "--by-type",
                "--by-relation",
                "--show-sources",
            ],
            dir.path(),
        );
        assert!(output.contains("= stats"));
        assert!(output.contains("nodes:"));
        assert!(output.contains("types:"));
        assert!(output.contains("relations:"));
        assert!(output.contains("sources:"));
    }

    #[test]
    fn graph_missing_descriptions_alias_works() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph
            .node_by_id_mut("concept:temperature")
            .expect("node")
            .properties
            .description
            .clear();
        write_graph(&path, &graph);
        let output = exec_ok(
            &[
                "kg",
                "graph",
                "fridge",
                "missing-descriptions",
                "--limit",
                "10",
            ],
            dir.path(),
        );
        assert!(output.contains("= missing-descriptions ("));
        assert!(output.contains("concept:temperature"));
    }

    #[test]
    fn graph_missing_facts_quality_command_works() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph
            .node_by_id_mut("process:defrost")
            .expect("node")
            .properties
            .key_facts
            .clear();
        write_graph(&path, &graph);
        let output = exec_ok(
            &[
                "kg",
                "graph",
                "fridge",
                "quality",
                "missing-facts",
                "--limit",
                "10",
            ],
            dir.path(),
        );
        assert!(output.contains("= missing-facts ("));
        assert!(output.contains("process:defrost"));
    }

    #[test]
    fn graph_duplicates_detects_similar_names() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph.nodes.push(Node {
            id: "concept:smart_home_api".to_owned(),
            r#type: "Interface".to_owned(),
            name: "Smart Home API".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec!["smart_home_integration.md".to_owned()],
        });
        graph.refresh_counts();
        write_graph(&path, &graph);
        let output = exec_ok(
            &[
                "kg",
                "graph",
                "fridge",
                "quality",
                "duplicates",
                "--threshold",
                "0.7",
            ],
            dir.path(),
        );
        assert!(output.contains("= duplicates ("));
        assert!(output.contains("interface:smart_api"));
        assert!(output.contains("concept:smart_home_api"));
    }

    #[test]
    fn graph_edge_gaps_reports_structural_gaps() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph.nodes.push(Node {
            id: "datastore:manual_cache".to_owned(),
            r#type: "DataStore".to_owned(),
            name: "Manual Cache".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec!["manual.md".to_owned()],
        });
        graph.nodes.push(Node {
            id: "process:manual_sync".to_owned(),
            r#type: "Process".to_owned(),
            name: "Manual Sync".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec!["manual.md".to_owned()],
        });
        graph.refresh_counts();
        write_graph(&path, &graph);
        let output = exec_ok(&["kg", "graph", "fridge", "edge-gaps"], dir.path());
        assert!(output.contains("datastore-missing-stored-in:"));
        assert!(output.contains("datastore:manual_cache"));
        assert!(output.contains("process:manual_sync"));
    }

    #[test]
    fn graph_audit_reports_invalid_conditions() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph.nodes.push(Node {
            id: "concept:refrigerator".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Duplicate Refrigerator".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec!["missing.md".to_owned()],
        });
        graph.refresh_counts();
        write_graph(&path, &graph);
        let output = exec_ok(
            &["kg", "graph", "fridge", "audit", "--deep", "--limit", "20"],
            dir.path(),
        );
        assert!(output.contains("= audit"));
        assert!(output.contains("status: INVALID"));
        assert!(output.contains("duplicate node id: concept:refrigerator"));
        assert!(output.contains("missing source file:"));
    }

    #[test]
    fn graph_check_reports_validation_errors() {
        let dir = tempdir().expect("tempdir");
        let path = write_fixture(&test_graph_root(dir.path()));
        let mut graph = GraphFile::load(&path).expect("load graph");
        graph.nodes.push(Node {
            id: "bad-id".to_owned(),
            r#type: "WeirdType".to_owned(),
            name: String::new(),
            properties: NodeProperties {
                confidence: Some(1.5),
                ..NodeProperties::default()
            },
            source_files: Vec::new(),
        });
        graph.refresh_counts();
        write_graph(&path, &graph);
        let output = exec_ok(
            &["kg", "graph", "fridge", "check", "--limit", "20"],
            dir.path(),
        );
        assert!(output.contains("= check"));
        assert!(output.contains("status: INVALID"));
        assert!(output.contains("node bad-id has invalid type WeirdType"));
        assert!(output.contains("node id bad-id does not match prefix:snake_case"));
        assert!(output.contains("node bad-id missing name"));
        assert!(output.contains("node bad-id missing source_files"));
        assert!(output.contains("confidence out of range"));
    }

    #[test]
    fn resolve_graph_path_uses_config_mapping() {
        let dir = tempdir().expect("tempdir");
        let mapped_dir = dir.path().join("mapped");
        write_fixture(&mapped_dir);
        write_config(dir.path(), "[graphs]\nfridge = \"mapped/fridge.json\"\n");
        let output = exec_ok(
            &["kg", "fridge", "node", "get", "concept:refrigerator"],
            dir.path(),
        );
        assert!(output.contains("# concept:refrigerator | Lodowka"));
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
    fn add_persists_node_in_existing_graph() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "node",
                "add",
                "concept:ice_maker",
                "--type",
                "Concept",
                "--name",
                "Kostkarka",
                "--description",
                "Automatyczna kostkarka do lodu",
                "--domain-area",
                "hardware",
                "--provenance",
                "manual",
                "--confidence",
                "0.9",
                "--created-at",
                "2026-03-20T01:00:00Z",
                "--fact",
                "Wytwarza kostki lodu co 2 godziny",
                "--alias",
                "Ice Maker",
                "--source",
                "instrukcja_obslugi.md",
            ],
            dir.path(),
        );
        assert_eq!(output, "+ node concept:ice_maker\n");
        let graph =
            GraphFile::load(&test_graph_root(dir.path()).join("fridge.json")).expect("load graph");
        let node = graph.node_by_id("concept:ice_maker").expect("new node");
        assert_eq!(node.properties.alias, vec!["Ice Maker"]);
        assert_eq!(node.properties.domain_area, "hardware");
        assert_eq!(node.properties.provenance, "manual");
        assert_eq!(node.properties.confidence, Some(0.9));
        assert_eq!(node.properties.created_at, "2026-03-20T01:00:00Z");
        assert_eq!(graph.metadata.node_count, graph.nodes.len());
    }

    #[test]
    fn modify_updates_existing_node_without_duplicate_values() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "node",
                "modify",
                "concept:temperature",
                "--name",
                "Temperatura Komory",
                "--domain-area",
                "sensing",
                "--provenance",
                "service_manual",
                "--confidence",
                "0.75",
                "--created-at",
                "2026-03-20T01:05:00Z",
                "--fact",
                "Alarm po 15 minutach odchylenia",
                "--fact",
                "Alarm po 15 minutach odchylenia",
                "--alias",
                "Temp",
                "--alias",
                "Temp",
                "--source",
                "panel_api.md",
            ],
            dir.path(),
        );
        assert_eq!(output, "~ node concept:temperature\n");
        let graph =
            GraphFile::load(&test_graph_root(dir.path()).join("fridge.json")).expect("load graph");
        let node = graph.node_by_id("concept:temperature").expect("node");
        assert_eq!(node.name, "Temperatura Komory");
        assert_eq!(node.properties.alias, vec!["Temp"]);
        assert_eq!(node.properties.domain_area, "sensing");
        assert_eq!(node.properties.provenance, "service_manual");
        assert_eq!(node.properties.confidence, Some(0.75));
        assert_eq!(node.properties.created_at, "2026-03-20T01:05:00Z");
        assert_eq!(
            node.properties
                .key_facts
                .iter()
                .filter(|f| f.as_str() == "Alarm po 15 minutach odchylenia")
                .count(),
            1
        );
        assert!(node.source_files.iter().any(|s| s == "panel_api.md"));
    }

    #[test]
    fn remove_deletes_node_and_incident_edges() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &["kg", "fridge", "node", "remove", "process:defrost"],
            dir.path(),
        );
        assert_eq!(output, "- node process:defrost (3 edges removed)\n");
        let graph =
            GraphFile::load(&test_graph_root(dir.path()).join("fridge.json")).expect("load graph");
        assert!(graph.node_by_id("process:defrost").is_none());
        assert!(graph
            .edges
            .iter()
            .all(|e| e.source_id != "process:defrost" && e.target_id != "process:defrost"));
    }

    #[test]
    fn edge_add_persists_new_edge() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "edge",
                "add",
                "concept:refrigerator",
                "READS_FROM",
                "datastore:settings_storage",
                "--detail",
                "Lodowka odczytuje ustawienia z pamieci ustawien",
            ],
            dir.path(),
        );
        assert_eq!(
            output,
            "+ edge concept:refrigerator READS_FROM datastore:settings_storage\n"
        );
        let graph =
            GraphFile::load(&test_graph_root(dir.path()).join("fridge.json")).expect("load graph");
        assert!(graph.has_edge(
            "concept:refrigerator",
            "READS_FROM",
            "datastore:settings_storage"
        ));
    }

    #[test]
    fn edge_remove_deletes_existing_edge() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "edge",
                "remove",
                "concept:refrigerator",
                "HAS",
                "concept:temperature",
            ],
            dir.path(),
        );
        assert_eq!(
            output,
            "- edge concept:refrigerator HAS concept:temperature\n"
        );
        let graph =
            GraphFile::load(&test_graph_root(dir.path()).join("fridge.json")).expect("load graph");
        assert!(!graph.has_edge("concept:refrigerator", "HAS", "concept:temperature"));
    }

    #[test]
    fn help_lists_mvp_commands() {
        let help = Cli::try_parse_from(["kg", "--help"]).expect_err("help exits");
        let rendered = help.to_string();
        assert!(rendered.contains("create"));
        assert!(rendered.contains("fridge node"));
        assert!(rendered.contains("edge"));
        assert!(rendered.contains("quality"));
        assert!(rendered.contains("kg graph fridge stats"));
    }

    #[test]
    fn get_full_renders_new_properties() {
        let dir = tempdir().expect("tempdir");
        write_fixture(&test_graph_root(dir.path()));
        exec_ok(
            &[
                "kg",
                "fridge",
                "node",
                "modify",
                "concept:refrigerator",
                "--domain-area",
                "appliance",
                "--provenance",
                "user_import",
                "--confidence",
                "0.88",
                "--created-at",
                "2026-03-20T01:10:00Z",
            ],
            dir.path(),
        );
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "node",
                "get",
                "concept:refrigerator",
                "--full",
            ],
            dir.path(),
        );
        assert!(output.contains("domain_area: appliance"));
        assert!(output.contains("provenance: user_import"));
        assert!(output.contains("confidence: 0.88"));
        assert!(output.contains("created_at: 2026-03-20T01:10:00Z"));
    }
}
