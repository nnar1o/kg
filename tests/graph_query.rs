mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_config, write_fixture};
use serde_json::Value;
use std::fs;

fn strip_find_wrapper(output: &str) -> String {
    output
        .lines()
        .skip(1)
        .filter(|line| !line.ends_with("more nodes omitted by limit"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_score_lines(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("score: "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_find_header_counts(output: &str) -> (usize, usize) {
    let header = output.lines().next().expect("find header");
    let counts = header
        .split('(')
        .nth(1)
        .and_then(|rest| rest.strip_suffix(')'))
        .expect("header counts");
    if let Some((shown, total)) = counts.split_once('/') {
        (
            shown.parse::<usize>().expect("shown count"),
            total.parse::<usize>().expect("total count"),
        )
    } else {
        let total = counts.parse::<usize>().expect("total count");
        (total, total)
    }
}

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
fn find_fuzzy_matches_key_facts_without_primary_hits() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "oauth2 producenta",
            "--mode",
            "fuzzy",
        ],
        dir.path(),
    );

    assert!(output.contains("? oauth2 producenta ("));
    assert!(output.contains("# interface:smart_api | Smart Home API (REST)"));
}

#[test]
fn find_fuzzy_matches_attached_note_content() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:fuzzy_note_search",
            "--text",
            "kalibracja_czujnika_turbo",
            "--tag",
            "serwis",
        ],
        dir.path(),
    );

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "kalibracja_czujnika_turbo",
            "--mode",
            "fuzzy",
        ],
        dir.path(),
    );

    assert!(output.contains("# concept:refrigerator | Lodowka"));
}

#[test]
fn find_prefers_higher_importance_in_fuzzy_and_bm25() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "add",
            "concept:a_importance_high",
            "--type",
            "Concept",
            "--name",
            "Importance High",
            "--description",
            "fraza_importance_test",
            "--importance",
            "6",
            "--source",
            "importance.md",
        ],
        dir.path(),
    );

    exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "add",
            "concept:z_importance_low",
            "--type",
            "Concept",
            "--name",
            "Importance Low",
            "--description",
            "fraza_importance_test",
            "--importance",
            "1",
            "--source",
            "importance.md",
        ],
        dir.path(),
    );

    for mode in ["fuzzy", "bm25"] {
        let output = exec_ok(
            &[
                "kg",
                "fridge",
                "node",
                "find",
                "fraza_importance_test",
                "--mode",
                mode,
                "--limit",
                "10",
            ],
            dir.path(),
        );

        let high_pos = output
            .find("# concept:a_importance_high | Importance High")
            .expect("high importance result present");
        let low_pos = output
            .find("# concept:z_importance_low | Importance Low")
            .expect("low importance result present");
        assert!(
            high_pos < low_pos,
            "higher importance should rank first in mode {mode}"
        );
    }
}

#[test]
fn find_includes_outgoing_neighbor_context_by_default() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(&["kg", "fridge", "node", "find", "urlopowy"], dir.path());

    assert!(output.contains("# feature:vacation_mode | Tryb Urlopowy [Feature]"));
    assert!(output.contains("# interface:smart_api | Smart Home API (REST)"));
}

#[test]
fn find_returns_expected_primary_match_for_neighbor_phrase() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "Chlodziarka",
            "--full",
            "--limit",
            "1",
        ],
        dir.path(),
    );

    assert!(output.contains("score: "));
    assert!(output.contains("# concept:refrigerator | Lodowka"));
}

#[test]
fn find_single_result_matches_get_full_rendering() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let get_output = exec_ok(
        &["kg", "fridge", "node", "get", "process:cooling", "--full"],
        dir.path(),
    );
    let find_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "process:cooling",
            "--full",
            "--limit",
            "1",
        ],
        dir.path(),
    );

    let find_body = strip_find_wrapper(&find_output);
    assert!(find_body.contains("score: "));
    assert_eq!(strip_score_lines(&find_body).trim(), get_output.trim());
}

