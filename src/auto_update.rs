use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};
use crate::validate::is_generated_node_type;

const FILE_PREFIX: &str = "GFIL";
const DIR_PREFIX: &str = "GDIR";
const RELATION_HAS: &str = "GHAS";
const LEGACY_RELATION_HAS: &str = "HAS";

#[derive(Debug, Default)]
pub struct AutoUpdateSummary {
    pub roots_updated: usize,
    pub nodes_added: usize,
    pub nodes_updated: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
    pub edges_updated: usize,
    pub notes_removed: usize,
}

#[derive(Debug, Clone)]
struct RootSpec {
    id: String,
    path: PathBuf,
    scan_ignore_unknown: bool,
}

#[derive(Debug, Clone)]
struct ScannedEntry {
    path: PathBuf,
    parent_path: PathBuf,
    node_type: &'static str,
}

#[derive(Debug, Clone)]
struct ExistingGeneratedNode {
    id: String,
}

pub fn auto_update_graph(graph: &mut GraphFile) -> Result<AutoUpdateSummary> {
    let roots = collect_root_specs(graph)?;
    if roots.is_empty() {
        bail!(
            "no root D/DIR nodes with a filesystem source found; add one with `kg graph <name> node add DIR:<name> --type DIR --name <name> --source \"SOURCECODE /abs/path\"`"
        );
    }

    let mut summary = AutoUpdateSummary::default();
    for root in roots {
        update_root(graph, &root, &mut summary)?;
        summary.roots_updated += 1;
    }
    graph.refresh_counts();
    Ok(summary)
}

pub fn escape_name(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '~' => escaped.push_str("~~"),
            ':' => escaped.push_str("~c"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn update_root(
    graph: &mut GraphFile,
    root: &RootSpec,
    summary: &mut AutoUpdateSummary,
) -> Result<()> {
    let current_generated = generated_subtree(graph, &root.id);
    let generated_nodes_by_path: HashMap<PathBuf, ExistingGeneratedNode> = current_generated
        .iter()
        .filter_map(|node_id| {
            let node = graph.node_by_id(node_id)?;
            generated_node_scan_path(node, &root.path).map(|path| {
                (
                    path,
                    ExistingGeneratedNode {
                        id: node.id.clone(),
                    },
                )
            })
        })
        .collect();

    let scanned = scan_tree(&root.path, root.scan_ignore_unknown)?;
    let mut dir_ids_by_path = HashMap::from([(root.path.clone(), root.id.clone())]);
    let mut seen_generated_paths = HashSet::new();

    for entry in scanned {
        let parent_id = dir_ids_by_path
            .get(&entry.parent_path)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing parent directory mapping for {}",
                    entry.parent_path.display()
                )
            })?;

        let relative_path = entry.path.strip_prefix(&root.path).with_context(|| {
            format!(
                "failed to compute relative path for {} under {}",
                entry.path.display(),
                root.path.display()
            )
        })?;
        let target_id = generated_node_id(entry.node_type, relative_path);

        let node_id = if let Some(existing) = generated_nodes_by_path.get(&entry.path) {
            if existing.id != target_id {
                rename_generated_node_id(graph, &existing.id, &target_id, summary)?;
            }
            target_id.clone()
        } else {
            target_id.clone()
        };

        upsert_generated_node(graph, &node_id, entry.node_type, summary);
        ensure_has_edge(graph, &parent_id, &node_id, summary);

        seen_generated_paths.insert(entry.path.clone());
        if entry.node_type == DIR_PREFIX {
            dir_ids_by_path.insert(entry.path.clone(), node_id);
        }
    }

    let stale_ids: Vec<String> = current_generated
        .into_iter()
        .filter_map(|node_id| {
            let node = graph.node_by_id(&node_id)?;
            let path = generated_node_scan_path(node, &root.path)?;
            if seen_generated_paths.contains(&path) {
                None
            } else {
                Some(node_id)
            }
        })
        .collect();
    remove_generated_nodes(graph, &stale_ids, summary);
    Ok(())
}

