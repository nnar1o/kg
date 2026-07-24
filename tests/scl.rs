mod common;

use common::{exec_ok, load_graph, temp_workspace, test_graph_root, write_fixture};
use std::path::Path;

use kg::scl;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse an SCL script, convert each line to canonical CLI args, then
/// dispatch via `exec_ok` (which sets HOME=cwd so `default_graph_root`
/// resolves to `cwd/.kg/graphs`).
///
/// Commands where `to_args()` returns empty (Help, ListTypes, etc.) are
/// collected as debug-format strings rather than dispatched.
fn scl_dispatch(script: &str, ctx: &mut scl::Ctx, cwd: &Path) -> Vec<String> {
    let lines = scl::parse_script(script, ctx).expect("scl parse_script should succeed");
    let mut outputs = Vec::new();
    for line in lines {
        let args = line.to_args(ctx);
        if args.is_empty() {
            outputs.push(format!("{line:?}"));
            continue;
        }
        // Build &[&str] for exec_ok.
        // exec_ok expects ["kg", ...canonical...].
        let mut str_args: Vec<String> = Vec::with_capacity(args.len() + 1);
        str_args.push("kg".to_owned());
        for os in &args {
            str_args.push(os.to_string_lossy().to_string());
        }
        let refs: Vec<&str> = str_args.iter().map(String::as_str).collect();
        outputs.push(exec_ok(&refs, cwd));
    }
    outputs
}

/// Default fridge context (non-strict).
fn fridge_ctx() -> scl::Ctx {
    scl::Ctx::new("fridge".to_owned(), false)
}

/// Load the fridge graph from a temp workspace.
fn load_fridge(path: &Path) -> kg::GraphFile {
    load_graph(&test_graph_root(path).join("fridge.json"))
}

// ---------------------------------------------------------------------------
// 1. find — `find <query>` dispatches to `node find` and returns results
// ---------------------------------------------------------------------------

#[test]
fn find_runs_without_error() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    let output = scl_dispatch("find fridge", &mut ctx, dir.path());
    let combined = output.join("");
    assert!(
        combined.contains("concept:") || combined.contains("? fridge"),
        "expected search results, got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// 2. get — `get <nodeid>` returns a specific node
// ---------------------------------------------------------------------------

#[test]
fn get_returns_node() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    let output = scl_dispatch("get concept:refrigerator", &mut ctx, dir.path());
    let combined = output.join("");
    assert!(combined.contains("concept:refrigerator"));
    assert!(combined.contains("Lodowka"));
}

// ---------------------------------------------------------------------------
// 3. add with defaults — verifies all auto-filled fields
// ---------------------------------------------------------------------------

#[test]
fn add_applies_defaults() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    let _output = scl_dispatch("add concept:test_thing", &mut ctx, dir.path());

    let graph = load_fridge(dir.path());
    let node = graph
        .node_by_id("concept:test_thing")
        .expect("node should exist");

    assert_eq!(node.r#type, "Concept", "type inferred from prefix");
    assert_eq!(node.name, "Test Thing", "name humanized from id suffix");
    assert_eq!(node.properties.provenance, "A", "default provenance");
    assert_eq!(node.properties.confidence, Some(0.7), "default confidence");
    assert_eq!(node.properties.importance, 0.5, "default importance");
    assert!(
        node.source_files
            .iter()
            .any(|s| s == "OTHER concept:test_thing"),
        "expected synthetic source, got: {:?}",
        node.source_files
    );
}

// ---------------------------------------------------------------------------
// 4. add with --name override — name should not be humanized
// ---------------------------------------------------------------------------

#[test]
fn add_with_name_override() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    let _output = scl_dispatch(
        r#"add concept:x --name "Custom Name""#,
        &mut ctx,
        dir.path(),
    );

    let graph = load_fridge(dir.path());
    let node = graph.node_by_id("concept:x").expect("node should exist");
    assert_eq!(
        node.name, "Custom Name",
        "name should be the explicit value"
    );
    assert_ne!(node.name, "X", "should NOT be humanized");
}

// ---------------------------------------------------------------------------
// 5. add with --source override — synthetic source NOT applied
// ---------------------------------------------------------------------------

#[test]
fn add_with_source_override() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    let _output = scl_dispatch(
        r#"add concept:y --source "URL https://example.com""#,
        &mut ctx,
        dir.path(),
    );

    let graph = load_fridge(dir.path());
    let node = graph.node_by_id("concept:y").expect("node should exist");
    assert!(
        !node
            .source_files
            .iter()
            .any(|s| s.contains("OTHER concept:y")),
        "synthetic source should NOT be present when --source is given"
    );
    assert!(
        node.source_files
            .iter()
            .any(|s| s == "URL https://example.com"),
        "custom source should be present, got: {:?}",
        node.source_files
    );
}