#[test]
fn find_single_result_matches_get_adaptive_rendering() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let get_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "get",
            "process:cooling",
            "--output-size",
            "700",
        ],
        dir.path(),
    );
    let find_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "process:cooling",
            "--output-size",
            "700",
            "--limit",
            "1",
        ],
        dir.path(),
    );

    let find_body = strip_find_wrapper(&find_output);
    assert!(find_body.contains("score: "));
    assert_eq!(strip_score_lines(&find_body).trim(), get_output.trim());
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
fn note_list_shows_omitted_marker_with_limit() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:test_one",
            "--text",
            "Pierwsza notatka",
        ],
        dir.path(),
    );
    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:test_two",
            "--text",
            "Druga notatka",
        ],
        dir.path(),
    );

    let output = exec_ok(
        &["kg", "fridge", "note", "list", "--limit", "1"],
        dir.path(),
    );

    assert!(output.contains("= notes (2)"));
    assert!(output.contains("... 1 more notes omitted"));
}

#[test]
fn note_add_preserves_multiline_text_in_kg() {
    let dir = temp_workspace();
    let graph_path = write_fixture(&test_graph_root(dir.path()));

    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:test_multiline",
            "--text",
            "line1\nline2\\nkeep",
        ],
        dir.path(),
    );

    let kg_path = graph_path.with_extension("kg");
    let raw = fs::read_to_string(&kg_path).expect("read kg");
    assert!(raw.contains("b line1\\nline2\\\\nkeep"));

    let graph = load_graph(&graph_path);
    let note = graph
        .notes
        .iter()
        .find(|note| note.id == "note:test_multiline")
        .expect("multiline note");
    assert_eq!(note.body, "line1\nline2\\nkeep");

    let output = exec_ok(&["kg", "fridge", "note", "list"], dir.path());
    assert!(output.contains("note:test_multiline"));
    assert!(output.contains("line1\\nline2\\\\nkeep"));
    assert!(!output.contains("line1\nline2\\nkeep"));
}

#[test]
fn get_full_escapes_multiline_text_fields_for_cli() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "modify",
            "concept:refrigerator",
            "--description",
            "Opis 1\nOpis 2\\nkeep",
            "--provenance",
            "manual\nentry",
        ],
        dir.path(),
    );
    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:test_multiline_render",
            "--text",
            "body1\nbody2\\nkeep",
            "--author",
            "alice\nbob",
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

    assert!(output.contains("desc: Opis 1\\nOpis 2\\\\nkeep"));
    assert!(output.contains("provenance: manual\\nentry"));
    assert!(output.contains("note_body: body1\\nbody2\\\\nkeep"));
    assert!(output.contains("note_author: alice\\nbob"));
}

#[test]
fn find_json_reports_total_matches_not_just_shown_rows() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg", "fridge", "node", "find", "lodowka", "--limit", "1", "--json",
        ],
        dir.path(),
    );
    let payload: Value = serde_json::from_str(&output).expect("parse json");
    let total = payload["total"].as_u64().expect("total") as usize;
    let query = &payload["queries"][0];
    let count = query["count"].as_u64().expect("count") as usize;
    let shown = query["nodes"].as_array().expect("nodes").len();

    assert!(count > shown);
    assert_eq!(total, count);
}

#[test]
fn find_always_returns_score_in_cli_and_json() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let cli_output = exec_ok(
        &["kg", "fridge", "node", "find", "lodowka", "--limit", "1"],
        dir.path(),
    );
    assert!(cli_output.contains("score: "));

    let json_output = exec_ok(
        &[
            "kg", "fridge", "node", "find", "lodowka", "--limit", "1", "--json",
        ],
        dir.path(),
    );
    let payload: Value = serde_json::from_str(&json_output).expect("parse json");
    let first = &payload["queries"][0]["nodes"][0];
    assert!(first["score"].is_i64());
    assert!(first["node"]["id"].is_string());
}