fn collect_root_specs(graph: &GraphFile) -> Result<Vec<RootSpec>> {
    let mut roots = Vec::new();
    for node in &graph.nodes {
        let is_root_type = matches!(node.r#type.as_str(), "D" | "DIR");
        if is_generated_node_type(&node.r#type) || !is_root_type {
            continue;
        }
        if matches!(node.properties.scan, Some(false)) {
            continue;
        }
        let Some(path) = extract_scan_path(&node.source_files) else {
            continue;
        };
        if !path.is_dir() {
            continue;
        }
        roots.push(RootSpec {
            id: node.id.clone(),
            path: path
                .canonicalize()
                .with_context(|| format!("failed to canonicalize root path: {}", path.display()))?,
            scan_ignore_unknown: node.properties.scan_ignore_unknown.unwrap_or(true),
        });
    }
    roots.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(roots)
}

fn generated_subtree(graph: &GraphFile, root_id: &str) -> HashSet<String> {
    let node_map: HashMap<&str, &Node> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root_id.to_owned()]);

    while let Some(source_id) = queue.pop_front() {
        for edge in graph.edges.iter().filter(|edge| {
            edge.source_id == source_id
                && (edge.relation == RELATION_HAS || edge.relation == LEGACY_RELATION_HAS)
        }) {
            let Some(target) = node_map.get(edge.target_id.as_str()) else {
                continue;
            };
            if !is_generated_node_type(&target.r#type) {
                continue;
            }
            if seen.insert(target.id.clone()) {
                queue.push_back(target.id.clone());
            }
        }
    }

    seen
}

fn extract_scan_path(sources: &[String]) -> Option<PathBuf> {
    for source in sources {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.contains(' ') {
            return Some(PathBuf::from(trimmed));
        }
        if let Some(pos) = trimmed.find(' ') {
            let path_str = trimmed[pos + 1..].trim();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
        return Some(PathBuf::from(trimmed));
    }
    None
}

fn scan_tree(root: &Path, ignore_unknown: bool) -> Result<Vec<ScannedEntry>> {
    if !root.is_dir() {
        bail!("root source is not a directory: {}", root.display());
    }
    let mut out = Vec::new();
    walk_tree(root, root, ignore_unknown, &mut out)?;
    out.sort_by(|left, right| {
        left.path
            .components()
            .count()
            .cmp(&right.path.components().count())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(out)
}

fn walk_tree(
    root: &Path,
    dir: &Path,
    ignore_unknown: bool,
    out: &mut Vec<ScannedEntry>,
) -> Result<()> {
    let mut children = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read dir: {}", dir.display()))?
    {
        children.push(entry?.path());
    }
    children.sort();

    for child in children {
        if should_skip_path(&child, ignore_unknown) {
            continue;
        }
        let node_type = if child.is_dir() {
            DIR_PREFIX
        } else {
            FILE_PREFIX
        };
        out.push(ScannedEntry {
            parent_path: child.parent().unwrap_or(root).to_path_buf(),
            path: child.clone(),
            node_type,
        });
        if child.is_dir() {
            walk_tree(root, &child, ignore_unknown, out)?;
        }
    }
    Ok(())
}

fn should_skip_path(path: &Path, ignore_unknown: bool) -> bool {
    if should_skip_directory(path) {
        return true;
    }
    if path.is_file() && ignore_unknown && !is_known_scannable_file(path) {
        return true;
    }
    false
}

fn should_skip_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | "out" | ".cache"
    )
}

fn is_known_scannable_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    matches!(
        name,
        "README"
            | "README.md"
            | "README.markdown"
            | "LICENSE"
            | "COPYING"
            | "CHANGELOG.md"
            | "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "package-lock.json"
            | "pyproject.toml"
            | "Pipfile"
            | "Pipfile.lock"
            | "Gemfile"
            | "Gemfile.lock"
            | "Makefile"
            | "CMakeLists.txt"
            | "Dockerfile"
            | "justfile"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
    ) || matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some(
            "md" | "markdown"
                | "txt"
                | "rst"
                | "adoc"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "less"
                | "rs"
                | "c"
                | "h"
                | "cc"
                | "cpp"
                | "cxx"
                | "hpp"
                | "java"
                | "kt"
                | "kts"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "py"
                | "rb"
                | "go"
                | "php"
                | "swift"
                | "scala"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "sql"
                | "proto"
                | "graphql"
                | "gql"
                | "lua"
                | "cs"
                | "fs"
                | "hs"
                | "ml"
                | "elm"
                | "dart"
                | "pl"
                | "pm"
                | "ex"
                | "exs"
                | "clj"
                | "edn"
                | "r"
                | "ini"
                | "cfg"
                | "conf"
                | "csv"
                | "log"
                | "lock"
        )
    )
}

