use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::graph::{GraphFile, Node, NodeProperties, Note};
use crate::import_csv::CsvImportSummary;

#[derive(Debug, Clone, Copy)]
pub enum MarkdownStrategy {
    PreferNew,
    PreferOld,
}

#[derive(Debug, Clone)]
pub struct MarkdownImportArgs<'a> {
    pub path: &'a str,
    pub notes_as_nodes: bool,
    pub strategy: MarkdownStrategy,
}

#[derive(Debug, Deserialize, Default)]
struct Frontmatter {
    id: Option<String>,
    node_id: Option<String>,
    note_id: Option<String>,
    name: Option<String>,
    r#type: Option<String>,
    description: Option<String>,
    domain_area: Option<String>,
    provenance: Option<String>,
    confidence: Option<f64>,
    importance: Option<f64>,
    created_at: Option<String>,
    alias: Option<Vec<String>>,
    key_facts: Option<Vec<String>>,
    source_files: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    author: Option<String>,
    note: Option<bool>,
}

pub fn import_markdown_into_graph(
    graph: &mut GraphFile,
    args: MarkdownImportArgs<'_>,
) -> Result<CsvImportSummary> {
    let mut summary = CsvImportSummary::default();
    let paths = collect_markdown_paths(Path::new(args.path))?;
    for path in paths {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read markdown: {}", path.display()))?;
        let (frontmatter, body) = parse_frontmatter(&raw)
            .with_context(|| format!("failed to parse frontmatter: {}", path.display()))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("note")
            .to_owned();
        let source_file = path.to_string_lossy().to_string();

        if should_import_note(&frontmatter) {
            let note_id = frontmatter
                .note_id
                .or(frontmatter.id)
                .unwrap_or_else(|| format!("note:{stem}"));
            let node_id = frontmatter
                .node_id
                .unwrap_or_else(|| format!("note:{stem}"));
            let note = Note {
                id: note_id,
                node_id,
                body: if body.is_empty() {
                    frontmatter.description.unwrap_or_default()
                } else {
                    body
                },
                tags: frontmatter.tags.unwrap_or_default(),
                author: frontmatter.author.unwrap_or_default(),
                created_at: frontmatter.created_at.unwrap_or_default(),
                provenance: frontmatter.provenance.unwrap_or_default(),
                source_files: frontmatter
                    .source_files
                    .unwrap_or_else(|| vec![source_file]),
            };
            merge_note(graph, note, args.strategy, &mut summary);
            continue;
        }

        let id = frontmatter
            .id
            .ok_or_else(|| anyhow::anyhow!("missing id in {}", path.display()))?;
        let node_type = frontmatter.r#type.unwrap_or_else(|| "Note".to_owned());
        let name = frontmatter.name.unwrap_or_else(|| id.clone());
        let mut node = Node {
            id,
            r#type: node_type,
            name,
            properties: NodeProperties::default(),
            source_files: frontmatter
                .source_files
                .unwrap_or_else(|| vec![source_file]),
        };
        node.properties.description = frontmatter.description.unwrap_or_default();
        node.properties.domain_area = frontmatter.domain_area.unwrap_or_default();
        node.properties.provenance = frontmatter.provenance.unwrap_or_default();
        node.properties.confidence = frontmatter.confidence;
        node.properties.importance = frontmatter.importance.unwrap_or(0.5);
        node.properties.created_at = frontmatter.created_at.unwrap_or_default();
        node.properties.alias = frontmatter.alias.unwrap_or_default();
        node.properties.key_facts = frontmatter.key_facts.unwrap_or_default();

        if args.notes_as_nodes && !body.is_empty() {
            node.properties.key_facts.push(body);
        }

        merge_node(graph, node, args.strategy, &mut summary);
    }
    graph.refresh_counts();
    Ok(summary)
}

fn should_import_note(frontmatter: &Frontmatter) -> bool {
    frontmatter.note.unwrap_or(false)
        || frontmatter.node_id.is_some()
        || frontmatter.note_id.is_some()
}

fn parse_frontmatter(raw: &str) -> Result<(Frontmatter, String)> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((Frontmatter::default(), raw.trim().to_owned()));
    }
    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or("---");
    if first.trim() != "---" {
        return Ok((Frontmatter::default(), raw.trim().to_owned()));
    }
    let mut frontmatter_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_frontmatter = true;
    for line in lines {
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter {
            frontmatter_lines.push(line);
        } else {
            body_lines.push(line);
        }
    }
    let frontmatter_raw = frontmatter_lines.join("\n");
    let frontmatter = if frontmatter_raw.trim().is_empty() {
        Frontmatter::default()
    } else {
        serde_yaml::from_str(&frontmatter_raw).context("invalid YAML frontmatter")?
    };
    Ok((frontmatter, body_lines.join("\n").trim().to_owned()))
}

fn collect_markdown_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("path not found: {}", path.display());
    }
    let mut out = Vec::new();
    walk_dir(path, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_dir(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            walk_dir(&entry_path, out)?;
        } else if entry_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
        {
            out.push(entry_path);
        }
    }
    Ok(())
}

fn merge_node(
    graph: &mut GraphFile,
    node: Node,
    strategy: MarkdownStrategy,
    summary: &mut CsvImportSummary,
) {
    let prefer_new = matches!(strategy, MarkdownStrategy::PreferNew);
    if let Some(existing) = graph.node_by_id_mut(&node.id) {
        if prefer_new {
            *existing = node;
            summary.nodes_updated += 1;
        } else {
            if differs(existing, &node) {
                summary.conflicts.push(format!("node {}", node.id));
            }
            summary.nodes_skipped += 1;
        }
    } else {
        graph.nodes.push(node);
        summary.nodes_added += 1;
    }
}

fn merge_note(
    graph: &mut GraphFile,
    note: Note,
    strategy: MarkdownStrategy,
    summary: &mut CsvImportSummary,
) {
    let prefer_new = matches!(strategy, MarkdownStrategy::PreferNew);
    if let Some(idx) = graph.notes.iter().position(|n| n.id == note.id) {
        if prefer_new {
            graph.notes[idx] = note;
            summary.notes_updated += 1;
        } else {
            if differs(&graph.notes[idx], &note) {
                summary.conflicts.push(format!("note {}", note.id));
            }
            summary.notes_skipped += 1;
        }
    } else {
        graph.notes.push(note);
        summary.notes_added += 1;
    }
}

fn differs<T: serde::Serialize>(left: &T, right: &T) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(l), Ok(r)) => l != r,
        _ => true,
    }
}
