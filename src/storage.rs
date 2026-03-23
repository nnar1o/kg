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

pub fn graph_store(cwd: &Path, graph_root: &Path) -> Result<Box<dyn GraphStore>> {
    let config = KgConfig::discover(cwd)?;
    let backend = config
        .as_ref()
        .and_then(|(_, config)| config.backend.as_deref())
        .unwrap_or("json");

    match backend {
        "json" => Ok(Box::new(JsonGraphStore::new_with_config(
            cwd, graph_root, config,
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
        let path = self.graph_root.join(format!("{graph_name}.json"));
        if path.exists() {
            bail!("graph already exists: {}", path.display());
        }
        let graph = GraphFile::new(graph_name);
        graph.save(&path)?;
        Ok(path)
    }

    fn resolve_graph_path(&self, graph: &str) -> Result<PathBuf> {
        if let Some(path) = self.config_graph_path(graph) {
            if path.exists() {
                return Ok(path);
            }
        }
        if let Some(config_graph_dir) = self.config_graph_dir() {
            let direct = config_graph_dir.join(graph);
            let json = config_graph_dir.join(format!("{graph}.json"));
            if direct.exists() {
                return Ok(direct);
            }
            if json.exists() {
                return Ok(json);
            }
        }

        let raw = PathBuf::from(graph);
        let candidates = [
            raw.clone(),
            self.cwd.join(graph),
            self.cwd.join(format!("{graph}.json")),
            self.graph_root.join(graph),
            self.graph_root.join(format!("{graph}.json")),
            self.cwd.join(format!("graph-example-{graph}.json")),
        ];
        for candidate in candidates {
            if candidate.exists() {
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
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
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
        GraphFile::load(path)
    }

    fn save_graph(&self, path: &Path, graph: &GraphFile) -> Result<()> {
        graph.save(path)
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
            if path.exists() {
                return Ok(path);
            }
        }
        if let Some(config_graph_dir) = self.config_graph_dir() {
            let direct = config_graph_dir.join(graph);
            let db = config_graph_dir.join(format!("{graph}.db"));
            if direct.exists() {
                return Ok(direct);
            }
            if db.exists() {
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
            if candidate.exists() {
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
        let mut graph: GraphFile =
            serde_json::from_str(raw_str).context("invalid graph JSON in redb")?;
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
        let index_path = path.with_extension("index.db");
        index.save(&index_path)?;

        Ok(())
    }
}

pub fn load_graph_index(graph_path: &Path) -> Result<Option<Bm25Index>> {
    let index_path = graph_path.with_extension("index.db");
    if !index_path.exists() {
        return Ok(None);
    }
    let index = Bm25Index::load(&index_path)?;
    Ok(Some(index))
}
