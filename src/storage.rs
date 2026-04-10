use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::KgConfig;
use crate::graph::GraphFile;
use crate::index::Bm25Index;

const REDB_GRAPH_TABLE: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("graph");
const REDB_GRAPH_KEY: &str = "current";
pub trait GraphStore {
    fn create_graph(&self, graph_name: &str) -> Result<PathBuf>;
    fn resolve_graph_path(&self, graph: &str) -> Result<PathBuf>;
    fn list_graphs(&self) -> Result<Vec<(String, PathBuf)>>;
    fn load_graph(&self, path: &Path) -> Result<GraphFile>;
    fn save_graph(&self, path: &Path, graph: &GraphFile) -> Result<()>;
}

pub fn graph_store(
    cwd: &Path,
    graph_root: &Path,
    force_legacy_json: bool,
) -> Result<Box<dyn GraphStore>> {
    let config = KgConfig::discover(cwd)?;
    let backend = config
        .as_ref()
        .and_then(|(_, config)| config.backend.as_deref())
        .unwrap_or("json");
    let backend = if force_legacy_json { "json" } else { backend };

    match backend {
        "json" => Ok(Box::new(JsonGraphStore::new_with_config(
            cwd,
            graph_root,
            config,
            force_legacy_json,
        )?)),
        "redb" => Ok(Box::new(RedbGraphStore::new_with_config(
            cwd, graph_root, config,
        )?)),
        other => bail!("unsupported backend: {other}"),
    }
}

#[derive(Debug)]
pub struct JsonGraphStore {
    cwd: PathBuf,
    graph_root: PathBuf,
    config: Option<(PathBuf, KgConfig)>,
    force_legacy_json: bool,
}

#[derive(Debug)]
pub struct RedbGraphStore {
    cwd: PathBuf,
    graph_root: PathBuf,
    config: Option<(PathBuf, KgConfig)>,
}

impl JsonGraphStore {
    pub fn new_with_config(
        cwd: &Path,
        graph_root: &Path,
        config: Option<(PathBuf, KgConfig)>,
        force_legacy_json: bool,
    ) -> Result<Self> {
        Ok(Self {
            cwd: cwd.to_path_buf(),
            graph_root: graph_root.to_path_buf(),
            config,
            force_legacy_json,
        })
    }

    fn config_graph_dir(&self) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|(config_path, config)| config.graph_dir(config_path))
    }

    fn config_graph_path(&self, graph: &str) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|(config_path, config)| config.graph_path(config_path, graph))
    }

    fn migrate_json_to_kg(&self, json_path: &Path, kg_path: &Path) -> Result<()> {
        if kg_path.exists() {
            return Ok(());
        }
        if let Some(parent) = kg_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        let mut graph = GraphFile::load(json_path).with_context(|| {
            format!(
                "failed to load source graph for migration: {}",
                json_path.display()
            )
        })?;
        let report = normalize_migrated_graph(&mut graph);
        graph.save(kg_path).with_context(|| {
            format!(
                "failed to migrate graph {} -> {}",
                json_path.display(),
                kg_path.display()
            )
        })?;
        let report_path = kg_migration_report_path(kg_path);
        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cache directory: {}", parent.display())
            })?;
        }
        fs::write(
            &report_path,
            render_migration_report(json_path, kg_path, &report),
        )
        .with_context(|| {
            format!(
                "failed to write migration report: {}",
                report_path.display()
            )
        })?;
        Ok(())
    }

    fn resolve_json_or_kg(&self, json_path: PathBuf, kg_path: PathBuf) -> Result<Option<PathBuf>> {
        if self.force_legacy_json {
            if json_path.exists() {
                return Ok(Some(json_path));
            }
            if kg_path.exists() {
                return Ok(Some(kg_path));
            }
            return Ok(None);
        }

        if kg_path.exists() {
            return Ok(Some(kg_path));
        }
        if json_path.exists() {
            self.migrate_json_to_kg(&json_path, &kg_path)?;
            return Ok(Some(kg_path));
        }
        Ok(None)
    }
}

