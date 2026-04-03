mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_fixture, write_graph};
use kg::{Edge, Node, NodeProperties};

#[test]
fn graph_stats_reports_counts() {
    let dir = temp_workspace();
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
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
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
    graph.nodes.push(Node {
        id: "bad-id".to_owned(),
        r#type: "WeirdType".to_owned(),
        name: String::new(),
        properties: NodeProperties {
            confidence: Some(1.5),
            importance: 9,
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
    assert!(output.contains("node id bad-id does not match prefix:snake_case"));
    assert!(output.contains("node bad-id missing name"));
    assert!(output.contains("node bad-id missing source_files"));
    assert!(output.contains("confidence out of range"));
    assert!(output.contains("importance out of range"));
}

#[test]
fn graph_check_reports_relation_semantic_type_mismatch() {
    let dir = temp_workspace();
    let path = write_fixture(&test_graph_root(dir.path()));
    let mut graph = load_graph(&path);
    graph.edges.push(Edge {
        source_id: "datastore:settings_storage".to_owned(),
        relation: "HAS".to_owned(),
        target_id: "process:cooling".to_owned(),
        properties: Default::default(),
    });
    graph.refresh_counts();
    write_graph(&path, &graph);

    let output = exec_ok(
        &["kg", "graph", "fridge", "check", "--limit", "50"],
        dir.path(),
    );
    assert!(output.contains("status: INVALID"));
    assert!(output.contains("edge source type invalid for relation"));
    assert!(output.contains("edge target type invalid for relation"));
    assert!(output.contains("datastore:settings_storage HAS process:cooling"));
}
