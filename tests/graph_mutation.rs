mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_fixture};

#[test]
fn add_persists_node_in_existing_graph() {
    let dir = temp_workspace();
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
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
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
    let dir = temp_workspace();
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
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
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
            .filter(|fact| fact.as_str() == "Alarm po 15 minutach odchylenia")
            .count(),
        1
    );
    assert!(
        node.source_files
            .iter()
            .any(|source| source == "panel_api.md")
    );
}

#[test]
fn remove_deletes_node_and_incident_edges() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(
        &["kg", "fridge", "node", "remove", "process:defrost"],
        dir.path(),
    );
    assert_eq!(output, "- node process:defrost (3 edges removed)\n");
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    assert!(graph.node_by_id("process:defrost").is_none());
    assert!(
        graph
            .edges
            .iter()
            .all(|edge| edge.source_id != "process:defrost" && edge.target_id != "process:defrost")
    );
}

#[test]
fn edge_add_persists_new_edge() {
    let dir = temp_workspace();
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
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    assert!(graph.has_edge(
        "concept:refrigerator",
        "READS_FROM",
        "datastore:settings_storage"
    ));
}

#[test]
fn edge_remove_deletes_existing_edge() {
    let dir = temp_workspace();
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
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    assert!(!graph.has_edge("concept:refrigerator", "HAS", "concept:temperature"));
}