fn upsert_generated_node(
    graph: &mut GraphFile,
    node_id: &str,
    node_type: &str,
    summary: &mut AutoUpdateSummary,
) {
    if let Some(node) = graph.node_by_id_mut(node_id) {
        node.r#type = node_type.to_owned();
        node.name.clear();
        node.properties = NodeProperties::default();
        node.properties.importance = 0.0;
        node.source_files.clear();
        summary.nodes_updated += 1;
        return;
    }

    graph.nodes.push(Node {
        id: node_id.to_owned(),
        r#type: node_type.to_owned(),
        name: String::new(),
        properties: NodeProperties {
            importance: 0.0,
            ..NodeProperties::default()
        },
        source_files: Vec::new(),
    });
    summary.nodes_added += 1;
}

fn ensure_has_edge(
    graph: &mut GraphFile,
    source_id: &str,
    target_id: &str,
    summary: &mut AutoUpdateSummary,
) {
    if let Some(edge) = graph.edges.iter_mut().find(|edge| {
        edge.source_id == source_id
            && edge.target_id == target_id
            && (edge.relation == RELATION_HAS || edge.relation == LEGACY_RELATION_HAS)
    }) {
        if edge.relation != RELATION_HAS {
            edge.relation = RELATION_HAS.to_owned();
            summary.edges_added += 1;
        }
        return;
    }
    graph.edges.push(Edge {
        source_id: source_id.to_owned(),
        relation: RELATION_HAS.to_owned(),
        target_id: target_id.to_owned(),
        properties: EdgeProperties::default(),
    });
    summary.edges_added += 1;
}

fn remove_generated_nodes(
    graph: &mut GraphFile,
    stale_ids: &[String],
    summary: &mut AutoUpdateSummary,
) {
    if stale_ids.is_empty() {
        return;
    }
    let stale: HashSet<&str> = stale_ids.iter().map(String::as_str).collect();
    let notes_before = graph.notes.len();
    graph
        .notes
        .retain(|note| !stale.contains(note.node_id.as_str()));
    summary.notes_removed = notes_before.saturating_sub(graph.notes.len());
    let edges_before = graph.edges.len();
    graph.nodes.retain(|node| !stale.contains(node.id.as_str()));
    graph.edges.retain(|edge| {
        !stale.contains(edge.source_id.as_str()) && !stale.contains(edge.target_id.as_str())
    });
    summary.edges_removed = edges_before.saturating_sub(graph.edges.len());
    summary.nodes_removed = stale_ids.len();
}

fn generated_node_scan_path(node: &Node, root_path: &Path) -> Option<PathBuf> {
    if let Some(path) = extract_scan_path(&node.source_files) {
        return Some(path);
    }

    let rel = if let Some((head, suffix)) = node.id.split_once(':') {
        if is_generated_node_type(&node.r#type) && head == node.r#type {
            PathBuf::from(unescape_generated_path(suffix))
        } else {
            PathBuf::from(unescape_generated_path(&node.id))
        }
    } else {
        PathBuf::from(unescape_generated_path(&node.id))
    };
    Some(root_path.join(rel))
}

