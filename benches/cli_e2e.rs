#![allow(clippy::field_reassign_with_default)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;

use kg::{Bm25Index, GraphFile, Node, NodeProperties};

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn kg_bin() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Some(p) = std::env::var_os("CARGO_BIN_EXE_kg") {
            return PathBuf::from(p);
        }
        assert_cmd::cargo::cargo_bin("kg")
    })
    .as_path()
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
            graph.notes.push(kg::Note {
                id: format!("note:{i}:{n}"),
                node_id: id.clone(),
                body: "note mentions cache index query storage".to_string(),
                tags: vec!["bench".to_string(), "synthetic".to_string()],
                author: "bench".to_string(),
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
                graph.edges.push(kg::Edge {
                    source_id: src.clone(),
                    relation: rel.to_string(),
                    target_id: tgt,
                    properties: kg::EdgeProperties::default(),
                });
            }
        }
    }

    graph.refresh_counts();
    graph
}

fn write_json(path: &Path, graph: &GraphFile) {
    let raw = serde_json::to_string_pretty(graph).expect("serialize graph");
    std::fs::write(path, raw.as_bytes()).expect("write graph json");
}

fn run_kg(cwd: &Path, args: &[OsString]) -> Vec<u8> {
    let out = Command::new(kg_bin())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run kg");
    assert!(out.status.success(), "kg failed: status={}", out.status);
    out.stdout
}

fn bench_cli_e2e(c: &mut Criterion) {
    let nodes = env_usize("KG_BENCH_NODES", 20_000);
    let edges_per_node = env_usize("KG_BENCH_EDGES_PER_NODE", 5);
    let notes_per_node = env_usize("KG_BENCH_NOTES_PER_NODE", 0);

    let graph = make_graph("bench", nodes, edges_per_node, notes_per_node);

    // ---------------------------------------------------------------------
    // JSON backend (graph path points to a .json file)
    // ---------------------------------------------------------------------
    let json_dir = TempDir::new().expect("tempdir");
    let graph_no_index = json_dir.path().join("graph_no_index.json");
    let graph_with_index = json_dir.path().join("graph_with_index.json");

    write_json(&graph_no_index, &graph);
    write_json(&graph_with_index, &graph);

    // Create a persisted BM25 index for the 'with_index' variant.
    let index = Bm25Index::build(&graph);
    let index_path = graph_with_index.with_extension("index.db");
    index.save(&index_path).expect("save index");

    // ---------------------------------------------------------------------
    // redb backend (requires .kg.toml backend = "redb")
    // ---------------------------------------------------------------------
    let redb_dir = TempDir::new().expect("tempdir");
    std::fs::write(redb_dir.path().join(".kg.toml"), "backend = \"redb\"\n")
        .expect("write .kg.toml");

    let import_json = redb_dir.path().join("import.json");
    write_json(&import_json, &graph);

    // Create and import into a redb graph named 'g'.
    black_box(run_kg(
        redb_dir.path(),
        &[OsString::from("create"), OsString::from("g")],
    ));
    black_box(run_kg(
        redb_dir.path(),
        &[
            OsString::from("g"),
            OsString::from("import-json"),
            OsString::from("--input"),
            import_json.into_os_string(),
        ],
    ));

    let mut group = c.benchmark_group("cli_e2e");
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(2));

    group.bench_with_input(
        BenchmarkId::new("json_find_fuzzy", nodes),
        &graph_no_index,
        |b, path| {
            let cwd = json_dir.path();
            let path = path.to_owned();
            b.iter(|| {
                let out = run_kg(
                    cwd,
                    &[
                        OsString::from("graph"),
                        path.clone().into_os_string(),
                        OsString::from("node"),
                        OsString::from("find"),
                        OsString::from("cache"),
                        OsString::from("--mode"),
                        OsString::from("fuzzy"),
                        OsString::from("--limit"),
                        OsString::from("5"),
                        OsString::from("--json"),
                    ],
                );
                black_box(out.len());
            })
        },
    );

    group.bench_with_input(
        BenchmarkId::new("json_find_bm25_no_index", nodes),
        &graph_no_index,
        |b, path| {
            let cwd = json_dir.path();
            let path = path.to_owned();
            b.iter(|| {
                let out = run_kg(
                    cwd,
                    &[
                        OsString::from("graph"),
                        path.clone().into_os_string(),
                        OsString::from("node"),
                        OsString::from("find"),
                        OsString::from("cache"),
                        OsString::from("--mode"),
                        OsString::from("bm25"),
                        OsString::from("--limit"),
                        OsString::from("5"),
                        OsString::from("--json"),
                    ],
                );
                black_box(out.len());
            })
        },
    );

    group.bench_with_input(
        BenchmarkId::new("json_find_bm25_with_index", nodes),
        &graph_with_index,
        |b, path| {
            let cwd = json_dir.path();
            let path = path.to_owned();
            b.iter(|| {
                let out = run_kg(
                    cwd,
                    &[
                        OsString::from("graph"),
                        path.clone().into_os_string(),
                        OsString::from("node"),
                        OsString::from("find"),
                        OsString::from("cache"),
                        OsString::from("--mode"),
                        OsString::from("bm25"),
                        OsString::from("--limit"),
                        OsString::from("5"),
                        OsString::from("--json"),
                    ],
                );
                black_box(out.len());
            })
        },
    );

    group.bench_function(BenchmarkId::new("redb_find_fuzzy", nodes), |b| {
        let cwd = redb_dir.path();
        b.iter(|| {
            let out = run_kg(
                cwd,
                &[
                    OsString::from("g"),
                    OsString::from("node"),
                    OsString::from("find"),
                    OsString::from("cache"),
                    OsString::from("--mode"),
                    OsString::from("fuzzy"),
                    OsString::from("--limit"),
                    OsString::from("5"),
                    OsString::from("--json"),
                ],
            );
            black_box(out.len());
        })
    });

    group.bench_function(BenchmarkId::new("redb_find_bm25_with_index", nodes), |b| {
        let cwd = redb_dir.path();
        b.iter(|| {
            let out = run_kg(
                cwd,
                &[
                    OsString::from("g"),
                    OsString::from("node"),
                    OsString::from("find"),
                    OsString::from("cache"),
                    OsString::from("--mode"),
                    OsString::from("bm25"),
                    OsString::from("--limit"),
                    OsString::from("5"),
                    OsString::from("--json"),
                ],
            );
            black_box(out.len());
        })
    });

    group.finish();
}

criterion_group!(benches, bench_cli_e2e);
criterion_main!(benches);
