mod common;

use common::{exec_ok, temp_workspace, test_graph_root, write_config, write_fixture};

#[test]
fn find_supports_multiple_queries() {
    let dir = temp_workspace();
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
fn kql_filters_nodes_by_type() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(&["kg", "fridge", "kql", "node type=Concept"], dir.path());
    assert!(output.contains("nodes:"));
    assert!(output.contains("concept:refrigerator"));
}

#[test]
fn find_uses_fuzzy_matching_for_imperfect_queries() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(&["kg", "fridge", "node", "find", "smrt api"], dir.path());
    assert!(output.contains("? smrt api ("));
    assert!(output.contains("# interface:smart_api | Smart Home API (REST)"));
    assert!(!output.contains("# process:diagnostics | Autodiagnostyka"));
}

#[test]
fn list_graphs_shows_available_graph_names() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    write_fixture(&dir.path().join(".kg").join("graphs"));
    let output = exec_ok(&["kg", "list"], dir.path());
    assert!(output.contains("= graphs (1)"));
    assert!(output.contains("- fridge"));
}

#[test]
fn list_nodes_supports_type_filter_and_limit() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let output = exec_ok(
        &["kg", "fridge", "list", "--type", "Process", "--limit", "1"],
        dir.path(),
    );
    assert!(output.contains("= nodes (3)"));
    assert!(output.contains("[Process]"));
    assert!(!output.contains("[Concept]"));
}

#[test]
fn node_list_subcommand_matches_graph_list() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let graph_list = exec_ok(&["kg", "fridge", "list", "--limit", "5"], dir.path());
    let node_list = exec_ok(
        &["kg", "fridge", "node", "list", "--limit", "5"],
        dir.path(),
    );
    assert_eq!(graph_list, node_list);
}

#[test]
fn resolve_graph_path_uses_config_mapping() {
    let dir = temp_workspace();
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
fn get_full_renders_new_properties() {
    let dir = temp_workspace();
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
