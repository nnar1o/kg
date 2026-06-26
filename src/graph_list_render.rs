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
