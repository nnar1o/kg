use anyhow::Result;
use serde::Serialize;

use crate::storage::GraphStore;

#[derive(Debug, Serialize)]
pub(crate) struct GraphListEntry {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphListResponse {
    graphs: Vec<GraphListEntry>,
}

pub(crate) fn render_graph_list(store: &dyn GraphStore, full: bool) -> Result<String> {
    let graphs = store.list_graphs()?;

    let mut lines = vec![format!("= graphs ({})", graphs.len())];
    for (name, path) in graphs {
        if full {
            lines.push(format!("- {name} | {}", path.display()));
        } else {
            lines.push(format!("- {name}"));
        }
    }
    Ok(format!("{}\n", lines.join("\n")))
}

pub(crate) fn render_graph_list_json(store: &dyn GraphStore) -> Result<String> {
    let graphs = store.list_graphs()?;
    let entries = graphs
        .into_iter()
        .map(|(name, path)| GraphListEntry {
            name,
            path: path.display().to_string(),
        })
        .collect();
    let payload = GraphListResponse { graphs: entries };
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use anyhow::{bail, Result};
    use crate::graph::GraphFile;

    struct MockStore {
        graphs: Vec<(String, PathBuf)>,
    }

    impl MockStore {
        fn new(graphs: Vec<(&str, &str)>) -> Self {
            Self {
                graphs: graphs.into_iter().map(|(n, p)| (n.to_owned(), PathBuf::from(p))).collect(),
            }
        }
    }

    impl GraphStore for MockStore {
        fn create_graph(&self, _graph_name: &str) -> Result<PathBuf> { bail!("unused") }
        fn resolve_graph_path(&self, _graph: &str) -> Result<PathBuf> { bail!("unused") }
        fn list_graphs(&self) -> Result<Vec<(String, PathBuf)>> { Ok(self.graphs.clone()) }
        fn load_graph(&self, _path: &Path) -> Result<GraphFile> { bail!("unused") }
        fn save_graph(&self, _path: &Path, _graph: &GraphFile) -> Result<()> { bail!("unused") }
    }

    #[test]
    fn render_graph_list_empty() {
        let store = MockStore::new(vec![]);
        let out = render_graph_list(&store, false).unwrap();
        assert_eq!(out, "= graphs (0)\n");
    }

    #[test]
    fn render_graph_list_with_graphs() {
        let store = MockStore::new(vec![("main", "/a/b.json"), ("dev", "/c/d.json")]);
        let out = render_graph_list(&store, false).unwrap();
        assert_eq!(out, "= graphs (2)\n- main\n- dev\n");
    }

    #[test]
    fn render_graph_list_full_shows_paths() {
        let store = MockStore::new(vec![("main", "/a/b.json")]);
        let out = render_graph_list(&store, true).unwrap();
        assert!(out.contains("main | /a/b.json"));
    }

    #[test]
    fn render_graph_list_json_empty() {
        let store = MockStore::new(vec![]);
        let out = render_graph_list_json(&store).unwrap();
        assert_eq!(out, "{\n  \"graphs\": []\n}");
    }

    #[test]
    fn render_graph_list_json_with_graphs() {
        let store = MockStore::new(vec![("main", "/a/b.json")]);
        let out = render_graph_list_json(&store).unwrap();
        assert!(out.contains("\"name\": \"main\""));
        assert!(out.contains("\"path\": \"/a/b.json\""));
    }
}
