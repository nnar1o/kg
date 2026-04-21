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
    pub(crate) user_short_uid: &'a str,
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
            mode,
            full,
            output_size,
            json,
            debug_score,
            include_metadata,
            tune,
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

            let bm25_index = if matches!(
                mode,
                crate::cli::FindMode::Bm25 | crate::cli::FindMode::Hybrid
            ) {
                load_graph_index(context.path).ok().flatten()
            } else {
                None
            };
            let tune_parsed = if let Some(raw) = tune.as_deref() {
                Some(crate::output::FindTune::parse(raw).ok_or_else(|| {
                    anyhow!("invalid --tune value, expected key=value pairs for bm25,fuzzy,vector")
                })?)
            } else {
                None
            };

            let find_mode = crate::map_find_mode(mode);
            let timer = access_log::Timer::new();
            let mut query_hits: Vec<Vec<String>> = Vec::new();
            let mut query_result_counts: Vec<usize> = Vec::new();
            for query in &queries {
                let matches = output::find_nodes_with_index_tuned(
                    context.graph_file,
                    query,
                    limit,
                    true,
                    include_metadata,
                    find_mode,
                    bm25_index.as_ref(),
                    tune_parsed.as_ref(),
                );
                query_result_counts.push(matches.len());
                query_hits.push(matches.into_iter().map(|node| node.id).collect());
            }
            let result = if json {
                crate::render_find_json_with_index(
                    context.graph_file,
                    &queries,
                    limit,
                    include_metadata,
                    find_mode,
                    debug_score,
                    bm25_index.as_ref(),
                    tune_parsed.as_ref(),
                )
            } else if full {
                output::render_find_with_index_tuned(
                    context.graph_file,
                    &queries,
                    limit,
                    true,
                    include_metadata,
                    find_mode,
                    full,
                    debug_score,
                    bm25_index.as_ref(),
                    tune_parsed.as_ref(),
                )
            } else {
                output::render_find_adaptive_with_index_tuned(
                    context.graph_file,
                    &queries,
                    limit,
                    true,
                    include_metadata,
                    find_mode,
                    output_size,
                    debug_score,
                    bm25_index.as_ref(),
                    tune_parsed.as_ref(),
                )
            };
            let duration_ms = timer.elapsed_ms();

            for (query, count) in queries.iter().zip(query_result_counts.into_iter()) {
                let entry = access_log::AccessLogEntry::new(query.clone(), count, duration_ms);
                if let Err(error) = access_log::append_entry(context.path, &entry) {
                    eprintln!("warning: failed to log access: {}", error);
                }
            }
            for node_id in query_hits.into_iter().flatten() {
                if let Err(error) =
                    access_log::append_hit(context.path, context.user_short_uid, &node_id)
                {
                    eprintln!("warning: failed to log kg hit: {}", error);
                }
            }

            Ok(result)
        }

        NodeCommand::Get {
            id,
            full,
            output_size,
            json,
        } => {
            let id = crate::validate::normalize_node_id(&id);
            let timer = access_log::Timer::new();
            let index_hint = crate::kg_sidecar::lookup_node_line(context.path, &id);
            let node = if index_hint.is_some() {
                context
                    .graph_file
                    .node_by_id_sorted(&id)
                    .or_else(|| context.graph_file.node_by_id(&id))
            } else {
                context.graph_file.node_by_id(&id)
            }
            .ok_or_else(|| anyhow!("node not found: {id}"))?;
            let result = if json {
                crate::render_node_json(node)
            } else if full {
                output::render_node(context.graph_file, node, full)
            } else {
                output::render_node_adaptive(context.graph_file, node, output_size)
            };

            let duration_ms = timer.elapsed_ms();
            let entry = access_log::AccessLogEntry::node_get(id.clone(), duration_ms);
            if let Err(error) = access_log::append_entry(context.path, &entry) {
                eprintln!("warning: failed to log access: {}", error);
            }
            if let Err(error) = access_log::append_hit(context.path, context.user_short_uid, &id) {
                eprintln!("warning: failed to log kg hit: {}", error);
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
            importance,
            fact,
            alias,
            source,
            valid_from,
            valid_to,
        }) => {
            let id = crate::validate::canonicalize_node_id_for_type(&id, &node_type)
                .map_err(anyhow::Error::msg)?;
            let description = description.unwrap_or_else(|| name.clone());
            let domain_area = domain_area.unwrap_or_else(|| node_type.to_ascii_lowercase());
            let provenance = provenance.unwrap_or_else(|| "U".to_owned());
            let confidence = confidence.or(Some(0.8));
            let created_at = created_at.unwrap_or_else(current_utc_timestamp);
            let source = if source.is_empty() {
                vec!["DOC kg graph node add".to_owned()]
            } else {
                source
            };
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
                    importance,
                    key_facts: fact,
                    alias,
                    valid_from: valid_from.unwrap_or_default(),
                    valid_to: valid_to.unwrap_or_default(),
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
            importance,
            fact,
            alias,
            source,
            valid_from,
            valid_to,
        }) => {
            let id = crate::validate::normalize_node_id(&id);

            // Capture old state for diff
            let old_node = context.graph_file.node_by_id(&id).cloned();

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
                importance,
                fact.clone(),
                alias.clone(),
                source.clone(),
                valid_from.clone(),
                valid_to.clone(),
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

            // Generate diff output
            let mut diff_lines = vec![format!("~ node {id}")];

            if let Some(old) = old_node {
                if let Some(new) = context.graph_file.node_by_id(&id) {
                    // Name diff
                    if old.name != new.name {
                        diff_lines.push(format!("- name: {}", old.name));
                        diff_lines.push(format!("+ name: {}", new.name));
                    }
                    // Description diff
                    if old.properties.description != new.properties.description {
                        let old_desc = old.properties.description.lines().next().unwrap_or("");
                        let new_desc = new.properties.description.lines().next().unwrap_or("");
                        if old_desc != new_desc {
                            diff_lines.push(format!("- description: {}", old_desc));
                            diff_lines.push(format!("+ description: {}", new_desc));
                        }
                    }
                    // Importance diff
                    if (old.properties.importance - new.properties.importance).abs() > 0.001 {
                        diff_lines.push(format!("- importance: {}", old.properties.importance));
                        diff_lines.push(format!("+ importance: {}", new.properties.importance));
                    }
                    // Confidence diff
                    let old_conf = old.properties.confidence.unwrap_or(-1.0);
                    let new_conf = new.properties.confidence.unwrap_or(-1.0);
                    if (old_conf - new_conf).abs() > 0.001 {
                        diff_lines.push(format!("- confidence: {}", old.properties.confidence.map(|c| c.to_string()).unwrap_or_else(|| "none".to_string())));
                        diff_lines.push(format!("+ confidence: {}", new.properties.confidence.map(|c| c.to_string()).unwrap_or_else(|| "none".to_string())));
                    }
                    // valid_from diff
                    if old.properties.valid_from != new.properties.valid_from {
                        diff_lines.push(format!("- valid_from: {}", old.properties.valid_from));
                        diff_lines.push(format!("+ valid_from: {}", new.properties.valid_from));
                    }
                    // valid_to diff
                    if old.properties.valid_to != new.properties.valid_to {
                        diff_lines.push(format!("- valid_to: {}", old.properties.valid_to));
                        diff_lines.push(format!("+ valid_to: {}", new.properties.valid_to));
                    }
                }
            }

            Ok(format!("{}\n", diff_lines.join("\n")))
        }

        NodeCommand::Rename { from, to } => {
            let from = crate::validate::normalize_node_id(&from);
            let to = crate::validate::normalize_node_id(&to);
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
            let id = crate::validate::normalize_node_id(&id);
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
    }
}

fn current_utc_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let days_since_epoch = secs / 86400;
    let (year, month, day) = days_to_date(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days: i64) -> (i64, u32, u32) {
    let mut year = 1970;
    let mut remaining_days = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &days_in_month in &month_days {
        if remaining_days < days_in_month as i64 {
            break;
        }
        remaining_days -= days_in_month as i64;
        month += 1;
    }

    (year, month, (remaining_days + 1) as u32)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
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
            valid_from,
            valid_to,
        }) => {
            let source_id = crate::validate::normalize_node_id(&source_id);
            let target_id = crate::validate::normalize_node_id(&target_id);
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
                        valid_from: valid_from.unwrap_or_default(),
                        valid_to: valid_to.unwrap_or_default(),
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
                                source_id: crate::validate::normalize_node_id(source_id),
                                relation: relation.to_string(),
                                target_id: crate::validate::normalize_node_id(target_id),
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
            let source_id = crate::validate::normalize_node_id(&source_id);
            let target_id = crate::validate::normalize_node_id(&target_id);
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
