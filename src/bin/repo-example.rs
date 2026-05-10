use std::path::PathBuf;

use anyhow::Result;
use kg::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("repo-example.kg"));

    let graph = build_graph();
    graph.save(&output)?;
    Ok(())
}

fn build_graph() -> GraphFile {
    let mut graph = GraphFile::new("repo-example");
    graph.metadata.description = "Auto-generated example graph for the kg repository".to_owned();
    graph.metadata.version = "1.0".to_owned();

    graph.nodes.push(node(
        "^:graph_info",
        "^",
        "Graph Metadata",
        "Internal graph metadata for cross-graph linking.",
        vec!["graph_uuid=0123456789abcdef0123", "schema_version=2"],
        vec!["DOC .kg/internal/graph_info"],
        1.0,
    ));
    graph.nodes.push(node(
        "K:graph_serialization",
        "Concept",
        "Graph serialization",
        "Native .kg serialization plus import/export paths.",
        vec![
            "`src/graph.rs` parses and serializes the line-based .kg format.",
            "`src/export_html.rs` renders graph data for visualization.",
        ],
        vec![
            "SOURCECODE src/export_html.rs",
            "SOURCECODE src/graph.rs",
            "SOURCECODE src/import_csv.rs",
            "SOURCECODE src/import_markdown.rs",
        ],
        4.0,
    ));
    graph.nodes.push(node(
        "K:kg_repo",
        "Concept",
        "kg repository",
        "Local knowledge-graph tooling for AI assistants.",
        vec!["Persistent project memory stored locally as readable .kg files."],
        vec!["DOC README.md", "DOC docs/build-graph-from-docs.md", "DOC docs/mcp.md"],
        5.0,
    ));
    graph.nodes.push(node(
        "K:repo_scope",
        "Concept",
        "Repository scope",
        "Compact graph slice covering the repo, not every file.",
        vec!["Grounded in the docs and code that describe repo workflows."],
        vec!["DOC README.md", "DOC docs/build-graph-from-docs.md"],
        4.0,
    ));
    graph.nodes.push(node(
        "F:persistent_memory",
        "Feature",
        "Persistent memory",
        "Stable project memory for AI assistants across sessions.",
        vec!["The README describes Git-friendly, structured memory."],
        vec!["DOC README.md"],
        5.0,
    ));
    graph.nodes.push(node(
        "I:kg_cli",
        "Interface",
        "kg CLI",
        "Command-line interface for creating, querying, and maintaining graphs.",
        vec![
            "`src/cli.rs` defines the main graph, node, edge, note, and export commands.",
            "`src/main.rs` delegates to `kg::run`.",
        ],
        vec!["SOURCECODE src/cli.rs", "SOURCECODE src/lib.rs", "SOURCECODE src/main.rs"],
        5.0,
    ));
    graph.nodes.push(node(
        "I:kg_mcp",
        "Interface",
        "kg-mcp",
        "Local stdio MCP server that exposes kg operations to AI clients.",
        vec![
            "README and `docs/mcp.md` document MCP client setup.",
            "`src/bin/kg-mcp.rs` implements the server entrypoint.",
        ],
        vec![
            "DOC README.md",
            "DOC docs/mcp.md",
            "SOURCECODE src/bin/kg-mcp.rs",
            "SOURCECODE src/lib.rs",
        ],
        5.0,
    ));
    graph.nodes.push(node(
        "P:command_dispatch",
        "Process",
        "Command dispatch",
        "Routing CLI arguments into graph operations.",
        vec!["`src/lib.rs` wires the CLI parser to execution handlers.", "`src/main.rs` handles exit paths."],
        vec!["SOURCECODE src/cli.rs", "SOURCECODE src/lib.rs", "SOURCECODE src/main.rs"],
        4.0,
    ));
    graph.nodes.push(node(
        "P:graph_query",
        "Process",
        "Graph query",
        "Search, list, KQL, and inspection workflows.",
        vec!["Query rendering lives in `src/output.rs`.", "`src/cli.rs` exposes `node find`, `node get`, `list`, and `kql`."],
        vec!["SOURCECODE src/cli.rs", "SOURCECODE src/lib.rs", "SOURCECODE src/output.rs"],
        5.0,
    ));
    graph.nodes.push(node(
        "P:graph_mutation",
        "Process",
        "Graph mutation",
        "Node, edge, and note updates.",
        vec![
            "`src/app/graph_node_edge.rs` and `src/app/graph_note.rs` implement mutation flows.",
            "`src/graph.rs` persists graph updates with schema migration.",
        ],
        vec![
            "SOURCECODE src/app/graph_node_edge.rs",
            "SOURCECODE src/app/graph_note.rs",
            "SOURCECODE src/graph.rs",
            "SOURCECODE src/lib.rs",
        ],
        5.0,
    ));
    graph.nodes.push(node(
        "P:quality_checks",
        "Process",
        "Quality checks",
        "Validation and quality reporting for graph content.",
        vec![
            "`src/validate.rs` defines allowed types, relations, and edge rules.",
            "`src/lib.rs` exposes check, audit, and quality commands.",
        ],
        vec!["SOURCECODE src/cli.rs", "SOURCECODE src/lib.rs", "SOURCECODE src/validate.rs"],
        5.0,
    ));
    graph.nodes.push(node(
        "C:config_discovery",
        "Convention",
        "Config discovery",
        "`.kg.toml` is discovered in the current directory and parents.",
        vec!["The config can define graph directories and defaults.", "README documents git-friendly local graph storage."],
        vec!["DOC README.md", "SOURCECODE src/config.rs"],
        4.0,
    ));
    graph.nodes.push(node(
        "D:local_graph_files",
        "DataStore",
        "Local graph files",
        "Native graph files stored locally as readable `.kg` documents.",
        vec!["The README recommends keeping `.kg` files in git.", "Graph loading supports `.kg` text plus legacy JSON fallback."],
        vec!["DOC README.md", "SOURCECODE src/graph.rs", "SOURCECODE src/storage.rs"],
        5.0,
    ));
    graph.nodes.push(node(
        "D:event_log_snapshots",
        "DataStore",
        "Event log snapshots",
        "Append-only event log support for graph mutations.",
        vec!["Mutating operations can be captured as snapshots.", "The repo includes feedback and log workflows around this store."],
        vec!["DOC README.md", "SOURCECODE src/event_log.rs", "SOURCECODE src/lib.rs"],
        3.0,
    ));
    graph.nodes.push(node(
        "Z:native_kg_format",
        "Decision",
        "Native .kg format",
        "The repo prefers readable .kg files over JSON-only storage.",
        vec!["README says .kg is git-friendly and diffable.", "`src/graph.rs` still accepts legacy JSON payloads for migration."],
        vec!["DOC README.md", "SOURCECODE src/graph.rs"],
        5.0,
    ));
    graph.nodes.push(node(
        "R:grounded_facts_only",
        "Rule",
        "Grounded facts only",
        "Add only facts supported by docs, code, or current discussion.",
        vec!["The build-graph docs explicitly prohibit speculation.", "Ambiguous items should become notes or be skipped."],
        vec!["DOC docs/ai-prompt-graph-from-docs.md", "DOC docs/build-graph-from-docs.md", "DOC README.md"],
        5.0,
    ));
    graph.nodes.push(node(
        "R:stable_ids",
        "Rule",
        "Stable IDs",
        "Use canonical `<type>:<snake_case_name>` identifiers from day one.",
        vec!["`src/validate.rs` maps node types to expected prefixes.", "The docs recommend one canonical ID per concept."],
        vec!["DOC docs/ai-prompt-graph-from-docs.md", "DOC docs/build-graph-from-docs.md", "SOURCECODE src/validate.rs"],
        5.0,
    ));

    graph.edges.extend([
        edge("^:graph_info", "USES", "K:kg_repo"),
        edge("K:graph_serialization", "DECIDED_BY", "Z:native_kg_format"),
        edge("K:kg_repo", "HAS", "F:persistent_memory"),
        edge("K:kg_repo", "HAS", "I:kg_cli"),
        edge("K:kg_repo", "HAS", "I:kg_mcp"),
        edge("K:kg_repo", "HAS", "K:graph_serialization"),
        edge("K:kg_repo", "HAS", "K:repo_scope"),
        edge("K:kg_repo", "AFFECTED_BY", "P:command_dispatch"),
        edge("K:kg_repo", "AFFECTED_BY", "P:graph_query"),
        edge("K:kg_repo", "AFFECTED_BY", "P:graph_mutation"),
        edge("K:kg_repo", "AFFECTED_BY", "P:quality_checks"),
        edge("I:kg_cli", "AFFECTED_BY", "P:command_dispatch"),
        edge("I:kg_mcp", "AFFECTED_BY", "P:graph_query"),
        edge("I:kg_mcp", "AFFECTED_BY", "P:graph_mutation"),
        edge("P:command_dispatch", "GOVERNED_BY", "C:config_discovery"),
        edge("P:graph_query", "READS_FROM", "D:local_graph_files"),
        edge("P:graph_mutation", "STORED_IN", "D:local_graph_files"),
        edge("P:graph_mutation", "AFFECTED_BY", "D:event_log_snapshots"),
        edge("P:graph_mutation", "GOVERNED_BY", "R:grounded_facts_only"),
        edge("P:quality_checks", "GOVERNED_BY", "R:stable_ids"),
    ]);

    graph
}

fn node(
    id: &str,
    node_type: &str,
    name: &str,
    description: &str,
    facts: Vec<&str>,
    sources: Vec<&str>,
    importance: f64,
) -> Node {
    Node {
        id: id.to_owned(),
        r#type: node_type.to_owned(),
        name: name.to_owned(),
        properties: NodeProperties {
            description: description.to_owned(),
            provenance: "A".to_owned(),
            importance,
            key_facts: facts.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        },
        source_files: sources.into_iter().map(str::to_owned).collect(),
    }
}

fn edge(source_id: &str, relation: &str, target_id: &str) -> Edge {
    Edge {
        source_id: source_id.to_owned(),
        relation: relation.to_owned(),
        target_id: target_id.to_owned(),
        properties: EdgeProperties::default(),
    }
}
