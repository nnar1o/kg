use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

fn is_kg_graph(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("kg")
}

pub fn kglog_path(graph_path: &Path) -> Option<PathBuf> {
    if is_kg_graph(graph_path) {
        let stem = graph_path.file_stem()?.to_str()?;
        Some(crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.kglog")))
    } else {
        None
    }
}

pub fn kgindex_path(graph_path: &Path) -> Option<PathBuf> {
    if is_kg_graph(graph_path) {
        let stem = graph_path.file_stem()?.to_str()?;
        Some(crate::cache_paths::cache_root_for_graph(graph_path).join(format!("{stem}.kgindex")))
    } else {
        None
    }
}

pub fn append_hit_with_uid(graph_path: &Path, user_short_uid: &str, node_id: &str) -> Result<()> {
    append_kglog_line(graph_path, user_short_uid, 'H', node_id, None)
}

pub fn append_feedback_with_uid(
    graph_path: &Path,
    user_short_uid: &str,
    node_id: &str,
    feedback: &str,
) -> Result<()> {
    append_kglog_line(graph_path, user_short_uid, 'F', node_id, Some(feedback))
}

pub fn append_warning(graph_path: &Path, warning: &str) -> Result<()> {
    append_kglog_line(graph_path, "kgparse0", 'W', "-", Some(warning))
}

fn normalize_kglog_field(value: &str) -> String {
    value
        .trim()
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn append_kglog_line(
    graph_path: &Path,
    user_short_uid: &str,
    marker: char,
    node_id: &str,
    feedback: Option<&str>,
) -> Result<()> {
    let Some(path) = kglog_path(graph_path) else {
        return Ok(());
    };

    let mut line = format!(
        "{} {} {} {}",
        utc_timestamp(),
        user_short_uid,
        marker,
        node_id
    );
    if let Some(value) = feedback {
        line.push(' ');
        line.push_str(&normalize_kglog_field(value));
    }
    line.push('\n');

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory: {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open kglog file: {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to write kglog file: {}", path.display()))?;
    Ok(())
}

pub fn ensure_kgindex_fresh(graph_path: &Path) -> Result<()> {
    let Some(index_path) = kgindex_path(graph_path) else {
        return Ok(());
    };
    let should_rebuild = if !index_path.exists() {
        true
    } else {
        let graph_mtime = fs::metadata(graph_path)
            .and_then(|m| m.modified())
            .with_context(|| format!("failed to read graph mtime: {}", graph_path.display()))?;
        let index_mtime = fs::metadata(&index_path)
            .and_then(|m| m.modified())
            .with_context(|| format!("failed to read kgindex mtime: {}", index_path.display()))?;
        index_mtime < graph_mtime
    };

    if should_rebuild {
        rebuild_kgindex(graph_path)?;
    }
    Ok(())
}

pub fn lookup_node_line(graph_path: &Path, node_id: &str) -> Option<usize> {
    let index_path = kgindex_path(graph_path)?;
    if !index_path.exists() {
        return None;
    }
    let raw = fs::read_to_string(index_path).ok()?;
    for line in raw.lines() {
        let (id, line_no) = line.split_once(' ')?;
        if id == node_id {
            return line_no.trim().parse::<usize>().ok();
        }
    }
    None
}

pub fn invalidate_kgindex(graph_path: &Path) -> Result<()> {
    let Some(index_path) = kgindex_path(graph_path) else {
        return Ok(());
    };
    if index_path.exists() {
        fs::remove_file(&index_path)
            .with_context(|| format!("failed to remove kgindex: {}", index_path.display()))?;
    }
    Ok(())
}

pub fn rebuild_kgindex(graph_path: &Path) -> Result<()> {
    let Some(index_path) = kgindex_path(graph_path) else {
        return Ok(());
    };
    let raw = fs::read_to_string(graph_path)
        .with_context(|| format!("failed to read graph file: {}", graph_path.display()))?;

    let mut lines = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        if let Some(node_id) = parse_node_id_from_header(line.trim()) {
            lines.push(format!("{} {}", node_id, idx + 1));
        }
    }

    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory: {}", parent.display()))?;
    }

    fs::write(&index_path, format!("{}\n", lines.join("\n")))
        .with_context(|| format!("failed to write kgindex file: {}", index_path.display()))?;
    Ok(())
}

fn parse_node_id_from_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("@ ")?;
    let rest = rest.trim();
    let first_colon = rest.find(':')?;
    let type_token = rest[..first_colon].trim();
    let node_token = rest[first_colon + 1..].trim();
    if node_token.is_empty() {
        return None;
    }
    if node_token.contains(':') {
        return Some(node_token.to_owned());
    }

    let prefix = crate::validate::TYPE_TO_PREFIX
        .iter()
        .find(|(typ, prefix)| {
            *prefix == type_token
                || *typ == type_token
                || crate::validate::canonical_type_code_for(typ)
                    .is_some_and(|code| code == type_token)
        })
        .map(|(_, prefix)| *prefix);

    match prefix {
        Some(prefix) => Some(format!("{prefix}:{node_token}")),
        None => Some(node_token.to_owned()),
    }
}

fn utc_timestamp() -> String {
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

#[cfg(test)]
mod tests {
    use super::{kgindex_path, lookup_node_line, rebuild_kgindex};

    #[test]
    fn rebuild_kgindex_indexes_node_headers_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let graph_path = dir.path().join("fridge.kg");
        std::fs::write(
            &graph_path,
            "@ K:concept:refrigerator\nN Lodowka\nD Desc\n@ P:process:cooling\nN Cooling\nD Desc\n",
        )
        .expect("write graph");

        rebuild_kgindex(&graph_path).expect("build index");
        let index_path = kgindex_path(&graph_path).expect("kgindex path");
        let raw = std::fs::read_to_string(index_path).expect("read index");
        assert!(raw.contains("concept:refrigerator 1"));
        assert!(raw.contains("process:cooling 4"));
    }

    #[test]
    fn lookup_node_line_reads_existing_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let graph_path = dir.path().join("fridge.kg");
        std::fs::write(
            &graph_path,
            "@ K:concept:refrigerator\nN Lodowka\nD Desc\n@ P:process:cooling\nN Cooling\nD Desc\n",
        )
        .expect("write graph");
        rebuild_kgindex(&graph_path).expect("build index");

        assert_eq!(
            lookup_node_line(&graph_path, "concept:refrigerator"),
            Some(1)
        );
        assert_eq!(lookup_node_line(&graph_path, "process:cooling"), Some(4));
        assert_eq!(lookup_node_line(&graph_path, "concept:missing"), None);
    }

    #[test]
    fn rebuild_kgindex_keeps_full_id_for_legacy_header_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let graph_path = dir.path().join("legacy.kg");
        std::fs::write(
            &graph_path,
            "@ concept:refrigerator\nN Lodowka\nD Desc\n@ process:cooling\nN Cooling\nD Desc\n",
        )
        .expect("write graph");

        rebuild_kgindex(&graph_path).expect("build index");
        assert_eq!(
            lookup_node_line(&graph_path, "concept:refrigerator"),
            Some(1)
        );
        assert_eq!(lookup_node_line(&graph_path, "process:cooling"), Some(4));
    }
}