// ---------------------------------------------------------------------------
// 6. modify — updates importance and appends facts
// ---------------------------------------------------------------------------

#[test]
fn modify_updates_fields() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    // First add a node via SCL (defaults fill required fields),
    // then modify it.
    scl_dispatch(
        r#"add concept:test_thing --domain "testing""#,
        &mut ctx,
        dir.path(),
    );
    let graph = load_fridge(dir.path());
    assert!(
        graph.node_by_id("concept:test_thing").is_some(),
        "node should exist before modify"
    );

    // Modify importance and append a fact
    let _output = scl_dispatch(
        r#"modify concept:test_thing --importance 0.9 --fact "uses R134a""#,
        &mut ctx,
        dir.path(),
    );

    let graph = load_fridge(dir.path());
    let node = graph
        .node_by_id("concept:test_thing")
        .expect("node should exist");
    assert_eq!(node.properties.importance, 0.9, "importance updated");
    assert!(
        node.properties.key_facts.iter().any(|f| f == "uses R134a"),
        "fact should be appended, got: {:?}",
        node.properties.key_facts
    );
}

// ---------------------------------------------------------------------------
// 7. connect — creates an edge between two nodes
// ---------------------------------------------------------------------------

#[test]
fn connect_creates_edge() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    // Both nodes exist in the fixture
    let _output = scl_dispatch(
        "connect concept:refrigerator USES process:cooling",
        &mut ctx,
        dir.path(),
    );

    let graph = load_fridge(dir.path());
    assert!(
        graph.has_edge("concept:refrigerator", "USES", "process:cooling"),
        "edge should exist after connect"
    );
}

// ---------------------------------------------------------------------------
// 8. connect with invalid relation → SclError with unknown_relation
// ---------------------------------------------------------------------------

#[test]
fn connect_with_invalid_relation_errors() {
    let ctx = fridge_ctx();
    // "OWNS" is accepted as a custom relation token; use a relation with
    // whitespace to trigger the `unknown_relation` error.
    let err = scl::parse_line(r#"connect concept:refrigerator "BAD REL" process:x"#, &ctx)
        .expect_err("should return SclError");
    assert_eq!(err.category, scl::category::UNKNOWN_RELATION);
}

// ---------------------------------------------------------------------------
// 9. connect with edge type mismatch → SclError with edge_type_mismatch
// ---------------------------------------------------------------------------

#[test]
fn connect_with_edge_type_mismatch_errors() {
    let ctx = fridge_ctx();
    // STORED_IN expects source type Concept/Process/Rule and target type DataStore.
    // Using `process:comp` as target does not match DataStore.
    let err = scl::parse_line("connect concept:refrigerator STORED_IN process:comp", &ctx)
        .expect_err("should return SclError");
    assert_eq!(err.category, scl::category::EDGE_TYPE_MISMATCH);
}

// ---------------------------------------------------------------------------
// 10. remove node — add then remove, verify node gone
// ---------------------------------------------------------------------------

#[test]
fn remove_deletes_node() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    // Add a node
    scl_dispatch("add concept:temp_remove_me", &mut ctx, dir.path());
    assert!(
        load_fridge(dir.path())
            .node_by_id("concept:temp_remove_me")
            .is_some(),
        "node should exist before removal"
    );

    // Remove it
    scl_dispatch("remove concept:temp_remove_me", &mut ctx, dir.path());

    assert!(
        load_fridge(dir.path())
            .node_by_id("concept:temp_remove_me")
            .is_none(),
        "node should be gone after removal"
    );
}

// ---------------------------------------------------------------------------
// 11. remove edge — connect then `remove edge`, verify edge gone
// ---------------------------------------------------------------------------

#[test]
fn remove_edge_deletes_edge() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    // Connect two existing fixture nodes
    scl_dispatch(
        "connect concept:refrigerator USES process:cooling",
        &mut ctx,
        dir.path(),
    );
    assert!(
        load_fridge(dir.path()).has_edge("concept:refrigerator", "USES", "process:cooling"),
        "edge should exist before removal"
    );

    // Remove via `remove edge <src> <R> <tgt>`
    scl_dispatch(
        "remove edge concept:refrigerator USES process:cooling",
        &mut ctx,
        dir.path(),
    );

    assert!(
        !load_fridge(dir.path()).has_edge("concept:refrigerator", "USES", "process:cooling"),
        "edge should be gone after removal"
    );
}

// ---------------------------------------------------------------------------
// 12. remove disambiguation — 3 barewords, uppercase relation → edge remove
// ---------------------------------------------------------------------------

