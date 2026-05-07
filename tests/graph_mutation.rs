mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_fixture, write_graph};
use kg::{Node, NodeProperties};

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
            "D",
            "--confidence",
            "0.75",
            "--created-at",
            "2026-03-20T01:05:00Z",
            "--importance",
            "0.95",
            "--fact",
            "Alarm po 15 minutach odchylenia",
            "--fact",
            "Alarm po 15 minutach odchylenia",
            "--alias",
            "Temp",
            "--alias",
            "Temp",
            "--source",
            "DOC panel_api.md",
        ],
        dir.path(),
    );
    // Output should show the changes (actual behavior)
    assert!(output.contains("~ node concept:temperature"));
    assert!(output.contains("name: Temperatura Komory"));
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    let node = graph.node_by_id("concept:temperature").expect("node");
    assert_eq!(node.name, "Temperatura Komory");
    assert_eq!(node.properties.alias, vec!["Temp"]);
    assert_eq!(node.properties.domain_area, "sensing");
    assert_eq!(node.properties.provenance, "D");
    assert_eq!(node.properties.confidence, Some(0.75));
    assert_eq!(node.properties.created_at, "2026-03-20T01:05:00Z");
    assert_eq!(node.properties.importance, 0.95);
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
            .any(|source| source == "DOC panel_api.md")
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
fn edge_add_allows_rule_affected_by_bug() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "edge",
            "add",
            "rule:defrost_schedule_rule",
            "AFFECTED_BY",
            "bug:defrost_sensor_false_trigger",
            "--detail",
            "Regula rozmrazania wymaga poprawki po falszywym alarmie czujnika",
        ],
        dir.path(),
    );
    assert_eq!(
        output,
        "+ edge rule:defrost_schedule_rule AFFECTED_BY bug:defrost_sensor_false_trigger\n"
    );
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    assert!(graph.has_edge(
        "rule:defrost_schedule_rule",
        "AFFECTED_BY",
        "bug:defrost_sensor_false_trigger"
    ));
}

#[test]
fn edge_add_allows_process_available_in_interface() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "edge",
            "add",
            "process:defrost",
            "AVAILABLE_IN",
            "interface:smart_api",
            "--detail",
            "Proces rozmrazania mozna uruchomic zdalnie przez API serwisowe",
        ],
        dir.path(),
    );
    assert_eq!(
        output,
        "+ edge process:defrost AVAILABLE_IN interface:smart_api\n"
    );
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.json"));
    assert!(graph.has_edge("process:defrost", "AVAILABLE_IN", "interface:smart_api"));
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

#[test]
fn auto_update_roundtrips_generated_node_types() {
    let dir = temp_workspace();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    std::fs::write(dir.path().join("src/main.rs"), b"fn main() {}").expect("write main.rs");
    std::fs::write(dir.path().join("src/lib.rs"), b"pub fn lib() {}").expect("write lib.rs");
    std::fs::create_dir_all(dir.path().join("src/utils")).expect("create utils dir");
    std::fs::write(dir.path().join("src/utils/helper.rs"), b"pub fn help() {}")
        .expect("write helper.rs");

    exec_ok(&["kg", "create", "project"], dir.path());

    let src_path_buf = dir.path().join("src");
    let src_path = src_path_buf.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(Node {
        id: "D:src".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", src_path)],
    });
    write_graph(&graph_path, &graph);
    assert!(graph.node_by_id("D:src").is_some());

    let output = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(output.contains("nodes_added: 4"));

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    assert!(graph.node_by_id("D:src").is_some());
    assert!(graph.node_by_id("utils").is_some());
    assert!(graph.node_by_id("main.rs").is_some());
    assert!(graph.node_by_id("lib.rs").is_some());

    assert!(graph.has_edge("D:src", "GHAS", "main.rs"));
    assert!(graph.has_edge("D:src", "GHAS", "lib.rs"));
    assert!(graph.has_edge("D:src", "GHAS", "utils"));
    assert!(graph.has_edge("utils", "GHAS", "utils/helper.rs"));
}

