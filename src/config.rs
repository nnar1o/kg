#![allow(dead_code, unused_variables)]

use std::collections::{HashMap, HashSet};
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
    #[serde(default)]
    pub graph_dirs: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_nudge_percent")]
    pub nudge: Option<u8>,
    #[serde(default, deserialize_with = "deserialize_user_short_uid")]
    pub user_short_uid: Option<String>,
    #[serde(default)]
    pub graphs: HashMap<String, PathBuf>,
    #[serde(default)]
    pub default_graph: Option<String>,
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

    pub fn graph_dirs(&self, config_path: &Path) -> Vec<PathBuf> {
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        if let Some(dir) = self.graph_dir.as_ref() {
            let resolved = resolve_path(base, dir);
            if seen.insert(resolved.clone()) {
                out.push(resolved);
            }
        }
        for dir in &self.graph_dirs {
            let resolved = resolve_path(base, dir);
            if seen.insert(resolved.clone()) {
                out.push(resolved);
            }
        }
        out
    }

    pub fn graph_path(&self, config_path: &Path, name: &str) -> Option<PathBuf> {
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        self.graphs.get(name).map(|path| resolve_path(base, path))
    }

    pub fn nudge_percent(&self) -> u8 {
        self.nudge.unwrap_or(DEFAULT_NUDGE_PERCENT)
    }

    /// Get default graph name from config or environment
    /// Priority: KG_DEFAULT_GRAPH env > config.default_graph
    pub fn default_graph(&self, cwd: &Path) -> Option<String> {
        // Check env var first
        let env_default = std::env::var("KG_DEFAULT_GRAPH").ok();
        if let Some(graph) = env_default {
            return Some(graph);
        }
        // Then config
        self.default_graph.clone()
    }
}

/// Get default graph name - checks env var and config
pub fn resolve_default_graph(cwd: &Path) -> Option<String> {
    match KgConfig::discover(cwd) {
        Ok(Some((_, cfg))) => cfg.default_graph(cwd),
        _ => None,
    }
}

pub fn ensure_user_short_uid(cwd: &Path) -> String {
    let env_uid = std::env::var("KG_USER_SHORT_UID").ok();
    resolve_user_short_uid_with_env(cwd, env_uid.as_deref())
}

fn resolve_user_short_uid_with_env(cwd: &Path, env_uid: Option<&str>) -> String {
    if let Some(uid) = env_uid.and_then(normalize_user_short_uid) {
        return uid;
    }

    match KgConfig::discover(cwd) {
        Ok(Some((config_path, cfg))) => {
            if let Some(uid) = cfg
                .user_short_uid
                .as_deref()
                .and_then(normalize_user_short_uid)
            {
                return uid;
            }
            let generated = generate_user_short_uid();
            let _ = persist_user_short_uid(&config_path, &generated);
            generated
        }
        Ok(None) | Err(_) => {
            let generated = generate_user_short_uid();
            let config_path = cwd.join(".kg.toml");
            let _ = persist_user_short_uid(&config_path, &generated);
            generated
        }
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

fn deserialize_user_short_uid<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value {
        Some(value) => normalize_user_short_uid(&value).map(Some).ok_or_else(|| {
            de::Error::custom("user_short_uid must be 1..=16 chars [a-zA-Z0-9_-] (or unset)")
        }),
        None => Ok(None),
    }
}

fn normalize_user_short_uid(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let len = trimmed.chars().count();
    if len == 0 || len > 16 {
        return None;
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn generate_user_short_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut value = nanos;
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while value > 0 {
        out.push(alphabet[(value % 36) as usize] as char);
        value /= 36;
    }
    if out.is_empty() {
        out.push('0');
    }
    out.reverse();
    let mut uid: String = out.into_iter().collect();
    if uid.len() > 8 {
        uid = uid.split_off(uid.len() - 8);
    } else if uid.len() < 8 {
        uid = format!("{:0>8}", uid);
    }
    uid
}

fn persist_user_short_uid(config_path: &Path, uid: &str) -> Result<()> {
    let mut raw = if config_path.exists() {
        fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?
    } else {
        String::new()
    };

    let mut replaced = false;
    let mut lines = Vec::new();
    for line in raw.lines() {
        if line.trim_start().starts_with("user_short_uid") {
            lines.push(format!("user_short_uid = \"{}\"", uid));
            replaced = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !replaced {
        if !raw.is_empty() && !raw.ends_with('\n') {
            raw.push('\n');
        }
        raw.push_str(&format!("user_short_uid = \"{}\"\n", uid));
    } else {
        raw = format!("{}\n", lines.join("\n"));
    }

    fs::write(config_path, raw)
        .with_context(|| format!("failed to write config: {}", config_path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_NUDGE_PERCENT, KgConfig, normalize_user_short_uid, resolve_user_short_uid_with_env,
    };
    use std::path::PathBuf;

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

    #[test]
    fn user_short_uid_accepts_valid_value() {
        let config: KgConfig = toml::from_str("user_short_uid = \"dev_01\"\n").expect("config");
        assert_eq!(config.user_short_uid.as_deref(), Some("dev_01"));
    }

    #[test]
    fn user_short_uid_rejects_invalid_value() {
        let err = toml::from_str::<KgConfig>("user_short_uid = \"bad uid\"\n")
            .expect_err("invalid config");
        assert!(err.to_string().contains("user_short_uid must be"));
    }

    #[test]
    fn normalize_user_short_uid_enforces_shape() {
        assert_eq!(normalize_user_short_uid(" u-1 "), Some("u-1".to_string()));
        assert_eq!(normalize_user_short_uid(""), None);
        assert_eq!(normalize_user_short_uid("bad uid"), None);
    }

    #[test]
    fn ensure_user_short_uid_persists_when_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".kg.toml"), "nudge = 20\n").expect("write config");

        let uid = resolve_user_short_uid_with_env(dir.path(), None);
        assert!(uid.len() <= 16);

        let saved = std::fs::read_to_string(dir.path().join(".kg.toml")).expect("read config");
        assert!(saved.contains("user_short_uid = \""));
    }

    #[test]
    fn graph_dirs_resolve_relative_paths_and_deduplicate() {
        let config: KgConfig = toml::from_str(
            "graph_dir = \"graphs\"\ngraph_dirs = [\"graphs\", \"extra\", \"/tmp/kg\"]\n",
        )
        .expect("config");

        let config_path = PathBuf::from("/workspace/project/.kg.toml");
        let dirs = config.graph_dirs(&config_path);

        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/workspace/project/graphs"),
                PathBuf::from("/workspace/project/extra"),
                PathBuf::from("/tmp/kg"),
            ]
        );
    }
}