#[test]
fn find_debug_score_returns_breakdown_in_cli_and_json() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let cli_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "lodowka",
            "--limit",
            "1",
            "--debug-score",
        ],
        dir.path(),
    );
    assert!(cli_output.contains("score_debug: raw_relevance="));

    let json_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "lodowka",
            "--limit",
            "1",
            "--json",
            "--debug-score",
        ],
        dir.path(),
    );
    let payload: Value = serde_json::from_str(&json_output).expect("parse json");
    let first = &payload["queries"][0]["nodes"][0];
    assert!(first["score_breakdown"]["raw_relevance"].is_number());
    assert!(first["score_breakdown"]["authority_cap"].is_i64());
}

#[test]
fn adaptive_find_header_matches_rendered_result_count() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "lodowka",
            "--limit",
            "5",
            "--output-size",
            "300",
        ],
        dir.path(),
    );

    let (shown, total) = parse_find_header_counts(&output);
    let adaptive_omitted = output
        .lines()
        .find_map(|line| {
            line.strip_prefix("... +")
                .and_then(|rest| rest.strip_suffix(" more nodes omitted"))
        })
        .expect("adaptive omission line")
        .parse::<usize>()
        .expect("adaptive omitted count");

    assert_eq!(shown + adaptive_omitted, 5);
    assert!(total >= 5);
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
fn get_with_large_output_size_shows_richer_adaptive_output() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "get",
            "concept:refrigerator",
            "--output-size",
            "12000",
        ],
        dir.path(),
    );

    assert!(output.contains("desc: Glowne urzadzenie AGD do przechowywania zywnosci"));
    assert!(output.contains("Klasa energetyczna: A++ lub wyzsza dla nowych modeli"));
    assert!(output.contains("importance: 4"));
    assert!(!output.contains("depth 1:"));
}

#[test]
fn find_with_large_output_size_shows_description_for_single_result() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "find",
            "lodowka",
            "--output-size",
            "12000",
        ],
        dir.path(),
    );

    assert!(output.contains("desc: Glowne urzadzenie AGD do przechowywania zywnosci"));
    assert!(output.contains("importance: 4"));
    assert!(!output.contains("depth 1:"));
}

#[test]
fn find_shows_limit_omission_marker() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let output = exec_ok(
        &[
            "kg", "fridge", "node", "find", "lodowka", "--full", "--limit", "2",
        ],
        dir.path(),
    );

    assert!(output.contains("? lodowka (2/"));
    assert!(output.contains("more nodes omitted by limit"));
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
    assert!(output.contains("desc: Glowne urzadzenie AGD do przechowywania zywnosci"));
    assert!(output.contains("sources: instrukcja_obslugi.md"));
}

#[test]
fn get_full_renders_attached_notes() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    exec_ok(
        &[
            "kg",
            "fridge",
            "note",
            "add",
            "concept:refrigerator",
            "--id",
            "note:test_full_render",
            "--text",
            "Pelna notatka serwisowa",
            "--tag",
            "ops",
            "--author",
            "tester",
            "--created-at",
            "2026-04-07T12:00:00Z",
            "--provenance",
            "manual",
            "--source",
            "notes.md",
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

    assert!(output.contains("notes: 1"));
    assert!(output.contains("! note:test_full_render"));
    assert!(output.contains("note_body: Pelna notatka serwisowa"));
    assert!(output.contains("note_tags: ops"));
    assert!(output.contains("note_author: tester"));
    assert!(output.contains("note_created_at: 2026-04-07T12:00:00Z"));
    assert!(output.contains("note_provenance: manual"));
    assert!(output.contains("note_sources: notes.md"));
}

#[test]
fn find_and_get_include_feature_nodes_by_default() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));

    let get_output = exec_ok(
        &[
            "kg",
            "fridge",
            "node",
            "get",
            "feature:vacation_mode",
            "--full",
        ],
        dir.path(),
    );
    assert!(get_output.contains("# feature:vacation_mode | Tryb Urlopowy [Feature]"));

    let find_output = exec_ok(&["kg", "fridge", "node", "find", "urlopowy"], dir.path());
    assert!(find_output.contains("# feature:vacation_mode | Tryb Urlopowy [Feature]"));
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
