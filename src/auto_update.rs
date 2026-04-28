use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};

const FILE_PREFIX: &str = "F";
const DIR_PREFIX: &str = "D";
const PROVENANCE_AUTO: &str = "G";
const RELATION_HAS: &str = "HAS";

#[derive(Debug, Default)]
pub struct AutoUpdateSummary {
    pub roots_updated: usize,
    pub nodes_added: usize,
    pub nodes_updated: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
    pub notes_removed: usize,
}

#[derive(Debug, Clone)]
struct RootSpec {
    id: String,
    path: PathBuf,
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
            "no root D nodes with a filesystem source found; add one with `kg graph <name> node add D:<name> --type D --name <name> --source \"SOURCECODE /abs/path\"`"
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

fn update_root(graph: &mut GraphFile, root: &RootSpec, summary: &mut AutoUpdateSummary) -> Result<()> {
    let current_generated = generated_subtree(graph, &root.id);
    let generated_nodes_by_path: HashMap<PathBuf, ExistingGeneratedNode> = current_generated
        .iter()
        .filter_map(|node_id| {
            let node = graph.node_by_id(node_id)?;
            extract_scan_path(&node.source_files).map(|path| {
                (path, ExistingGeneratedNode { id: node.id.clone() })
            })
        })
        .collect();

    let scanned = scan_tree(&root.path)?;
    let mut all_ids: HashSet<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    let mut dir_ids_by_path = HashMap::from([(root.path.clone(), root.id.clone())]);
    let mut seen_generated_ids = HashSet::new();

    for entry in scanned {
        let parent_id = dir_ids_by_path.get(&entry.parent_path).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "missing parent directory mapping for {}",
                entry.parent_path.display()
            )
        })?;

        let node_id = if let Some(existing) = generated_nodes_by_path.get(&entry.path) {
            existing.id.clone()
        } else {
            let generated = generate_unique_id(entry.node_type, file_name(&entry.path), &all_ids);
            all_ids.insert(generated.clone());
            generated
        };

        upsert_generated_node(graph, &node_id, entry.node_type, &entry.path, summary);
        ensure_has_edge(graph, &parent_id, &node_id, summary);

        seen_generated_ids.insert(node_id.clone());
        if entry.node_type == DIR_PREFIX {
            dir_ids_by_path.insert(entry.path.clone(), node_id);
        }
    }

    let stale_ids: Vec<String> = current_generated
        .into_iter()
        .filter(|node_id| !seen_generated_ids.contains(node_id))
        .collect();
    remove_generated_nodes(graph, &stale_ids, summary);
    Ok(())
}

fn collect_root_specs(graph: &GraphFile) -> Result<Vec<RootSpec>> {
    let mut roots = Vec::new();
    for node in &graph.nodes {
        if node.properties.provenance == PROVENANCE_AUTO || !node.id.starts_with("D:") {
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
        });
    }
    roots.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(roots)
}

fn generated_subtree(graph: &GraphFile, root_id: &str) -> HashSet<String> {
    let node_map: HashMap<&str, &Node> = graph.nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root_id.to_owned()]);

    while let Some(source_id) = queue.pop_front() {
        for edge in graph.edges.iter().filter(|edge| edge.source_id == source_id && edge.relation == RELATION_HAS) {
            let Some(target) = node_map.get(edge.target_id.as_str()) else {
                continue;
            };
            if target.properties.provenance != PROVENANCE_AUTO {
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
            let path_str = trimmed[pos+1..].trim();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
        return Some(PathBuf::from(trimmed));
    }
    None
}

fn scan_tree(root: &Path) -> Result<Vec<ScannedEntry>> {
    if !root.is_dir() {
        bail!("root source is not a directory: {}", root.display());
    }
    let mut out = Vec::new();
    walk_tree(root, root, &mut out)?;
    out.sort_by(|left, right| {
        left.path
            .components()
            .count()
            .cmp(&right.path.components().count())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(out)
}

fn walk_tree(root: &Path, dir: &Path, out: &mut Vec<ScannedEntry>) -> Result<()> {
    let mut children = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read dir: {}", dir.display()))? {
        children.push(entry?.path());
    }
    children.sort();

    for child in children {
        let node_type = if child.is_dir() { DIR_PREFIX } else { FILE_PREFIX };
        out.push(ScannedEntry {
            parent_path: child.parent().unwrap_or(root).to_path_buf(),
            path: child.clone(),
            node_type,
        });
        if child.is_dir() {
            walk_tree(root, &child, out)?;
        }
    }
    Ok(())
}

fn upsert_generated_node(
    graph: &mut GraphFile,
    node_id: &str,
    node_type: &str,
    path: &Path,
    summary: &mut AutoUpdateSummary,
) {
    let source = format!("SOURCECODE {}", path.display());
    if let Some(node) = graph.node_by_id_mut(node_id) {
        node.r#type = node_type.to_owned();
        node.name.clear();
        node.properties.provenance = PROVENANCE_AUTO.to_owned();
        node.source_files = vec![source];
        summary.nodes_updated += 1;
        return;
    }

    graph.nodes.push(Node {
        id: node_id.to_owned(),
        r#type: node_type.to_owned(),
        name: String::new(),
        properties: NodeProperties {
            provenance: PROVENANCE_AUTO.to_owned(),
            ..NodeProperties::default()
        },
        source_files: vec![source],
    });
    summary.nodes_added += 1;
}

fn ensure_has_edge(graph: &mut GraphFile, source_id: &str, target_id: &str, summary: &mut AutoUpdateSummary) {
    let exists = graph.edges.iter().any(|edge| {
        edge.source_id == source_id && edge.relation == RELATION_HAS && edge.target_id == target_id
    });
    if exists {
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

fn remove_generated_nodes(graph: &mut GraphFile, stale_ids: &[String], summary: &mut AutoUpdateSummary) {
    if stale_ids.is_empty() {
        return;
    }
    let stale: HashSet<&str> = stale_ids.iter().map(String::as_str).collect();
    let notes_before = graph.notes.len();
    graph.notes.retain(|note| !stale.contains(note.node_id.as_str()));
    summary.notes_removed = notes_before.saturating_sub(graph.notes.len());
    let edges_before = graph.edges.len();
    graph.nodes.retain(|node| !stale.contains(node.id.as_str()));
    graph.edges.retain(|edge| {
        !stale.contains(edge.source_id.as_str()) && !stale.contains(edge.target_id.as_str())
    });
    summary.edges_removed = edges_before.saturating_sub(graph.edges.len());
    summary.nodes_removed = stale_ids.len();
}

fn generate_unique_id(prefix: &str, raw_name: &str, used_ids: &HashSet<String>) -> String {
    let escaped = escape_name(raw_name);
    let base = format!("{prefix}:{escaped}");
    if !used_ids.contains(&base) {
        return base;
    }

    let mut idx = 1usize;
    loop {
        let candidate = format!("{base}:{idx}");
        if !used_ids.contains(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
}