#[derive(Default)]
struct MigrationReport {
    mapped_node_types: usize,
    custom_node_types: usize,
    mapped_relations: usize,
    incoming_edges_rewritten: usize,
    duplicate_edges_removed: usize,
    warnings: Vec<String>,
}

fn kg_migration_report_path(kg_path: &Path) -> PathBuf {
    let stem = kg_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    crate::cache_paths::cache_root_for_graph(kg_path).join(format!("{stem}.migration.log"))
}

fn render_migration_report(json_path: &Path, kg_path: &Path, report: &MigrationReport) -> String {
    let mut lines = vec![
        String::from("= migration-report"),
        format!("source: {}", json_path.display()),
        format!("target: {}", kg_path.display()),
        format!("mapped_node_types: {}", report.mapped_node_types),
        format!("custom_node_types: {}", report.custom_node_types),
        format!("mapped_relations: {}", report.mapped_relations),
        format!(
            "incoming_edges_rewritten: {}",
            report.incoming_edges_rewritten
        ),
        format!(
            "duplicate_edges_removed: {}",
            report.duplicate_edges_removed
        ),
        format!("warnings: {}", report.warnings.len()),
    ];
    if !report.warnings.is_empty() {
        lines.push(String::from("warning-list:"));
        for warning in &report.warnings {
            lines.push(format!("- {warning}"));
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn normalize_migrated_graph(graph: &mut GraphFile) -> MigrationReport {
    let mut report = MigrationReport::default();

    for node in &mut graph.nodes {
        let original = node.r#type.clone();
        if let Some(mapped) = map_node_type_alias(&original) {
            if mapped != original {
                report.mapped_node_types += 1;
                node.r#type = mapped.to_owned();
            }
            continue;
        }

        let custom = sanitize_custom_key(&original);
        if custom != original {
            report.custom_node_types += 1;
            report.warnings.push(format!(
                "node {}: unknown type '{}' migrated as custom '{}'",
                node.id, original, custom
            ));
            node.r#type = custom;
        }
    }

    for edge in &mut graph.edges {
        let original_relation = edge.relation.clone();
        let mut relation = original_relation.clone();
        let mut incoming = false;

        if let Some(stripped) = relation.strip_prefix("<-") {
            incoming = true;
            relation = stripped.trim().to_owned();
        } else if let Some(stripped) = relation.strip_suffix("<-") {
            incoming = true;
            relation = stripped.trim().to_owned();
        }

        if let Some(mapped) = map_relation_alias(&relation) {
            if mapped != relation {
                report.mapped_relations += 1;
            }
            relation = mapped.to_owned();
        }

        if incoming {
            std::mem::swap(&mut edge.source_id, &mut edge.target_id);
            report.incoming_edges_rewritten += 1;
            report.warnings.push(format!(
                "edge rewritten from incoming to outgoing: {} {} {}",
                edge.source_id, relation, edge.target_id
            ));
        }

        edge.relation = relation;

        if edge.source_id == edge.target_id {
            report.warnings.push(format!(
                "self-edge kept: {} {}",
                edge.source_id, edge.relation
            ));
        }
    }

    let mut seen = HashSet::new();
    let original_len = graph.edges.len();
    graph.edges.retain(|edge| {
        seen.insert(format!(
            "{}|{}|{}|{}",
            edge.source_id, edge.relation, edge.target_id, edge.properties.detail
        ))
    });
    report.duplicate_edges_removed = original_len.saturating_sub(graph.edges.len());

    report
}

fn normalize_alias_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>()
}

fn map_node_type_alias(value: &str) -> Option<&'static str> {
    match normalize_alias_token(value).as_str() {
        "feature" | "features" => Some("Feature"),
        "concept" | "concepts" => Some("Concept"),
        "interface" | "interfaces" | "iface" => Some("Interface"),
        "process" | "processes" => Some("Process"),
        "datastore" | "datastores" | "data" | "datastorage" => Some("DataStore"),
        "attribute" | "attributes" => Some("Attribute"),
        "entity" | "entities" => Some("Entity"),
        "note" | "notes" => Some("Note"),
        "rule" | "rules" => Some("Rule"),
        "convention" | "conventions" => Some("Convention"),
        "bug" | "bugs" => Some("Bug"),
        "decision" | "decisions" => Some("Decision"),
        "openquestion" | "question" | "questions" => Some("OpenQuestion"),
        "claim" | "claims" => Some("Claim"),
        "insight" | "insights" => Some("Insight"),
        "reference" | "references" | "ref" | "refs" => Some("Reference"),
        "term" | "terms" => Some("Term"),
        "status" | "statuses" => Some("Status"),
        "doubt" | "doubts" => Some("Doubt"),
        _ => None,
    }
}

fn map_relation_alias(value: &str) -> Option<&'static str> {
    match normalize_alias_token(value).as_str() {
        "documentedin" | "documents" => Some("DOCUMENTED_IN"),
        "has" => Some("HAS"),
        "triggers" => Some("TRIGGERS"),
        "affectedby" | "affects" => Some("AFFECTED_BY"),
        "readsfrom" | "reads" | "storedin" => Some("READS_FROM"),
        "governedby" | "governs" => Some("GOVERNED_BY"),
        "dependson" | "depends" | "?" => Some("DEPENDS_ON"),
        "availablein" => Some("AVAILABLE_IN"),
        "supports" => Some("SUPPORTS"),
        "summarizes" => Some("SUMMARIZES"),
        "relatedto" | "doubtd" => Some("RELATED_TO"),
        "contradicts" | "contradictsx" => Some("CONTRADICTS"),
        "createdby" | "creates" => Some("CREATED_BY"),
        "decidedby" => Some("DECIDED_BY"),
        "uses" => Some("USES"),
        "transitions" => Some("TRANSITIONS"),
        "available" => Some("AVAILABLE_IN"),
        _ => None,
    }
}

