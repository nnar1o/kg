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
    assert!(dir.path().join(".kg/graphs/fridge.json").exists());
}

#[test]
fn kg_mcp_binary_is_built() {
    let path = cargo_bin("kg-mcp");
    assert!(path.exists());
}
