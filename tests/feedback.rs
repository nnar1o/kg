mod common;

use common::{exec_ok, temp_workspace, test_graph_root, write_fixture};

#[test]
fn feedback_log_lists_recent_entries_and_supports_filters() {
    let dir = temp_workspace();
    let log_path = dir.path().join("kg-mcp.feedback.log");
    std::fs::write(
        &log_path,
        "ts_ms=1\tuid=aaaaaa\taction=YES\tpick=-\tselected=concept:refrigerator\tgraph=fridge\tqueries=lodowka\n\
ts_ms=2\tuid=bbbbbb\taction=NO\tpick=-\tselected=-\tgraph=fridge\tqueries=smart\n\
ts_ms=3\tuid=cccccc\taction=PICK\tpick=2\tselected=process:diagnostics\tgraph=fridge\tqueries=diag\n",
    )
    .expect("write feedback log");

    let output = exec_ok(&["kg", "feedback-log", "--limit", "2"], dir.path());
    assert!(output.contains("= feedback-log"));
    assert!(output.contains("total_entries: 3"));
    assert!(output.contains("showing: 2"));
    assert!(output.contains("- 3 | cccccc | PICK"));

    let filtered = exec_ok(
        &["kg", "feedback-log", "--uid", "aaaaaa", "--limit", "5"],
        dir.path(),
    );
    assert!(filtered.contains("total_entries: 1"));
    assert!(!filtered.contains("uid=aaaaaa"));
    assert!(filtered.contains("aaaaaa"));
    assert!(!filtered.contains("bbbbbb"));
}

#[test]
fn feedback_summary_parses_and_aggregates_correctly() {
    let dir = temp_workspace();
    write_fixture(&test_graph_root(dir.path()));
    let log_path = dir.path().join("kg-mcp.feedback.log");
    std::fs::write(
        &log_path,
        "ts_ms=1\tuid=aaaaaa\taction=YES\tpick=-\tselected=concept:foo\tgraph=fridge\tqueries=foo\n\
ts_ms=2\tuid=bbbbbb\taction=YES\tpick=-\tselected=concept:bar\tgraph=fridge\tqueries=bar\n\
ts_ms=3\tuid=cccccc\taction=NO\tpick=-\tselected=-\tgraph=fridge\tqueries=baz\n\
ts_ms=4\tuid=dddddd\taction=NIL\tpick=-\tselected=-\tgraph=fridge\tqueries=missing\n\
ts_ms=5\tuid=eeeeee\taction=PICK\tpick=1\tselected=concept:foo\tgraph=fridge\tqueries=xyz\n",
    )
    .expect("write feedback log");

    let output = exec_ok(&["kg", "fridge", "feedback-summary"], dir.path());
    assert!(output.contains("= feedback-summary"));
    assert!(output.contains("YES:  2"));
    assert!(output.contains("NO:   1"));
    assert!(output.contains("NIL:  1"));
    assert!(output.contains("Brakujące node'y"));
    assert!(output.contains("missing"));
    assert!(output.contains("concept:foo"));
    assert!(output.contains("Top wyszukiwane"));
}