#[test]
fn remove_disambiguation_three_barewords() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = fridge_ctx();

    // Add an edge
    scl_dispatch(
        "connect concept:refrigerator USES process:cooling",
        &mut ctx,
        dir.path(),
    );
    assert!(
        load_fridge(dir.path()).has_edge("concept:refrigerator", "USES", "process:cooling"),
        "edge should exist before removal"
    );

    // Remove via 3 barewords: `remove <src> <RELATION> <tgt>`
    scl_dispatch(
        "remove concept:refrigerator USES process:cooling",
        &mut ctx,
        dir.path(),
    );

    assert!(
        !load_fridge(dir.path()).has_edge("concept:refrigerator", "USES", "process:cooling"),
        "edge should be gone after 3-bareword remove"
    );
}

// ---------------------------------------------------------------------------
// 13. Passthrough — unknown verb returns Ok(None) (not an error)
// ---------------------------------------------------------------------------

#[test]
fn passthrough_unknown_verb() {
    let ctx = fridge_ctx();
    let result = scl::parse_line("fridge node find \"x\"", &ctx).expect("should not error");
    assert!(
        result.is_none(),
        "unknown verb should produce None (passthrough to CLI)"
    );
}

// ---------------------------------------------------------------------------
// 14. strict mode — missing required field error
// ---------------------------------------------------------------------------

#[test]
fn strict_mode_rejects_missing_fields() {
    let mut ctx = scl::Ctx::new("fridge".to_owned(), false);
    let err = scl::parse_script("strict; add concept:z", &mut ctx)
        .expect_err("should error in strict mode");
    assert_eq!(err.category, scl::category::MISSING_REQUIRED_FIELD);
    assert!(err.message.contains("--provenance"));
}

// ---------------------------------------------------------------------------
// 15. use persists — set graph context then dispatch find
// ---------------------------------------------------------------------------

#[test]
fn use_persists_across_lines() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let mut ctx = scl::Ctx::new("default".to_owned(), false);

    let outputs = scl_dispatch("use fridge; find x", &mut ctx, dir.path());

    assert_eq!(ctx.graph, "fridge", "graph context updated by `use`");
    assert!(!outputs.is_empty(), "expected output from find");
    let combined = outputs.join("");
    assert!(
        combined.contains("concept:") || combined.contains("? x"),
        "expected search results on fridge graph, got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// 16. feedback — parses to Feedback variant with correct fields
// ---------------------------------------------------------------------------

#[test]
fn feedback_parses_correctly() {
    let ctx = fridge_ctx();
    let line = scl::parse_line("feedback abc123 yes", &ctx)
        .expect("should parse")
        .expect("should produce a line");
    match line {
        scl::CanonicalLine::Feedback { uid, verdict, pick } => {
            assert_eq!(uid, "abc123");
            assert_eq!(verdict, "YES");
            assert_eq!(pick, None);
        }
        other => panic!("expected Feedback, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 17. Empty script → vec![Help { topic: None }]
// ---------------------------------------------------------------------------

#[test]
fn empty_script_returns_help() {
    let mut ctx = fridge_ctx();
    let result = scl::parse_script("", &mut ctx).expect("should not error");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], scl::CanonicalLine::Help { topic: None });
}

// ---------------------------------------------------------------------------
// 18. list types — returns ListTypes variant
// ---------------------------------------------------------------------------

#[test]
fn list_types_returns_valid_types() {
    let ctx = fridge_ctx();
    let line = scl::parse_line("list types", &ctx)
        .expect("should parse")
        .expect("should produce a line");
    assert_eq!(line, scl::CanonicalLine::ListTypes);
    assert!(line.to_args(&ctx).is_empty());
}

// ---------------------------------------------------------------------------
// 19. list relations — returns ListRelations variant
// ---------------------------------------------------------------------------

#[test]
fn list_relations_returns_valid_relations() {
    let ctx = fridge_ctx();
    let line = scl::parse_line("list relations", &ctx)
        .expect("should parse")
        .expect("should produce a line");
    assert_eq!(line, scl::CanonicalLine::ListRelations);
    assert!(line.to_args(&ctx).is_empty());
}

// ---------------------------------------------------------------------------
// 20. help — `help` alone → Help { topic: None }
// ---------------------------------------------------------------------------

#[test]
fn help_no_topic() {
    let ctx = fridge_ctx();
    let line = scl::parse_line("help", &ctx)
        .expect("should parse")
        .expect("should produce a line");
    assert_eq!(line, scl::CanonicalLine::Help { topic: None });
}

// ---------------------------------------------------------------------------
// Additional: help with topic
// ---------------------------------------------------------------------------

#[test]
fn help_with_topic() {
    let ctx = fridge_ctx();
    let line = scl::parse_line("help find", &ctx)
        .expect("should parse")
        .expect("should produce a line");
    assert_eq!(
        line,
        scl::CanonicalLine::Help {
            topic: Some("find".to_owned())
        }
    );
}
