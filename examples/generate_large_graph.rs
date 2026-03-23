use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use kg::{Edge, EdgeProperties, GraphFile, Node, NodeProperties, Note};

#[derive(Debug, Parser)]
#[command(about = "Generate a synthetic large graph JSON file for testing")]
struct Args {
    /// Graph name stored in metadata
    #[arg(long, default_value = "synthetic")]
    name: String,

    /// Output path (e.g. ./big.json)
    #[arg(long)]
    out: PathBuf,

    /// Number of nodes
    #[arg(long, default_value_t = 100_000)]
    nodes: usize,

    /// Outgoing edges per node
    #[arg(long, default_value_t = 5)]
    edges_per_node: usize,

    /// Notes per node (increases BM25 document size)
    #[arg(long, default_value_t = 0)]
    notes_per_node: usize,
}

fn make_graph(name: &str, nodes: usize, edges_per_node: usize, notes_per_node: usize) -> GraphFile {
    let mut graph = GraphFile::new(name);

    let types = [
        "Concept",
        "Process",
        "DataStore",
        "Interface",
        "Rule",
        "Feature",
        "Decision",
    ];
    let relations = ["HAS", "DEPENDS_ON", "USES", "READS_FROM", "AVAILABLE_IN"];

    graph.nodes.reserve(nodes);
    graph.edges.reserve(nodes.saturating_mul(edges_per_node));
    graph.notes.reserve(nodes.saturating_mul(notes_per_node));

    for i in 0..nodes {
        let t = types[i % types.len()];
        let id = format!("{}:{i}", t.to_lowercase());
        let name = format!("{t} {i}");

        let mut props = NodeProperties::default();
        props.description = format!(
            "{t} {i} provides query and cache behavior; uses storage index; part of system alpha beta"
        );
        props.domain_area = if i % 2 == 0 { "core" } else { "infra" }.to_string();
        props.provenance = "synthetic".to_string();
        props.created_at = "2026-01-01".to_string();
        props.key_facts = vec![
            format!("fact: node {i} touches cache"),
            format!("fact: node {i} uses index"),
            format!("fact: node {i} reads storage"),
        ];
        props.alias = vec![format!("alias-{i}"), format!("alias-{t}-{i}")];

        graph.nodes.push(Node {
            id: id.clone(),
            r#type: t.to_string(),
            name,
            properties: props,
            source_files: Vec::new(),
        });

        for n in 0..notes_per_node {
            graph.notes.push(Note {
                id: format!("note:{i}:{n}"),
                node_id: id.clone(),
                body: format!("note for {i} mentions cache index query storage"),
                tags: vec!["synthetic".to_string()],
                author: "generator".to_string(),
                created_at: "2026-01-01".to_string(),
                provenance: "synthetic".to_string(),
                source_files: Vec::new(),
            });
        }
    }

    for i in 0..nodes {
        let src_type = types[i % types.len()];
        let src = format!("{}:{i}", src_type.to_lowercase());
        for e in 0..edges_per_node {
            let rel = relations[(i + e) % relations.len()];
            let offset = 1 + (e * 97) % nodes.max(1);
            let j = (i + offset) % nodes.max(1);
            let tgt_type = types[j % types.len()];
            let tgt = format!("{}:{j}", tgt_type.to_lowercase());
            if src != tgt {
                graph.edges.push(Edge {
                    source_id: src.clone(),
                    relation: rel.to_string(),
                    target_id: tgt,
                    properties: EdgeProperties::default(),
                });
            }
        }
    }

    graph.refresh_counts();
    graph
}

fn main() -> Result<()> {
    let args = Args::parse();
    let graph = make_graph(
        &args.name,
        args.nodes,
        args.edges_per_node,
        args.notes_per_node,
    );
    graph.save(&args.out)?;
    eprintln!(
        "wrote {} nodes, {} edges to {}",
        graph.metadata.node_count,
        graph.metadata.edge_count,
        args.out.display()
    );
    Ok(())
}