#[test]
fn auto_update_is_idempotent() {
    let dir = temp_workspace();
    std::fs::create_dir_all(dir.path().join("data")).expect("create data dir");
    std::fs::write(dir.path().join("data/file.txt"), b"content").expect("write file");

    exec_ok(&["kg", "create", "project"], dir.path());

    let data_path = dir.path().join("data");
    let data_path_str = data_path.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(Node {
        id: "D:data".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", data_path_str)],
    });
    write_graph(&graph_path, &graph);

    let first = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(first.contains("nodes_added: 1"));

    let second = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(second.contains("nodes_added: 0"));
    assert!(second.contains("nodes_removed: 0"));

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("D:") || n.id.contains('/'))
            .count(),
        1
    );
}

#[test]
fn update_with_spaces_in_paths() {
    let dir = temp_workspace();
    let dir_with_spaces = dir.path().join("my project");
    std::fs::create_dir_all(&dir_with_spaces).expect("create dir with spaces");
    std::fs::write(dir_with_spaces.join("readme.md"), b"# Project").expect("write readme");

    exec_ok(&["kg", "create", "project"], dir.path());

    let dir_with_spaces_str = dir_with_spaces.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(Node {
        id: "D:my project".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", dir_with_spaces_str)],
    });
    write_graph(&graph_path, &graph);

    let output = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(output.contains("nodes_added: 1"));

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    let node = graph.node_by_id("readme.md").expect("generated file node");
    assert_eq!(node.source_files.len(), 0);
}

#[test]
fn auto_update_removes_deleted_nodes() {
    let dir = temp_workspace();
    std::fs::create_dir_all(dir.path().join("data")).expect("create data dir");
    std::fs::write(dir.path().join("data/file.txt"), b"content").expect("write file");

    exec_ok(&["kg", "create", "project"], dir.path());

    let data_path = dir.path().join("data");
    let data_path_str = data_path.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(Node {
        id: "D:data".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", data_path_str)],
    });
    write_graph(&graph_path, &graph);

    exec_ok(&["kg", "project", "update"], dir.path());

    std::fs::remove_file(dir.path().join("data/file.txt")).expect("delete file");

    let output = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(output.contains("nodes_removed: 1"));
    assert!(output.contains("edges_removed: 1"));

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    assert!(graph.node_by_id("file.txt").is_none());
    assert!(!graph.has_edge("D:data", "GHAS", "file.txt"));
}

#[test]
fn auto_update_handles_notes_on_removed_nodes() {
    let dir = temp_workspace();
    std::fs::create_dir_all(dir.path().join("data")).expect("create data dir");
    std::fs::write(dir.path().join("data/file.txt"), b"content").expect("write file");

    exec_ok(&["kg", "create", "project"], dir.path());

    let data_path = dir.path().join("data");
    let data_path_str = data_path.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(Node {
        id: "D:data".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", data_path_str)],
    });
    write_graph(&graph_path, &graph);

    exec_ok(&["kg", "project", "update"], dir.path());

    exec_ok(
        &[
            "kg",
            "project",
            "note",
            "add",
            "file.txt",
            "--text",
            "Important file",
        ],
        dir.path(),
    );

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    assert_eq!(graph.notes.len(), 1);

    std::fs::remove_file(dir.path().join("data/file.txt")).expect("delete file");

    let output = exec_ok(&["kg", "project", "update"], dir.path());
    assert!(output.contains("notes_removed: 1"));

    let graph = load_graph(&test_graph_root(dir.path()).join("project.kg"));
    assert_eq!(graph.notes.len(), 0);
}

#[test]
fn generated_node_rendering_fallback_without_explicit_names() {
    let dir = temp_workspace();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    std::fs::write(dir.path().join("src/main.rs"), b"fn main() {}").expect("write main.rs");

    exec_ok(&["kg", "create", "project"], dir.path());

    let src_path_buf = dir.path().join("src");
    let src_path = src_path_buf.to_str().unwrap();
    let graph_path = test_graph_root(dir.path()).join("project.kg");
    let mut graph = load_graph(&graph_path);
    graph.nodes.push(kg::Node {
        id: "D:src".to_owned(),
        r#type: "D".to_owned(),
        name: String::new(),
        properties: kg::NodeProperties::default(),
        source_files: vec![format!("SOURCECODE {}", src_path)],
    });
    write_graph(&graph_path, &graph);

    exec_ok(&["kg", "project", "update"], dir.path());

    let output = exec_ok(&["kg", "project", "node", "get", "D:src"], dir.path());
    assert!(output.contains("D:src") || output.contains("src"));

    let output = exec_ok(&["kg", "project", "node", "get", "main.rs"], dir.path());
    assert!(output.contains("main.rs"));
}
