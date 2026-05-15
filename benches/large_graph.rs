#![allow(clippy::field_reassign_with_default)]

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use kg::{Bm25Index, GraphFile, Node, NodeProperties};

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
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
        // Intentionally repetitive terms (cache/index/storage/query) to exercise search.
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
                body: format!("note for {i} mentions cache index query storage"),
                tags: vec!["bench".to_string(), "synthetic".to_string()],
                author: "bench".to_string(),
                created_at: "2026-01-01".to_string(),
                provenance: "synthetic".to_string(),
                source_files: Vec::new(),
            });
        }
    }

    // Deterministic, moderately dense directed edges.
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

fn bench_large_graph(c: &mut Criterion) {
    let nodes = env_usize("KG_BENCH_NODES", 20_000);
    let edges_per_node = env_usize("KG_BENCH_EDGES_PER_NODE", 5);
    let notes_per_node = env_usize("KG_BENCH_NOTES_PER_NODE", 0);
    let limit = env_usize("KG_BENCH_LIMIT", 50);

    let graph = make_graph("bench", nodes, edges_per_node, notes_per_node);
    let queries = vec![
        "cache".to_string(),
        "storage index".to_string(),
        "concept:19999".to_string(),
    ];

    let mut group = c.benchmark_group("large_graph");
    group.measurement_time(Duration::from_secs(5));
    group.warm_up_time(Duration::from_secs(1));

    group.bench_with_input(
        BenchmarkId::new("serialize_pretty", nodes),
        &graph,
        |b, g| {
            b.iter(|| {
                let s = serde_json::to_string_pretty(black_box(g)).unwrap();
                black_box(s.len());
            })
        },
    );

    let raw = serde_json::to_string(black_box(&graph)).unwrap();
    group.bench_with_input(BenchmarkId::new("parse_json", nodes), &raw, |b, s| {
        b.iter(|| {
            let parsed: GraphFile = serde_json::from_str(black_box(s)).unwrap();
            black_box(parsed.metadata.node_count);
        })
    });

    group.bench_with_input(
        BenchmarkId::new("node_by_id_worst_case", nodes),
        &graph,
        |b, g| {
            let id = format!("decision:{}", nodes.saturating_sub(1));
            b.iter(|| {
                let node = black_box(g).node_by_id(black_box(&id));
                black_box(node.map(|n| n.id.as_str()));
            })
        },
    );

    group.bench_with_input(BenchmarkId::new("find_fuzzy", nodes), &graph, |b, g| {
        b.iter(|| {
            let count = kg::output::count_find_results_with_index(
                black_box(g),
                black_box(&queries),
                limit,
                true,
                false,
                kg::output::FindMode::Fuzzy,
                None,
            );
            black_box(count);
        })
    });

    // BM25 without a persisted index is intentionally expensive (tokenizes every node document).
    // Keep sizes modest or override via env vars.
    group.bench_with_input(
        BenchmarkId::new("find_bm25_no_index", nodes),
        &graph,
        |b, g| {
            b.iter(|| {
                let count = kg::output::count_find_results_with_index(
                    black_box(g),
                    black_box(&queries),
                    limit,
                    true,
                    false,
                    kg::output::FindMode::Bm25,
                    None,
                );
                black_box(count);
            })
        },
    );

    let index = Bm25Index::build(&graph);
    group.bench_with_input(
        BenchmarkId::new("find_bm25_with_index", nodes),
        &graph,
        |b, g| {
            b.iter(|| {
                let count = kg::output::count_find_results_with_index(
                    black_box(g),
                    black_box(&queries),
                    limit,
                    true,
                    false,
                    kg::output::FindMode::Bm25,
                    Some(black_box(&index)),
                );
                black_box(count);
            })
        },
    );

    group.finish();
}

criterion_group!(benches, bench_large_graph);
criterion_main!(benches);
