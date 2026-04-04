mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_config};

#[test]
fn create_graph_writes_empty_graph_file() {
    let dir = temp_workspace();
    let output = exec_ok(&["kg", "create", "fridge"], dir.path());
    assert!(output.contains("+ created"));
    assert!(output.contains(".kg/graphs/fridge.kg"));
    assert!(!test_graph_root(dir.path()).join("fridge.json").exists());
    let graph = load_graph(&test_graph_root(dir.path()).join("fridge.kg"));
    assert_eq!(graph.metadata.name, "fridge");
    assert_eq!(graph.metadata.node_count, 0);
    assert!(graph.nodes.is_empty());
}

#[test]
fn create_graph_writes_redb_file() {
    let dir = temp_workspace();
    write_config(dir.path(), "backend = \"redb\"\n");
    let output = exec_ok(&["kg", "create", "fridge"], dir.path());
    assert!(output.contains("+ created"));
    assert!(output.contains(".kg/graphs/fridge.db"));
    assert!(test_graph_root(dir.path()).join("fridge.db").exists());
}