fn sanitize_custom_key(value: &str) -> String {
    let mut out: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .flat_map(char::to_lowercase)
        .collect();
    if out.len() < 2 {
        out = format!("x{out}");
    }
    if out.len() > 10 {
        out.truncate(10);
    }
    out
}

impl RedbGraphStore {
    pub fn new_with_config(
        cwd: &Path,
        graph_root: &Path,
        config: Option<(PathBuf, KgConfig)>,
    ) -> Result<Self> {
        Ok(Self {
            cwd: cwd.to_path_buf(),
            graph_root: graph_root.to_path_buf(),
            config,
        })
    }

    fn config_graph_dir(&self) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|(config_path, config)| config.graph_dir(config_path))
    }

    fn config_graph_path(&self, graph: &str) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|(config_path, config)| config.graph_path(config_path, graph))
    }

    fn open_db(&self, path: &Path) -> Result<redb::Database> {
        if path.exists() {
            redb::Database::open(path)
                .with_context(|| format!("failed to open redb: {}", path.display()))
        } else {
            redb::Database::create(path)
                .with_context(|| format!("failed to create redb: {}", path.display()))
        }
    }
}

impl GraphStore for JsonGraphStore {
    fn create_graph(&self, graph_name: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.graph_root)?;
        let ext = if self.force_legacy_json { "json" } else { "kg" };
        let path = self.graph_root.join(format!("{graph_name}.{ext}"));
        let legacy_json_path = self.graph_root.join(format!("{graph_name}.json"));
        let native_kg_path = self.graph_root.join(format!("{graph_name}.kg"));
        if legacy_json_path.exists() || native_kg_path.exists() {
            bail!("graph already exists: {}", graph_name);
        }
        if path.exists() {
            bail!("graph already exists: {}", path.display());
        }
        let graph = GraphFile::new(graph_name);
        graph.save(&path)?;
        Ok(path)
    }

    fn resolve_graph_path(&self, graph: &str) -> Result<PathBuf> {
        if let Some(path) = self.config_graph_path(graph) {
            if path.is_file() {
                if !self.force_legacy_json
                    && path.extension().and_then(|ext| ext.to_str()) == Some("json")
                {
                    let kg_path = path.with_extension("kg");
                    if let Some(resolved) = self.resolve_json_or_kg(path.clone(), kg_path)? {
                        return Ok(resolved);
                    }
                }
                return Ok(path);
            }
        }
        if let Some(config_graph_dir) = self.config_graph_dir() {
            let direct = config_graph_dir.join(graph);
            let json = config_graph_dir.join(format!("{graph}.json"));
            let kg = config_graph_dir.join(format!("{graph}.kg"));
            if direct.is_file() {
                return Ok(direct);
            }
            if let Some(path) = self.resolve_json_or_kg(json, kg)? {
                return Ok(path);
            }
        }

        let raw = PathBuf::from(graph);
        let candidates = [
            raw.clone(),
            self.cwd.join(graph),
            self.graph_root.join(graph),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        if let Some(path) = self.resolve_json_or_kg(
            self.cwd.join(format!("{graph}.json")),
            self.cwd.join(format!("{graph}.kg")),
        )? {
            return Ok(path);
        }
        if let Some(path) = self.resolve_json_or_kg(
            self.graph_root.join(format!("{graph}.json")),
            self.graph_root.join(format!("{graph}.kg")),
        )? {
            return Ok(path);
        }
        if self.force_legacy_json {
            let fallback = self.cwd.join(format!("graph-example-{graph}.json"));
            if fallback.is_file() {
                return Ok(fallback);
            }
        }
        bail!("graph not found: {graph}")
    }

    fn list_graphs(&self) -> Result<Vec<(String, PathBuf)>> {
        let mut dirs = vec![self.graph_root.clone(), self.cwd.join(".kg").join("graphs")];
        if let Some(config_graph_dir) = self.config_graph_dir() {
            dirs.push(config_graph_dir);
        }
        dirs.sort();
        dirs.dedup();

        let mut graphs: Vec<(String, PathBuf)> = Vec::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                    continue;
                };
                if ext != "json" && ext != "kg" {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                graphs.push((stem.to_owned(), path));
            }
        }

        graphs.sort_by(|a, b| {
            let ext_rank = |path: &Path| -> u8 {
                match path.extension().and_then(|ext| ext.to_str()) {
                    Some("kg") => 0,
                    Some("json") => 1,
                    _ => 2,
                }
            };
            a.0.cmp(&b.0)
                .then_with(|| ext_rank(&a.1).cmp(&ext_rank(&b.1)))
                .then_with(|| a.1.cmp(&b.1))
        });
        graphs.dedup_by(|a, b| a.0 == b.0);
        Ok(graphs)
    }

    fn load_graph(&self, path: &Path) -> Result<GraphFile> {
        let graph = GraphFile::load(path)?;
        if let Err(error) = crate::kg_sidecar::ensure_kgindex_fresh(path) {
            eprintln!("warning: failed to refresh kgindex sidecar: {error}");
        }
        Ok(graph)
    }

    fn save_graph(&self, path: &Path, graph: &GraphFile) -> Result<()> {
        graph.save(path)?;
        if let Err(error) = crate::kg_sidecar::invalidate_kgindex(path) {
            eprintln!("warning: failed to invalidate kgindex sidecar: {error}");
        }
        Ok(())
    }
}

