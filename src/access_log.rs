use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AccessLogEntry {
    pub timestamp: String,
    pub query: String,
    pub results: usize,
    pub duration_ms: u128,
    pub node_get_id: Option<String>,
}

impl AccessLogEntry {
    pub fn new(query: String, results: usize, duration_ms: u128) -> Self {
        Self {
            timestamp: chrono_now(),
            query,
            results,
            duration_ms,
            node_get_id: None,
        }
    }

    pub fn node_get(id: String, duration_ms: u128) -> Self {
        Self {
            timestamp: chrono_now(),
            query: format!("GET {}", id),
            results: 1,
            duration_ms,
            node_get_id: Some(id),
        }
    }

    fn to_line(&self) -> String {
        if let Some(ref id) = self.node_get_id {
            format!(
                "{}\tGET\t{}\t1\t{}ms\n",
                self.timestamp, id, self.duration_ms
            )
        } else {
            format!(
                "{}\tFIND\t{}\t{}\t{}ms\n",
                self.timestamp, self.query, self.results, self.duration_ms
            )
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs();
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    let millis = now.subsec_millis();

    let days_since_epoch = secs / 86400;
    let (year, month, day) = days_to_date(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        year, month, day, hours, minutes, seconds, millis
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

pub fn access_log_path(graph_path: &Path) -> PathBuf {
    let stem = graph_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("graph");
    let ext = graph_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("json");
    crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.{ext}.access.log"))
}

fn legacy_access_log_path(graph_path: &Path) -> PathBuf {
    let mut path = graph_path.to_path_buf();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("graph");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
    path.set_file_name(format!("{stem}.{ext}.access.log"));
    path
}

fn access_log_fallback_paths(graph_path: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        access_log_path(graph_path),
        legacy_access_log_path(graph_path),
    ];
    match graph_path.extension().and_then(|ext| ext.to_str()) {
        Some("kg") => {
            paths.push(access_log_path(&graph_path.with_extension("json")));
            paths.push(legacy_access_log_path(&graph_path.with_extension("json")));
        }
        Some("json") => {
            paths.push(access_log_path(&graph_path.with_extension("kg")));
            paths.push(legacy_access_log_path(&graph_path.with_extension("kg")));
        }
        _ => {}
    }
    paths
}

pub fn first_existing_access_log_path(graph_path: &Path) -> Option<PathBuf> {
    access_log_fallback_paths(graph_path)
        .into_iter()
        .find(|path| path.exists())
}

pub fn append_entry(graph_path: &Path, entry: &AccessLogEntry) -> Result<()> {
    let log_path = access_log_path(graph_path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    file.write_all(entry.to_line().as_bytes())?;
    Ok(())
}

pub fn append_hit(graph_path: &Path, user_short_uid: &str, node_id: &str) -> Result<()> {
    crate::kg_sidecar::append_hit_with_uid(graph_path, user_short_uid, node_id)
}

pub fn read_log(graph_path: &Path, limit: usize, show_empty: bool) -> Result<String> {
    let Some(log_path) = first_existing_access_log_path(graph_path) else {
        return Ok(String::from("= access-log\nempty: no entries yet\n"));
    };

    let content = fs::read_to_string(&log_path)?;
    let mut lines: Vec<&str> = content.lines().collect();

    if !show_empty {
        lines.retain(|l| {
            let parts: Vec<&str> = l.split('\t').collect();
            parts.len() >= 4 && parts[3] != "0"
        });
    }

    lines.reverse();
    let taken: Vec<&str> = lines.iter().take(limit).cloned().collect();

    let mut output = vec![String::from("= access-log")];
    output.push(format!("total_entries: {}", lines.len()));
    output.push(format!("showing: {}", taken.len()));

    if !show_empty {
        output.push(String::from(
            "(filtering: showing only queries with results)",
        ));
    }

    output.push("recent_entries:".to_owned());
    for line in &taken {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 5 {
            let timestamp = parts[0];
            let op = parts[1];
            let query = parts[2];
            let results = parts[3];
            let duration = parts[4];
            output.push(format!(
                "- {} | {} | {} | {} results | {}",
                timestamp, op, query, results, duration
            ));
        }
    }

    Ok(output.join("\n"))
}

pub fn log_stats(graph_path: &Path) -> Result<String> {
    let Some(log_path) = first_existing_access_log_path(graph_path) else {
        return Ok(String::from("= access-stats\nno access log found\n"));
    };

    let content = fs::read_to_string(&log_path)?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Ok(String::from("= access-stats\nno entries\n"));
    }

    let mut total_finds = 0;
    let mut total_gets = 0;
    let mut empty_finds = 0;
    let mut total_duration_ms: u128 = 0;
    let mut find_queries: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for line in &lines {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 5 {
            let op = parts[1];
            let results: usize = parts[3].parse().unwrap_or(0);
            let duration: u128 = parts[4].trim_end_matches("ms").parse().unwrap_or(0);
            total_duration_ms += duration;

            if op == "FIND" {
                total_finds += 1;
                let query = parts[2];
                *find_queries.entry(query.to_string()).or_insert(0) += 1;
                if results == 0 {
                    empty_finds += 1;
                }
            } else if op == "GET" {
                total_gets += 1;
            }
        }
    }

    let mut output = vec![String::from("= access-stats")];
    output.push(format!("total_operations: {}", lines.len()));
    output.push(format!("find_operations: {}", total_finds));
    output.push(format!("get_operations: {}", total_gets));
    output.push(format!(
        "empty_queries: {} ({:.1}%)",
        empty_finds,
        if total_finds > 0 {
            (empty_finds as f64 / total_finds as f64) * 100.0
        } else {
            0.0
        }
    ));
    output.push(format!(
        "avg_duration_ms: {:.1}",
        if !lines.is_empty() {
            total_duration_ms as f64 / lines.len() as f64
        } else {
            0.0
        }
    ));

    if !find_queries.is_empty() {
        output.push("top_queries:".to_owned());
        let mut sorted: Vec<_> = find_queries.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (query, count) in sorted.iter().take(10) {
            output.push(format!("- {} ({}x)", query, count));
        }
    }

    Ok(output.join("\n"))
}

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }
}
