mod common;

use common::{exec_ok, temp_workspace, test_graph_root, write_fixture};

#[test]
fn baseline_reports_quality_feedback_cost_and_golden_metrics() {
    let dir = temp_workspace();
    let graph_root = test_graph_root(dir.path());
    write_fixture(&graph_root);

    let feedback_log = dir.path().join("kg-mcp.feedback.log");
    std::fs::write(
        &feedback_log,
        "ts_ms=1\tuid=aaaaaa\taction=YES\tpick=-\tselected=concept:refrigerator\tgraph=fridge\tqueries=lodowka\n\
ts_ms=2\tuid=bbbbbb\taction=NO\tpick=-\tselected=-\tgraph=fridge\tqueries=unknown\n\
ts_ms=3\tuid=cccccc\taction=PICK\tpick=1\tselected=concept:refrigerator\tgraph=fridge\tqueries=fridge\n",
    )
    .expect("write feedback log");

    let access_log = graph_root.join("fridge.json.access.log");
    std::fs::write(
        &access_log,
        "2026-04-01 10:00:00.000\tFIND\tlodowka\t1\t10ms\n\
2026-04-01 10:00:01.000\tFIND\tunknown\t0\t10ms\n\
2026-04-01 10:00:02.000\tGET\tconcept:refrigerator\t1\t2ms\n",
    )
    .expect("write access log");

    let golden_path = dir.path().join("golden.json");
    std::fs::write(
        &golden_path,
        r#"[
  {"query": "lodowka", "expected": ["concept:refrigerator"]},
  {"query": "unknown thing", "expected": ["concept:not_found"]}
]"#,
    )
    .expect("write golden set");

    let output = exec_ok(
        &[
            "kg",
            "fridge",
            "baseline",
            "--golden",
            golden_path.to_str().expect("golden path"),
        ],
        dir.path(),
    );

    assert!(output.contains("= baseline"));
    assert!(output.contains("quality_score_0_100:"));
    assert!(output.contains("feedback:"));
    assert!(output.contains("cost:"));
    assert!(output.contains("feedback_events_per_1000_find_ops: 1500.0"));
    assert!(output.contains("golden_set:"));
    assert!(output.contains("cases: 2"));
}
