mod common;

use common::{exec_ok, temp_workspace, test_graph_root, write_config, write_fixture};
use std::fs;

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
    assert!(output.contains("importance: 4"));
    assert!(output.contains("created_at: 2026-03-20T01:10:00Z"));
}

#[test]
fn default_runtime_auto_migrates_json_graph_to_kg_side_by_side() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));
    let kg_path = graph_path.with_extension("kg");
    assert!(!kg_path.exists());

    let output = exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );

    assert!(output.contains("# concept:refrigerator | Lodowka"));
    assert!(kg_path.exists());
    assert!(graph_path.exists());
    let kg_raw = fs::read_to_string(&kg_path).expect("read migrated kg");
    assert!(kg_raw.contains("@ K:concept:refrigerator"));
    assert!(!kg_raw.trim_start().starts_with('{'));
}

#[test]
fn legacy_flag_uses_json_without_creating_kg_copy() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));
    let kg_path = graph_path.with_extension("kg");
    if kg_path.exists() {
        fs::remove_file(&kg_path).expect("remove stale kg");
    }

    let output = exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "--legacy",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );

    assert!(output.contains("# concept:refrigerator | Lodowka"));
    assert!(!kg_path.exists());
    assert!(graph_path.exists());
}

#[test]
fn default_runtime_creates_kg_sidecars_for_index_and_hit_log() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));
    let kg_path = graph_path.with_extension("kg");
    let kgindex_path = graph_path.with_extension("kgindex");
    let kglog_path = graph_path.with_extension("kglog");

    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );

    assert!(kg_path.exists());
    assert!(kgindex_path.exists());
    assert!(kglog_path.exists());

    let index_raw = fs::read_to_string(&kgindex_path).expect("read kgindex");
    assert!(index_raw.contains("concept:refrigerator "));

    let log_raw = fs::read_to_string(&kglog_path).expect("read kglog");
    assert!(log_raw.contains(" H concept:refrigerator"));
}

#[test]
fn modifying_kg_graph_invalidates_and_then_rebuilds_kgindex() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));
    let kgindex_path = graph_path.with_extension("kgindex");

    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );
    assert!(kgindex_path.exists());

    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "modify",
            "concept:refrigerator",
            "--description",
            "Nowy opis",
        ],
        dir.path(),
    );
    assert!(!kgindex_path.exists());

    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );
    assert!(kgindex_path.exists());
}

#[test]
fn migration_writes_report_and_maps_legacy_aliases() {
    let dir = temp_workspace();
    let legacy_dir = dir.path().join("legacy");
    fs::create_dir_all(&legacy_dir).expect("create legacy dir");
    let json_path = legacy_dir.join("fridge.json");
    fs::write(
        &json_path,
        r#"{
  "metadata": {"name": "fridge", "version": "1.0", "description": "x", "node_count": 3, "edge_count": 2},
  "nodes": [
    {"id": "concept:refrigerator", "type": "concept", "name": "Fridge", "properties": {"description": "d"}, "source_files": ["a.md"]},
    {"id": "process:cooling", "type": "process", "name": "Cooling", "properties": {"description": "d"}, "source_files": ["a.md"]},
    {"id": "x:legacy", "type": "Very Legacy Type", "name": "Legacy", "properties": {"description": "d"}, "source_files": ["a.md"]}
  ],
  "edges": [
    {"source_id": "concept:refrigerator", "relation": "stored_in", "target_id": "process:cooling", "properties": {}},
    {"source_id": "concept:refrigerator", "relation": "<-created_by", "target_id": "process:cooling", "properties": {}}
  ],
  "notes": []
}"#,
    )
    .expect("write legacy graph");

    write_config(dir.path(), "[graphs]\nfridge = \"legacy/fridge.json\"\n");
    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );

    let kg_path = legacy_dir.join("fridge.kg");
    let report_path = legacy_dir.join("fridge.migration.log");
    assert!(kg_path.exists());
    assert!(report_path.exists());

    let graph = kg::GraphFile::load(&kg_path).expect("load migrated kg");
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "concept:refrigerator")
            .expect("node")
            .r#type,
        "Concept"
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "x:legacy")
            .expect("legacy")
            .r#type,
        "verylegacy"
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.source_id == "concept:refrigerator" && e.relation == "READS_FROM")
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.source_id == "process:cooling" && e.relation == "CREATED_BY")
    );

    let report_raw = fs::read_to_string(report_path).expect("read report");
    assert!(report_raw.contains("= migration-report"));
    assert!(report_raw.contains("mapped_node_types:"));
    assert!(report_raw.contains("mapped_relations:"));
    assert!(report_raw.contains("incoming_edges_rewritten: 1"));
}

#[test]
fn get_still_works_when_kglog_path_is_unwritable() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));
    let kglog_path = graph_path.with_extension("kglog");

    exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );

    if kglog_path.exists() {
        fs::remove_file(&kglog_path).expect("remove old kglog");
    }
    fs::create_dir_all(&kglog_path).expect("replace kglog with directory");

    let output = exec_ok(
        &[
            "kg",
            "graph",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
        ],
        dir.path(),
    );
    assert!(output.contains("# concept:refrigerator | Lodowka"));
}
