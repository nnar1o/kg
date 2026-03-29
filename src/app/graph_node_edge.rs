use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use csv::ReaderBuilder;

use crate::access_log;
use crate::cli::{
    AddEdgeArgs, AddEdgeBatchArgs, AddNodeArgs, EdgeCommand, ModifyNodeArgs, NodeCommand,
    RemoveEdgeArgs,
};
use crate::graph::{Edge, EdgeProperties, GraphFile, Node, NodeProperties};
use crate::ops::{add_edge, add_node, modify_node, remove_edge, remove_node};
use crate::output;
use crate::schema::GraphSchema;
use crate::storage::{GraphStore, load_graph_index};
use crate::vectors;

pub(crate) struct GraphCommandContext<'a> {
    pub(crate) graph_name: &'a str,
    pub(crate) path: &'a Path,
    pub(crate) graph_file: &'a mut GraphFile,
    pub(crate) schema: Option<&'a GraphSchema>,
    pub(crate) store: &'a dyn GraphStore,
}

pub(crate) fn execute_node(
    command: NodeCommand,
    context: GraphCommandContext<'_>,
) -> Result<String> {
    match command {
        NodeCommand::Find {
            queries,
            limit,
            include_features,
            mode,
            full,
            json,
            vector_query,
        } => {
            if mode == crate::cli::FindMode::Vector {
                let result = if let Some(query_vec) = vector_query {
                    let vector_path = context
                        .path
                        .parent()
                        .map(|parent| parent.join(".kg.vectors.json"))
                        .unwrap_or_else(|| PathBuf::from(".kg.vectors.json"));
                    if !vector_path.exists() {
                        anyhow::bail!(
                            "vector store not found. Run: kg {} vectors import --input <file.jsonl>",
                            context.graph_name
                        );
                    }
                    let vector_store = vectors::VectorStore::load(&vector_path)?;
                    let node_ids: Vec<_> = context
                        .graph_file
                        .nodes
                        .iter()
                        .map(|node| node.id.clone())
                        .collect();
                    let results = vector_store.search(&query_vec, &node_ids, limit, 0.0);
                    let mut lines = vec![format!("= vector-search ({} results)", results.len())];
                    for (node_id, score) in &results {
                        if let Some(node) = context.graph_file.node_by_id(node_id) {
                            lines.push(format!(
                                "# {} | {} [{}] ({:.3})",
                                node.id, node.name, node.r#type, score
                            ));
                        }
                    }
                    format!("{}\n", lines.join("\n"))
                } else {
                    anyhow::bail!("--vector-query required for --mode vector")
                };
                return Ok(result);
            }

            let bm25_index = if mode == crate::cli::FindMode::Bm25 {
                load_graph_index(context.path).ok().flatten()
            } else {
                None
            };

            let timer = access_log::Timer::new();
            let results_count = output::count_find_results_with_index(
                context.graph_file,
                &queries,
                limit,
                include_features,
                crate::map_find_mode(mode),
                bm25_index.as_ref(),
            );
            let result = if json {
                crate::render_find_json_with_index(
                    context.graph_file,
                    &queries,
                    limit,
                    include_features,
                    crate::map_find_mode(mode),
                    bm25_index.as_ref(),
                )
            } else {
                output::render_find_with_index(
                    context.graph_file,
                    &queries,
                    limit,
                    include_features,
                    crate::map_find_mode(mode),
                    full,
                    bm25_index.as_ref(),
                )
            };
            let duration_ms = timer.elapsed_ms();

            for query in &queries {
                let entry =
                    access_log::AccessLogEntry::new(query.clone(), results_count, duration_ms);
                if let Err(error) = access_log::append_entry(context.path, &entry) {
                    eprintln!("warning: failed to log access: {}", error);
                }
            }

            Ok(result)
        }

        NodeCommand::Get {
            id,
            include_features,
            full,
            json,
        } => {
            let timer = access_log::Timer::new();
            let node = context
                .graph_file
                .node_by_id(&id)
                .ok_or_else(|| anyhow!("node not found: {id}"))?;
            if !include_features && node.r#type == "Feature" {
                bail!("feature nodes are hidden by default; use --include-features");
            }
            let result = if json {
                crate::render_node_json(node)
            } else {
                output::render_node(context.graph_file, node, full)
            };

            let duration_ms = timer.elapsed_ms();
            let entry = access_log::AccessLogEntry::node_get(id, duration_ms);
            if let Err(error) = access_log::append_entry(context.path, &entry) {
                eprintln!("warning: failed to log access: {}", error);
            }

            Ok(result)
        }

        NodeCommand::Add(AddNodeArgs {
            id,
            node_type,
            name,
            description,
            domain_area,
            provenance,
            confidence,
            created_at,
            fact,
            alias,
            source,
        }) => {
            let node = Node {
                id,
                r#type: node_type,
                name,
                properties: NodeProperties {
                    description,
                    domain_area,
                    provenance,
                    confidence,
                    created_at,
                    key_facts: fact,
                    alias,
                    ..NodeProperties::default()
                },
                source_files: source,
            };
            if let Some(schema) = context.schema {
                let violations = schema.validate_node_add(&node);
                crate::bail_on_schema_violations(&violations)?;
            }
            add_node(context.graph_file, node)?;
            context.store.save_graph(context.path, context.graph_file)?;

            let node_id = context
                .graph_file
                .nodes
                .last()
                .map(|node| node.id.clone())
                .ok_or_else(|| anyhow!("node not persisted after add"))?;
            crate::append_event_snapshot(
                context.path,
                "node.add",
                Some(node_id.clone()),
                context.graph_file,
            )?;
            Ok(format!("+ node {node_id}\n"))
        }

        NodeCommand::Modify(ModifyNodeArgs {
            id,
            node_type,
            name,
            description,
            domain_area,
            provenance,
            confidence,
            created_at,
            fact,
            alias,
            source,
        }) => {
            modify_node(
                context.graph_file,
                &id,
                node_type.clone(),
                name.clone(),
                description.clone(),
                domain_area.clone(),
                provenance.clone(),
                confidence,
                created_at.clone(),
                fact.clone(),
                alias.clone(),
                source.clone(),
            )?;
            if let Some(schema) = context.schema {
                if let Some(node) = context.graph_file.node_by_id(&id) {
                    let violations = schema.validate_node_add(node);
                    crate::bail_on_schema_violations(&violations)?;
                }
            }
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "node.modify",
                Some(id.clone()),
                context.graph_file,
            )?;
            Ok(format!("~ node {id}\n"))
        }

        NodeCommand::Rename { from, to } => {
            if context.graph_file.node_by_id(&to).is_some() {
                bail!("node already exists: {to}");
            }
            let Some(node) = context.graph_file.node_by_id_mut(&from) else {
                bail!("node not found: {from}");
            };
            node.id = to.clone();
            for edge in &mut context.graph_file.edges {
                if edge.source_id == from {
                    edge.source_id = to.clone();
                }
                if edge.target_id == from {
                    edge.target_id = to.clone();
                }
            }
            for note in &mut context.graph_file.notes {
                if note.node_id == from {
                    note.node_id = to.clone();
                }
            }
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "node.rename",
                Some(format!("{from} -> {to}")),
                context.graph_file,
            )?;
            Ok(format!("~ node {from} -> {to}\n"))
        }

        NodeCommand::Remove { id } => {
            let removed_edges = remove_node(context.graph_file, &id)?;
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "node.remove",
                Some(id.clone()),
                context.graph_file,
            )?;
            Ok(format!("- node {id} ({removed_edges} edges removed)\n"))
        }
        NodeCommand::List(args) => Ok(if args.json {
            crate::render_node_list_json(context.graph_file, &args)
        } else {
            crate::render_node_list(context.graph_file, &args)
        }),
    }
}

