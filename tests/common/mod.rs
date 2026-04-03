#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

pub fn temp_workspace() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

pub fn test_graph_root(cwd: &Path) -> PathBuf {
    cwd.join(".kg").join("graphs")
}

pub fn write_fixture(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("create graph root");
    let path = dir.join("fridge.json");
    std::fs::write(&path, include_str!("../../graph-example-fridge.json")).expect("write fixture");
    path
}

pub fn write_config(cwd: &Path, body: &str) {
    std::fs::write(cwd.join(".kg.toml"), body).expect("write config");
}

pub fn load_graph(path: &Path) -> kg::GraphFile {
    let resolved = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let kg_path = path.with_extension("kg");
        if kg_path.exists() {
            kg_path
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };
    kg::GraphFile::load(&resolved).expect("load graph")
}

pub fn write_graph(path: &Path, graph: &kg::GraphFile) {
    graph.save(path).expect("save graph");
}

pub fn exec_ok(args: &[&str], cwd: &Path) -> String {
    let command_args = args.iter().skip(1).copied().collect::<Vec<_>>();
    let output = std::process::Command::new(cargo_bin("kg"))
        .current_dir(cwd)
        .env("HOME", cwd)
        .args(command_args)
        .output()
        .expect("run kg");
    assert!(
        output.status.success(),
        "kg command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}
