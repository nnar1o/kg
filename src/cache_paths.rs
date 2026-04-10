use std::path::{Path, PathBuf};

pub fn cache_root_for_cwd(cwd: &Path) -> PathBuf {
    cwd.join(".kg").join("cache")
}

pub fn cache_root_for_graph(graph_path: &Path) -> PathBuf {
    if let Some(root) = detect_kg_root_for_graph(graph_path) {
        return root.join("cache");
    }
    graph_path
        .parent()
        .map(|parent| parent.join(".kg").join("cache"))
        .unwrap_or_else(|| PathBuf::from(".kg").join("cache"))
}

fn detect_kg_root_for_graph(graph_path: &Path) -> Option<PathBuf> {
    let mut current = graph_path.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|name| name.to_str()) == Some("graphs")
            && dir
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                == Some(".kg")
        {
            return dir.parent().map(Path::to_path_buf);
        }
        current = dir.parent();
    }
    None
}
