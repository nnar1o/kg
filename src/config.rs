use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, de};

pub const DEFAULT_NUDGE_PERCENT: u8 = 20;

#[derive(Debug, Default, Deserialize)]
pub struct KgConfig {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub graph_dir: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_nudge_percent")]
    pub nudge: Option<u8>,
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

    pub fn nudge_percent(&self) -> u8 {
        self.nudge.unwrap_or(DEFAULT_NUDGE_PERCENT)
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn deserialize_nudge_percent<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<u8>::deserialize(deserializer)?;
    match value {
        Some(value) if value <= 100 => Ok(Some(value)),
        Some(_) => Err(de::Error::custom("nudge must be between 0 and 100")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_NUDGE_PERCENT, KgConfig};

    #[test]
    fn nudge_defaults_to_twenty() {
        let config: KgConfig = toml::from_str("").expect("config");
        assert_eq!(config.nudge_percent(), DEFAULT_NUDGE_PERCENT);
    }

    #[test]
    fn nudge_accepts_zero_and_hundred() {
        let disabled: KgConfig = toml::from_str("nudge = 0\n").expect("config");
        let always: KgConfig = toml::from_str("nudge = 100\n").expect("config");
        assert_eq!(disabled.nudge_percent(), 0);
        assert_eq!(always.nudge_percent(), 100);
    }

    #[test]
    fn nudge_rejects_values_above_hundred() {
        let err = toml::from_str::<KgConfig>("nudge = 101\n").expect_err("invalid config");
        assert!(err.to_string().contains("nudge must be between 0 and 100"));
    }
}
