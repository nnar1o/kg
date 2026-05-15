#![allow(clippy::field_reassign_with_default)]

use std::path::PathBuf;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;

use kg::{Bm25Index, GraphFile, Node, NodeProperties};

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn make_graph(name: &str, nodes: usize, edges_per_node: usize) -> GraphFile {
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

    for i in 0..nodes {
        let t = types[i % types.len()];
        let id = format!("{}:{i}", t.to_lowercase());
        let name = format!("{t} {i}");

        let mut props = NodeProperties::default();
        props.description = format!("{t} {i} provides query cache storage index");
        props.domain_area = if i % 2 == 0 { "core" } else { "infra" }.to_string();
        props.provenance = "synthetic".to_string();
        props.created_at = "2026-01-01".to_string();
        props.key_facts = vec![format!("fact: node {i} uses index")];
        props.alias = vec![format!("alias-{i}"), format!("alias-{t}-{i}")];

        graph.nodes.push(Node {
            id: id.clone(),
            r#type: t.to_string(),
            name,
            properties: props,
            source_files: Vec::new(),
        });
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

fn bench_persistence(c: &mut Criterion) {
    let nodes = env_usize("KG_BENCH_NODES", 20_000);
    let edges_per_node = env_usize("KG_BENCH_EDGES_PER_NODE", 5);

    let graph = make_graph("bench", nodes, edges_per_node);

    let mut group = c.benchmark_group("persistence");
    group.measurement_time(Duration::from_secs(8));
    group.warm_up_time(Duration::from_secs(2));

    group.bench_with_input(BenchmarkId::new("bm25_build", nodes), &graph, |b, g| {
        b.iter(|| {
            let idx = Bm25Index::build(black_box(g));
            black_box(idx.doc_count);
        })
    });

    let idx = Bm25Index::build(&graph);
    let dir = TempDir::new().expect("tempdir");
    let base_path = dir.path().join("bm25.index.db");
    idx.save(&base_path).expect("save index");

    group.bench_with_input(
        BenchmarkId::new("bm25_load", nodes),
        &base_path,
        |b, path| {
            let path = path.to_owned();
            b.iter(|| {
                let loaded = Bm25Index::load(black_box(&path)).expect("load index");
                black_box(loaded.avg_doc_len);
            })
        },
    );

    group.bench_function(BenchmarkId::new("bm25_save_fresh", nodes), |b| {
        let idx = idx.clone();
        b.iter_batched(
            || {
                let tmp = TempDir::new().expect("tempdir");
                let path: PathBuf = tmp.path().join("bm25.index.db");
                (tmp, path)
            },
            |(_tmp, path)| {
                idx.save(black_box(&path)).expect("save index");
                black_box(std::fs::metadata(&path).expect("stat").len());
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_persistence);
criterion_main!(benches);
