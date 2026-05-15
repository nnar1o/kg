use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use tempfile::tempdir;

#[test]
fn kg_create_writes_graph() {
    let dir = tempdir().expect("tempdir");
    let output = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args(["create", "fridge"])
        .output()
        .expect("run kg");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("+ created"));
    assert!(dir.path().join(".kg/graphs/fridge.kg").exists());
    assert!(!dir.path().join(".kg/graphs/fridge.json").exists());
}

#[test]
fn kg_mcp_binary_is_built() {
    let path = cargo_bin("kg-mcp");
    assert!(path.exists());
}

#[test]
fn kg_help_shows_current_command_descriptions() {
    let output = Command::new(cargo_bin("kg"))
        .args(["--help"])
        .output()
        .expect("run kg --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("▓ ▄▄"));
    assert!(stdout.contains("Create a new graph"));
    assert!(stdout.contains("List available graphs"));
    assert!(stdout.contains("Run commands against a graph"));
    assert!(stdout.contains("kg graph fridge annotate \"Need fridge help\""));
    assert!(stdout.contains("kg graph fridge node find lodowka"));
}

#[test]
fn kg_graph_help_shows_nested_command_descriptions() {
    let output = Command::new(cargo_bin("kg"))
        .args(["graph", "demo", "--help"])
        .output()
        .expect("run kg graph --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Find, inspect, and edit nodes"));
    assert!(stdout.contains("Add and remove graph edges"));
    assert!(stdout.contains("Annotate free text with graph matches"));
    assert!(stdout.contains("Run graph quality reports"));
}

#[test]
fn kg_reports_underlying_graph_read_error_details() {
    let dir = tempdir().expect("tempdir");
    let graph_root = dir.path().join(".kg").join("graphs");
    std::fs::create_dir_all(&graph_root).expect("create graph root");
    std::fs::write(graph_root.join("broken.kg"), "{").expect("write broken graph");

    let output = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args(["graph", "broken", "stats"])
        .output()
        .expect("run kg");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: invalid legacy JSON payload in .kg file"));
    assert!(stderr.contains("line 1, column 1"));
    assert!(stderr.contains("EOF while parsing an object"));
    assert!(stderr.contains("fragment: {"));
}

#[test]
fn kg_ignores_invalid_kg_entry_and_warns_with_line_fragment() {
    let dir = tempdir().expect("tempdir");
    let graph_root = dir.path().join(".kg").join("graphs");
    let kglog_path = dir.path().join(".kg").join("cache").join("broken.kglog");
    std::fs::create_dir_all(&graph_root).expect("create graph root");
    std::fs::write(
        graph_root.join("broken.kg"),
        "@ K:concept:test\nN Test\nE not-a-timestamp\n",
    )
    .expect("write broken kg graph");

    let output = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args(["graph", "broken", "stats"])
        .output()
        .expect("run kg");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nodes: 1"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.trim().is_empty());
    let kglog = std::fs::read_to_string(&kglog_path).expect("read kglog");
    assert!(kglog.contains(" kgparse0 W - ignored invalid graph entry"));
    assert!(kglog.contains("invalid E timestamp at line 3"));
    assert!(kglog.contains("fragment: E not-a-timestamp"));
}

#[test]
fn kg_node_add_minimal_command_applies_safe_defaults() {
    let dir = tempdir().expect("tempdir");

    let create = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args(["create", "fridge"])
        .output()
        .expect("run kg create");
    assert!(create.status.success());

    let add = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args([
            "graph",
            "fridge",
            "node",
            "add",
            "concept:refrigerator",
            "--type",
            "Concept",
            "--name",
            "Refrigerator",
        ])
        .output()
        .expect("run kg node add");
    assert!(add.status.success());

    let stats = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args(["graph", "fridge", "stats", "--by-type"])
        .output()
        .expect("run kg stats");
    assert!(stats.status.success());
    let stdout = String::from_utf8_lossy(&stats.stdout);
    assert!(stdout.contains("nodes: 1"));
    assert!(stdout.contains("Concept: 1"));
    assert!(!stdout.contains("^:"));
}

#[test]
fn kg_graph_annotate_renders_inline_matches() {
    let dir = tempdir().expect("tempdir");
    let graph_root = dir.path().join(".kg").join("graphs");
    std::fs::create_dir_all(&graph_root).expect("create graph root");
    std::fs::write(
        graph_root.join("fridge.json"),
        include_str!("../graph-example-fridge.json"),
    )
    .expect("write fixture");

    let output = Command::new(cargo_bin("kg"))
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .args([
            "graph",
            "fridge",
            "annotate",
            "Need fridge and lodowka help",
        ])
        .output()
        .expect("run kg annotate");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fridge [kg fridge @K:refrigerator]"));
    assert!(stdout.contains("lodowka [kg lodowka @K:refrigerator]"));
}