pub(crate) fn execute_edge(
    command: EdgeCommand,
    context: GraphCommandContext<'_>,
) -> Result<String> {
    match command {
        EdgeCommand::Add(AddEdgeArgs {
            source_id,
            relation,
            target_id,
            detail,
        }) => {
            if let Some(schema) = context.schema {
                let source_node = context.graph_file.node_by_id(&source_id);
                let target_node = context.graph_file.node_by_id(&target_id);
                if let (Some(src), Some(tgt)) = (source_node, target_node) {
                    let violations = schema.validate_edge_add(
                        &source_id,
                        &src.r#type,
                        &relation,
                        &target_id,
                        &tgt.r#type,
                    );
                    crate::bail_on_schema_violations(&violations)?;
                }
            }
            add_edge(
                context.graph_file,
                Edge {
                    source_id: source_id.clone(),
                    relation: relation.clone(),
                    target_id: target_id.clone(),
                    properties: EdgeProperties {
                        detail,
                        ..EdgeProperties::default()
                    },
                },
            )?;
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "edge.add",
                Some(format!("{source_id} {relation} {target_id}")),
                context.graph_file,
            )?;
            Ok(format!("+ edge {source_id} {relation} {target_id}\n"))
        }
        EdgeCommand::AddBatch(AddEdgeBatchArgs { file }) => {
            use std::fs::File;
            use std::io::BufReader;

            let file_handle = File::open(&file)?;
            let mut reader = ReaderBuilder::new()
                .has_headers(true)
                .from_reader(BufReader::new(file_handle));

            let mut added = 0;
            let mut skipped = 0;
            let mut errors = Vec::new();

            for result in reader.records() {
                match result {
                    Ok(record) => {
                        let source_id = record.get(0).unwrap_or("");
                        let relation = record.get(1).unwrap_or("");
                        let target_id = record.get(2).unwrap_or("");
                        let detail = record.get(3).unwrap_or("");

                        if source_id.is_empty() || relation.is_empty() || target_id.is_empty() {
                            errors.push(format!("Invalid row: {:?}", record));
                            skipped += 1;
                            continue;
                        }

                        add_edge(
                            context.graph_file,
                            Edge {
                                source_id: source_id.to_string(),
                                relation: relation.to_string(),
                                target_id: target_id.to_string(),
                                properties: EdgeProperties {
                                    detail: detail.to_string(),
                                    ..EdgeProperties::default()
                                },
                            },
                        )?;
                        added += 1;
                    }
                    Err(error) => {
                        errors.push(format!("CSV error: {}", error));
                        skipped += 1;
                    }
                }
            }

            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "edge.add-batch",
                Some(format!("file={} added={}", file, added)),
                context.graph_file,
            )?;

            let mut output = format!("+ edges: {added}\n");
            if skipped > 0 {
                output.push_str(&format!("~ skipped: {}\n", skipped));
            }
            if !errors.is_empty() {
                output.push_str(&format!("! errors: {}\n", errors.len()));
                for error in errors.iter().take(3) {
                    output.push_str(&format!("  {}\n", error));
                }
            }
            Ok(output)
        }
        EdgeCommand::Remove(RemoveEdgeArgs {
            source_id,
            relation,
            target_id,
        }) => {
            remove_edge(context.graph_file, &source_id, &relation, &target_id)?;
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "edge.remove",
                Some(format!("{source_id} {relation} {target_id}")),
                context.graph_file,
            )?;
            Ok(format!("- edge {source_id} {relation} {target_id}\n"))
        }
    }
}
