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
    assert!(stdout.contains("Create a new graph"));
    assert!(stdout.contains("List available graphs"));
    assert!(stdout.contains("Run commands against a graph"));
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
    assert!(stdout.contains("Run graph quality reports"));
}
