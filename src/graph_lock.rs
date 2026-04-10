use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

const DEFAULT_LOCK_TIMEOUT_MS: u64 = 30_000;
const LOCK_RETRY_SLEEP_MS: u64 = 50;

pub struct GraphWriteLock {
    path: PathBuf,
}

impl Drop for GraphWriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn acquire_for_graph(graph_path: &Path) -> Result<GraphWriteLock> {
    let timeout_ms = std::env::var("KG_GRAPH_LOCK_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(DEFAULT_LOCK_TIMEOUT_MS);
    acquire_for_graph_with_timeout(graph_path, Duration::from_millis(timeout_ms))
}

pub(crate) fn acquire_for_graph_with_timeout(
    graph_path: &Path,
    timeout: Duration,
) -> Result<GraphWriteLock> {
    let lock_path = lock_path_for_graph(graph_path);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory: {}", parent.display()))?;
    }

    let start = SystemTime::now();
    loop {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let pid = std::process::id();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let _ = writeln!(file, "pid={pid} ts={now}");
                return Ok(GraphWriteLock { path: lock_path });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let elapsed = start.elapsed().unwrap_or_default();
                if elapsed >= timeout {
                    bail!(
                        "graph is locked by another process: {} (waited {} ms)",
                        graph_path.display(),
                        timeout.as_millis()
                    );
                }
                thread::sleep(Duration::from_millis(LOCK_RETRY_SLEEP_MS));
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to acquire lock: {}", lock_path.display()));
            }
        }
    }
}

fn lock_path_for_graph(graph_path: &Path) -> PathBuf {
    let stem = graph_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    let ext = graph_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("json");
    crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.{ext}.write.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_lock_blocks_parallel_writer() {
        let unique = format!(
            "kg-lock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique).join(".kg").join("graphs");
        fs::create_dir_all(&root).expect("create graph root");
        let graph_path = root.join("fridge.kg");
        fs::write(&graph_path, "{}").expect("write graph");

        let _first = acquire_for_graph_with_timeout(&graph_path, Duration::from_millis(50))
            .expect("first lock");
        let second = acquire_for_graph_with_timeout(&graph_path, Duration::from_millis(120));
        assert!(second.is_err());

        let parent = root.parent().and_then(|p| p.parent()).expect("temp parent");
        let _ = fs::remove_dir_all(parent);
    }
}
