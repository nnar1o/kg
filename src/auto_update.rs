use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::code_symbols::extract_code_symbols;
use crate::document_sections::{is_document_source, parse_document_sections};
use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};
use crate::validate::is_generated_node_type;

const FILE_PREFIX: &str = "GFIL";
const DIR_PREFIX: &str = "GDIR";
const DOC_PREFIX: &str = "GDOC";
const SECTION_PREFIX: &str = "GSEC";
const SYMBOL_PREFIX: &str = "GSYM";
const RELATION_HAS: &str = "GHAS";
const RELATION_DEFINES: &str = "GDEF";
const LEGACY_RELATION_HAS: &str = "HAS";
const LEGACY_RELATION_DEFINES: &str = "GDEFINES";

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

#[derive(Debug, Default)]
struct GeneratedEdgeIndex {
    by_key: HashMap<(String, String, String), usize>,
}

impl GeneratedEdgeIndex {
    fn from_graph(graph: &GraphFile) -> Self {
        let mut index = Self::default();
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            let key = (
                edge.source_id.clone(),
                edge.target_id.clone(),
                canonical_generated_relation(&edge.relation).to_owned(),
            );
            index.by_key.entry(key).or_insert(edge_index);
        }
        index
    }

    fn lookup(&self, source_id: &str, target_id: &str, relation: &str) -> Option<usize> {
        self.by_key
            .get(&(source_id.to_owned(), target_id.to_owned(), relation.to_owned()))
            .copied()
    }

    fn insert(&mut self, source_id: &str, target_id: &str, relation: &str, edge_index: usize) {
        self.by_key.entry((source_id.to_owned(), target_id.to_owned(), relation.to_owned())).or_insert(edge_index);
    }
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
    let mut edge_index = GeneratedEdgeIndex::from_graph(graph);
    let current_generated = generated_subtree(graph, &root.id);
    let generated_nodes_by_path: HashMap<PathBuf, ExistingGeneratedNode> = current_generated
        .iter()
        .filter_map(|node_id| {
            let node = graph.node_by_id(node_id)?;
            if !is_generated_path_node(&node.r#type) {
                return None;
            }
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
    let mut seen_generated_ids = HashSet::new();

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
                let old_id = existing.id.clone();
                rename_generated_node_id(graph, &existing.id, &target_id, summary)?;
                seen_generated_ids.insert(old_id);
            }
            target_id.clone()
        } else {
            target_id.clone()
        };

        upsert_generated_node(graph, &node_id, entry.node_type, summary)?;
        ensure_generated_edge(
            graph,
            &parent_id,
            &node_id,
            RELATION_HAS,
            &mut edge_index,
            summary,
        );
        seen_generated_ids.insert(node_id.clone());

        if entry.node_type == FILE_PREFIX {
            let source_text = fs::read_to_string(&entry.path).ok();

            if is_document_source(&entry.path) {
                if let Some(raw) = source_text.as_deref() {
                    let sections = parse_document_sections(raw);
                    let doc_id = generated_document_id(&relative_path);
                    migrate_legacy_generated_node_id(
                        graph,
                        &legacy_generated_document_id(&relative_path),
                        &doc_id,
                        &mut seen_generated_ids,
                        summary,
                    )?;
                    let doc_name = entry
                        .path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("document");
                    upsert_generated_text_node(
                        graph,
                        &doc_id,
                        DOC_PREFIX,
                        doc_name,
                        &format!("{} section(s)", sections.len()),
                        vec![format!("sections={}", sections.len())],
                        &entry.path,
                        summary,
                    )?;
                    ensure_generated_edge(
                        graph,
                        &node_id,
                        &doc_id,
                        RELATION_HAS,
                        &mut edge_index,
                        summary,
                    );
                    seen_generated_ids.insert(doc_id.clone());

                    for section in sections {
                        let section_id = generated_section_id(&relative_path, &section.id_path);
                        migrate_legacy_generated_node_id(
                            graph,
                            &legacy_generated_section_id(&relative_path, &section.legacy_id_path),
                            &section_id,
                            &mut seen_generated_ids,
                            summary,
                        )?;
                        upsert_generated_text_node(
                            graph,
                            &section_id,
                            SECTION_PREFIX,
                            &section.title,
                            &section.content,
                            vec![format!("level={}", section.level)],
                            &entry.path,
                            summary,
                        )?;
                        let parent_id = if section.id_path.len() > 1 {
                            generated_section_id(
                                &relative_path,
                                &section.id_path[..section.id_path.len() - 1],
                            )
                        } else {
                            doc_id.clone()
                        };
                        ensure_generated_edge(
                            graph,
                            &parent_id,
                            &section_id,
                            RELATION_HAS,
                            &mut edge_index,
                            summary,
                        );
                        seen_generated_ids.insert(section_id);
                    }
                }
            }

            if let Some(raw) = source_text.as_deref() {
                for symbol in extract_code_symbols(&entry.path, raw)? {
                    let symbol_id = generated_symbol_id(&relative_path, &symbol.kind, &symbol.name);
                    migrate_legacy_generated_node_id(
                        graph,
                        &legacy_generated_symbol_id(&relative_path, &symbol.kind, &symbol.name),
                        &symbol_id,
                        &mut seen_generated_ids,
                        summary,
                    )?;
                    upsert_generated_symbol_node(
                        graph,
                        &symbol_id,
                        &symbol.name,
                        &entry.path,
                        summary,
                    )?;
                    ensure_generated_edge(
                        graph,
                        &node_id,
                        &symbol_id,
                        RELATION_DEFINES,
                        &mut edge_index,
                        summary,
                    );
                    seen_generated_ids.insert(symbol_id);
                }
            }
        }

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
    let mut children_by_source: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if !matches!(
            edge.relation.as_str(),
            RELATION_HAS | RELATION_DEFINES | LEGACY_RELATION_HAS | LEGACY_RELATION_DEFINES
        ) {
            continue;
        }
        children_by_source
            .entry(edge.source_id.as_str())
            .or_default()
            .push(edge.target_id.as_str());
    }

    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root_id.to_owned()]);

    while let Some(source_id) = queue.pop_front() {
        let Some(target_ids) = children_by_source.get(source_id.as_str()) else {
            continue;
        };
        for target_id in target_ids {
            let Some(target) = node_map.get(target_id) else {
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
) -> Result<()> {
    if let Some(node) = graph.node_by_id_mut(node_id) {
        if !is_generated_node_type(&node.r#type) {
            bail!("generated node id collides with an existing non-generated node: {node_id}");
        }
        node.r#type = node_type.to_owned();
        node.name.clear();
        node.properties = NodeProperties::default();
        node.properties.importance = 0.0;
        node.source_files.clear();
        summary.nodes_updated += 1;
        return Ok(());
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
    Ok(())
}

fn upsert_generated_text_node(
    graph: &mut GraphFile,
    node_id: &str,
    node_type: &str,
    name: &str,
    description: &str,
    key_facts: Vec<String>,
    source_path: &Path,
    summary: &mut AutoUpdateSummary,
) -> Result<()> {
    let source_ref = source_reference(source_path);
    if let Some(node) = graph.node_by_id_mut(node_id) {
        if !is_generated_node_type(&node.r#type) {
            bail!("generated node id collides with an existing non-generated node: {node_id}");
        }
        node.r#type = node_type.to_owned();
        node.name = name.to_owned();
        node.properties = NodeProperties::default();
        node.properties.importance = 0.0;
        node.properties.description = description.to_owned();
        node.properties.key_facts = key_facts;
        node.source_files = vec![source_ref];
        summary.nodes_updated += 1;
        return Ok(());
    }

    graph.nodes.push(Node {
        id: node_id.to_owned(),
        r#type: node_type.to_owned(),
        name: name.to_owned(),
        properties: NodeProperties {
            importance: 0.0,
            description: description.to_owned(),
            key_facts,
            ..NodeProperties::default()
        },
        source_files: vec![source_ref],
    });
    summary.nodes_added += 1;
    Ok(())
}

fn upsert_generated_symbol_node(
    graph: &mut GraphFile,
    node_id: &str,
    name: &str,
    source_path: &Path,
    summary: &mut AutoUpdateSummary,
) -> Result<()> {
    let source_ref = source_reference(source_path);
    if let Some(node) = graph.node_by_id_mut(node_id) {
        if !is_generated_node_type(&node.r#type) {
            bail!("generated node id collides with an existing non-generated node: {node_id}");
        }
        node.r#type = SYMBOL_PREFIX.to_owned();
        node.name = name.to_owned();
        node.properties = NodeProperties::default();
        node.properties.importance = 0.0;
        node.source_files = vec![source_ref];
        summary.nodes_updated += 1;
        return Ok(());
    }

    graph.nodes.push(Node {
        id: node_id.to_owned(),
        r#type: SYMBOL_PREFIX.to_owned(),
        name: name.to_owned(),
        properties: NodeProperties {
            importance: 0.0,
            ..NodeProperties::default()
        },
        source_files: vec![source_ref],
    });
    summary.nodes_added += 1;
    Ok(())
}

fn ensure_generated_edge(
    graph: &mut GraphFile,
    source_id: &str,
    target_id: &str,
    relation: &str,
    edge_index: &mut GeneratedEdgeIndex,
    summary: &mut AutoUpdateSummary,
) {
    let relation = canonical_generated_relation(relation);
    if let Some(edge_index_pos) = edge_index.lookup(source_id, target_id, relation) {
        let edge = &mut graph.edges[edge_index_pos];
        if edge.relation != relation {
            edge.relation = relation.to_owned();
            summary.edges_added += 1;
        }
        return;
    }

    if let Some((edge_index_pos, edge)) = graph.edges.iter_mut().enumerate().find(|(_, edge)| {
        edge.source_id == source_id
            && edge.target_id == target_id
            && match relation {
                RELATION_HAS => edge.relation == relation || edge.relation == LEGACY_RELATION_HAS,
                RELATION_DEFINES => {
                    edge.relation == relation || edge.relation == LEGACY_RELATION_DEFINES
                }
                _ => edge.relation == relation,
            }
    }) {
        if edge.relation != relation {
            edge.relation = relation.to_owned();
            summary.edges_added += 1;
        }
        edge_index.insert(source_id, target_id, relation, edge_index_pos);
        return;
    }

    graph.edges.push(Edge {
        source_id: source_id.to_owned(),
        relation: relation.to_owned(),
        target_id: target_id.to_owned(),
        properties: EdgeProperties::default(),
    });
    edge_index.insert(source_id, target_id, relation, graph.edges.len() - 1);
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

fn canonical_generated_relation(relation: &str) -> &str {
    match relation {
        RELATION_HAS | LEGACY_RELATION_HAS => RELATION_HAS,
        RELATION_DEFINES | LEGACY_RELATION_DEFINES => RELATION_DEFINES,
        _ => relation,
    }
}

fn is_generated_path_node(node_type: &str) -> bool {
    matches!(node_type, FILE_PREFIX | DIR_PREFIX)
}

fn generated_node_id(_node_type: &str, relative_path: &Path) -> String {
    format!("{}:{}", _node_type, generated_path_suffix(relative_path))
}

fn generated_document_id(relative_path: &Path) -> String {
    format!("{}:{}", DOC_PREFIX, generated_path_suffix(relative_path))
}

fn generated_section_id(relative_path: &Path, id_path: &[String]) -> String {
    format!(
        "{}:{}/__section__/{}",
        SECTION_PREFIX,
        generated_path_suffix(relative_path),
        escape_name(&id_path.join("/"))
    )
}

fn generated_symbol_id(relative_path: &Path, kind: &str, name: &str) -> String {
    let path = generated_path_suffix(relative_path);
    format!(
        "{}:{}/{}/{}",
        SYMBOL_PREFIX,
        escape_name(&path),
        escape_name(kind),
        escape_name(name)
    )
}

fn legacy_generated_document_id(relative_path: &Path) -> String {
    format!("__doc__/{}", generated_path_suffix(relative_path))
}

fn legacy_generated_section_id(relative_path: &Path, id_path: &[String]) -> String {
    format!(
        "__doc__/{}/__section__/{}",
        generated_path_suffix(relative_path),
        id_path.join("/")
    )
}

fn legacy_generated_symbol_id(relative_path: &Path, kind: &str, name: &str) -> String {
    let path = generated_path_suffix(relative_path);
    format!("{}~c{}~c{}", escape_name(&path), escape_name(kind), escape_name(name))
}

fn migrate_legacy_generated_node_id(
    graph: &mut GraphFile,
    legacy_id: &str,
    new_id: &str,
    seen_generated_ids: &mut HashSet<String>,
    summary: &mut AutoUpdateSummary,
) -> Result<()> {
    if legacy_id == new_id || graph.node_by_id(new_id).is_some() {
        return Ok(());
    }

    let Some(node) = graph.node_by_id(legacy_id) else {
        return Ok(());
    };
    if !is_generated_node_type(&node.r#type) {
        return Ok(());
    }

    rename_generated_node_id(graph, legacy_id, new_id, summary)?;
    seen_generated_ids.insert(legacy_id.to_owned());
    Ok(())
}

fn generated_path_suffix(relative_path: &Path) -> String {
    let suffix = relative_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    escape_name(&suffix)
}

fn source_reference(path: &Path) -> String {
    format!("SOURCECODE {}", path.display())
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

    #[test]
    fn auto_update_graph_extracts_rust_symbols() {
        let dir = tempdir().expect("temp dir");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub fn hello() {}\npub struct World;\n",
        )
        .expect("write rust source");

        let mut graph = GraphFile::new("repo");
        graph.nodes.push(Node {
            id: "DIR:repo".to_owned(),
            r#type: "DIR".to_owned(),
            name: "repo".to_owned(),
            properties: NodeProperties {
                scan: Some(true),
                ..Default::default()
            },
            source_files: vec![format!("SOURCECODE {}", dir.path().display())],
        });

        let summary = auto_update_graph(&mut graph).expect("auto update");
        assert!(summary.nodes_added >= 3);

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.r#type == SYMBOL_PREFIX && node.name == "hello"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.r#type == SYMBOL_PREFIX && node.name == "World"));
        assert!(graph.edges.iter().any(|edge| edge.relation == RELATION_DEFINES));
    }

    #[test]
    fn auto_update_graph_migrates_legacy_defines_edges() {
        let dir = tempdir().expect("temp dir");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(src_dir.join("lib.rs"), "pub fn hello() {}\n")
            .expect("write rust source");

        let mut graph = GraphFile::new("repo");
        graph.nodes.push(Node {
            id: "DIR:repo".to_owned(),
            r#type: "DIR".to_owned(),
            name: "repo".to_owned(),
            properties: NodeProperties {
                scan: Some(true),
                ..Default::default()
            },
            source_files: vec![format!("SOURCECODE {}", dir.path().display())],
        });

        let file_id = generated_node_id(FILE_PREFIX, std::path::Path::new("src/lib.rs"));
        let symbol_id =
            generated_symbol_id(std::path::Path::new("src/lib.rs"), "fn", "hello");
        graph.nodes.push(Node {
            id: file_id.clone(),
            r#type: FILE_PREFIX.to_owned(),
            name: String::new(),
            properties: NodeProperties::default(),
            source_files: vec![source_reference(&src_dir.join("lib.rs"))],
        });
        graph.nodes.push(Node {
            id: symbol_id.clone(),
            r#type: SYMBOL_PREFIX.to_owned(),
            name: "hello".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![source_reference(&src_dir.join("lib.rs"))],
        });
        graph.edges.push(Edge {
            source_id: file_id,
            relation: LEGACY_RELATION_DEFINES.to_owned(),
            target_id: symbol_id,
            properties: EdgeProperties::default(),
        });

        auto_update_graph(&mut graph).expect("auto update");

        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.relation == RELATION_DEFINES));
        assert!(!graph
            .edges
            .iter()
            .any(|edge| edge.relation == LEGACY_RELATION_DEFINES));
    }

    #[test]
    fn auto_update_graph_migrates_legacy_section_ids() {
        let dir = tempdir().expect("temp dir");
        std::fs::write(dir.path().join("README.md"), "# A+B\nBody\n").expect("write doc");

        let mut graph = GraphFile::new("repo");
        graph.nodes.push(Node {
            id: "DIR:repo".to_owned(),
            r#type: "DIR".to_owned(),
            name: "repo".to_owned(),
            properties: NodeProperties {
                scan: Some(true),
                ..Default::default()
            },
            source_files: vec![format!("SOURCECODE {}", dir.path().display())],
        });

        let legacy_section_id = "__doc__/README.md/__section__/a_b".to_owned();
        graph.nodes.push(Node {
            id: legacy_section_id.clone(),
            r#type: SECTION_PREFIX.to_owned(),
            name: "A+B".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec![source_reference(&dir.path().join("README.md"))],
        });

        auto_update_graph(&mut graph).expect("auto update");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "GSEC:README.md/__section__/A~~2BB"));
        assert!(!graph.nodes.iter().any(|node| node.id == legacy_section_id));
    }

    #[test]
    fn generated_subtree_collects_reachable_generated_nodes() {
        let mut graph = GraphFile::new("repo");
        graph.nodes.push(Node {
            id: "D:repo".to_owned(),
            r#type: "D".to_owned(),
            name: "repo".to_owned(),
            properties: NodeProperties::default(),
            source_files: vec!["SOURCECODE /tmp/repo".to_owned()],
        });
        graph.nodes.push(Node {
            id: "GDIR:src".to_owned(),
            r#type: DIR_PREFIX.to_owned(),
            name: String::new(),
            properties: NodeProperties::default(),
            source_files: Vec::new(),
        });
        graph.nodes.push(Node {
            id: "GFIL:src/lib.rs".to_owned(),
            r#type: FILE_PREFIX.to_owned(),
            name: String::new(),
            properties: NodeProperties::default(),
            source_files: Vec::new(),
        });
        graph.nodes.push(Node {
            id: "concept:manual".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Manual".to_owned(),
            properties: NodeProperties::default(),
            source_files: Vec::new(),
        });
        graph.edges.push(Edge {
            source_id: "D:repo".to_owned(),
            relation: RELATION_HAS.to_owned(),
            target_id: "GDIR:src".to_owned(),
            properties: EdgeProperties::default(),
        });
        graph.edges.push(Edge {
            source_id: "GDIR:src".to_owned(),
            relation: LEGACY_RELATION_HAS.to_owned(),
            target_id: "GFIL:src/lib.rs".to_owned(),
            properties: EdgeProperties::default(),
        });
        graph.edges.push(Edge {
            source_id: "GDIR:src".to_owned(),
            relation: RELATION_HAS.to_owned(),
            target_id: "concept:manual".to_owned(),
            properties: EdgeProperties::default(),
        });

        let subtree = generated_subtree(&graph, "D:repo");
        assert!(subtree.contains("GDIR:src"));
        assert!(subtree.contains("GFIL:src/lib.rs"));
        assert!(!subtree.contains("concept:manual"));
    }

    #[test]
    fn ensure_generated_edge_reuses_local_index() {
        let mut graph = GraphFile::new("repo");
        graph.edges.push(Edge {
            source_id: "D:repo".to_owned(),
            relation: LEGACY_RELATION_HAS.to_owned(),
            target_id: "GDIR:src".to_owned(),
            properties: EdgeProperties::default(),
        });

        let mut edge_index = GeneratedEdgeIndex::from_graph(&graph);
        let mut summary = AutoUpdateSummary::default();

        ensure_generated_edge(
            &mut graph,
            "D:repo",
            "GDIR:src",
            RELATION_HAS,
            &mut edge_index,
            &mut summary,
        );
        ensure_generated_edge(
            &mut graph,
            "D:repo",
            "GDIR:src",
            RELATION_HAS,
            &mut edge_index,
            &mut summary,
        );

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].relation, RELATION_HAS);
    }
}