fn generated_node_id(_node_type: &str, relative_path: &Path) -> String {
    let suffix = relative_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    escape_name(&suffix)
}

fn unescape_generated_path(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '~' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('~') => out.push('~'),
            Some('c') => out.push(':'),
            Some(other) => {
                out.push('~');
                out.push(other);
            }
            None => out.push('~'),
        }
    }
    out
}

fn rename_generated_node_id(
    graph: &mut GraphFile,
    old_id: &str,
    new_id: &str,
    summary: &mut AutoUpdateSummary,
) -> Result<()> {
    if old_id == new_id {
        return Ok(());
    }
    if graph.node_by_id(new_id).is_some() {
        bail!("generated node id already exists: {new_id}");
    }

    let Some(node) = graph.node_by_id_mut(old_id) else {
        bail!("generated node not found for rename: {old_id}");
    };
    node.id = new_id.to_owned();

    for edge in &mut graph.edges {
        if edge.source_id == old_id {
            edge.source_id = new_id.to_owned();
            summary.edges_updated += 1;
        }
        if edge.target_id == old_id {
            edge.target_id = new_id.to_owned();
            summary.edges_updated += 1;
        }
    }

    for note in &mut graph.notes {
        if note.node_id == old_id {
            note.node_id = new_id.to_owned();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn extract_scan_path_no_spaces() {
        let sources = vec!["SOURCECODE /simple/path".to_string()];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/simple/path")));
    }

    #[test]
    fn extract_scan_path_with_spaces() {
        let sources = vec!["SOURCECODE /path/with spaces".to_string()];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/path/with spaces")));
    }

    #[test]
    fn extract_scan_path_no_type_prefix() {
        let sources = vec!["/simple/path".to_string()];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/simple/path")));
    }

    #[test]
    fn extract_scan_path_multiple_sources() {
        let sources = vec![
            "DOC /other/doc".to_string(),
            "SOURCECODE /target/path".to_string(),
        ];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/other/doc")));
    }

    #[test]
    fn extract_scan_path_empty_source() {
        let sources: Vec<String> = vec![];
        let result = extract_scan_path(&sources);
        assert_eq!(result, None);
    }

    #[test]
    fn extract_scan_path_first_valid() {
        let sources = vec!["".to_string(), "SOURCECODE /valid/path".to_string()];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/valid/path")));
    }

    #[test]
    fn extract_scan_path_filesystem_root() {
        let sources = vec!["SOURCECODE /".to_string()];
        let result = extract_scan_path(&sources);
        assert_eq!(result, Some(PathBuf::from("/")));
    }

    #[test]
    fn scan_tree_ignores_unknown_files_when_enabled() {
        let dir = tempdir().expect("temp dir");
        std::fs::write(dir.path().join("known.md"), b"# known").expect("write known");
        std::fs::write(dir.path().join("unknown.bin"), b"bin").expect("write unknown");
        std::fs::create_dir_all(dir.path().join("target")).expect("create target");
        std::fs::write(dir.path().join("target/ignored.txt"), b"x").expect("write target file");

        let scanned = scan_tree(dir.path(), true).expect("scan tree");
        let paths: HashSet<_> = scanned
            .into_iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        assert!(paths.contains("known.md"));
        assert!(!paths.contains("unknown.bin"));
        assert!(!paths.contains("ignored.txt"));
    }

    #[test]
    fn scan_tree_keeps_unknown_files_when_disabled() {
        let dir = tempdir().expect("temp dir");
        std::fs::write(dir.path().join("known.md"), b"# known").expect("write known");
        std::fs::write(dir.path().join("unknown.bin"), b"bin").expect("write unknown");

        let scanned = scan_tree(dir.path(), false).expect("scan tree");
        let paths: HashSet<_> = scanned
            .into_iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        assert!(paths.contains("known.md"));
        assert!(paths.contains("unknown.bin"));
    }
}