impl GraphStore for RedbGraphStore {
    fn create_graph(&self, graph_name: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.graph_root)?;
        let path = self.graph_root.join(format!("{graph_name}.db"));
        if path.exists() {
            bail!("graph already exists: {}", path.display());
        }
        let graph = GraphFile::new(graph_name);
        self.save_graph(&path, &graph)?;
        Ok(path)
    }

    fn resolve_graph_path(&self, graph: &str) -> Result<PathBuf> {
        if let Some(path) = self.config_graph_path(graph) {
            if path.is_file() {
                return Ok(path);
            }
        }
        if let Some(config_graph_dir) = self.config_graph_dir() {
            let direct = config_graph_dir.join(graph);
            let db = config_graph_dir.join(format!("{graph}.db"));
            if direct.is_file() {
                return Ok(direct);
            }
            if db.is_file() {
                return Ok(db);
            }
        }

        let raw = PathBuf::from(graph);
        let candidates = [
            raw.clone(),
            self.cwd.join(graph),
            self.cwd.join(format!("{graph}.db")),
            self.graph_root.join(graph),
            self.graph_root.join(format!("{graph}.db")),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        bail!("graph not found: {graph}")
    }

    fn list_graphs(&self) -> Result<Vec<(String, PathBuf)>> {
        let mut dirs = vec![self.graph_root.clone(), self.cwd.join(".kg").join("graphs")];
        if let Some(config_graph_dir) = self.config_graph_dir() {
            dirs.push(config_graph_dir);
        }
        dirs.sort();
        dirs.dedup();

        let mut graphs: Vec<(String, PathBuf)> = Vec::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("db") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                graphs.push((stem.to_owned(), path));
            }
        }

        graphs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        graphs.dedup_by(|a, b| a.0 == b.0);
        Ok(graphs)
    }

    fn load_graph(&self, path: &Path) -> Result<GraphFile> {
        let db = self.open_db(path)?;
        let read_txn = db
            .begin_read()
            .context("failed to start redb read transaction")?;
        let table = read_txn
            .open_table(REDB_GRAPH_TABLE)
            .context("missing graph table")?;
        let raw = table
            .get(REDB_GRAPH_KEY)
            .context("failed to read graph entry")?
            .ok_or_else(|| anyhow::anyhow!("graph entry missing"))?;
        let raw_str = std::str::from_utf8(raw.value()).context("invalid UTF-8 in graph entry")?;
        let mut graph: GraphFile = serde_json::from_str(raw_str).map_err(|error| {
            anyhow::anyhow!(
                "invalid graph JSON in redb: {} at line {}, column {}\n{}",
                path.display(),
                error.line(),
                error.column(),
                raw_str
                    .lines()
                    .nth(error.line().saturating_sub(1))
                    .map(|line| {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            "fragment: <empty line>".to_owned()
                        } else {
                            format!("fragment: {trimmed}")
                        }
                    })
                    .unwrap_or_else(|| "fragment: <unavailable>".to_owned())
            )
        })?;
        graph.refresh_counts();
        Ok(graph)
    }

    fn save_graph(&self, path: &Path, graph: &GraphFile) -> Result<()> {
        let mut snapshot = graph.clone();
        snapshot.refresh_counts();
        let raw = serde_json::to_string_pretty(&snapshot).context("failed to serialize graph")?;
        let db = self.open_db(path)?;
        let write_txn = db
            .begin_write()
            .context("failed to start redb write transaction")?;
        {
            let mut table = write_txn
                .open_table(REDB_GRAPH_TABLE)
                .context("failed to open graph table")?;
            table.insert(REDB_GRAPH_KEY, raw.as_bytes())?;
        }
        write_txn.commit().context("failed to commit redb")?;

        let index = Bm25Index::build(&snapshot);
        let index_path = redb_index_path(path);
        if let Some(parent) = index_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cache directory: {}", parent.display())
            })?;
        }
        index.save(&index_path)?;

        Ok(())
    }
}

pub fn load_graph_index(graph_path: &Path) -> Result<Option<Bm25Index>> {
    let index_path = redb_index_path(graph_path);
    let legacy_path = graph_path.with_extension("index.db");
    let path = if index_path.exists() {
        index_path
    } else if legacy_path.exists() {
        legacy_path
    } else {
        return Ok(None);
    };
    let index = Bm25Index::load(&path)?;
    Ok(Some(index))
}

fn redb_index_path(graph_path: &Path) -> PathBuf {
    let stem = graph_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    let ext = graph_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("db");
    crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.{ext}.index.db"))
}
