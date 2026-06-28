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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_root_for_cwd_uses_dot_kg_cache() {
        let root = cache_root_for_cwd(Path::new("/home/user/project"));
        assert_eq!(root, Path::new("/home/user/project/.kg/cache"));
    }

    #[test]
    fn cache_root_for_graph_within_kg_project() {
        let path = Path::new("/home/user/project/.kg/graphs/mygraph.json");
        let root = cache_root_for_graph(path);
        assert_eq!(root, Path::new("/home/user/project/.kg/cache"));
    }

    #[test]
    fn cache_root_for_graph_outside_kg_project() {
        let path = Path::new("/home/user/random/graph.json");
        let root = cache_root_for_graph(path);
        assert_eq!(root, Path::new("/home/user/random/.kg/cache"));
    }

    #[test]
    fn detect_kg_root_finds_ancestor_dot_kg() {
        let path = Path::new("/a/b/.kg/graphs/x.json");
        let root = detect_kg_root_for_graph(path);
        assert_eq!(root, Some(PathBuf::from("/a/b/.kg")));
    }

    #[test]
    fn detect_kg_root_returns_none_for_unrelated_path() {
        let path = Path::new("/tmp/random/file.json");
        assert!(detect_kg_root_for_graph(path).is_none());
    }
}
