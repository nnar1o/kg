use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct KgConfig {
    #[serde(default)]
    pub graph_dir: Option<PathBuf>,
    #[serde(default)]
    pub graphs: HashMap<String, PathBuf>,
}

impl KgConfig {
    pub fn discover(start: &Path) -> Result<Option<(PathBuf, Self)>> {
        for dir in start.ancestors() {
            let path = dir.join(".kg.toml");
            if path.exists() {
                let config = Self::load(&path)?;
                return Ok(Some((path, config)));
            }
        }
        Ok(None)
    }

    fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("invalid config TOML: {}", path.display()))
    }

    pub fn graph_dir(&self, config_path: &Path) -> Option<PathBuf> {
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        self.graph_dir.as_ref().map(|dir| resolve_path(base, dir))
    }

    pub fn graph_path(&self, config_path: &Path, name: &str) -> Option<PathBuf> {
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        self.graphs.get(name).map(|path| resolve_path(base, path))
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
