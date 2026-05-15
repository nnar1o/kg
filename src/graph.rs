#![allow(clippy::manual_contains, clippy::collapsible_if)]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};

const GRAPH_INFO_NODE_ID: &str = "^:graph_info";
const GRAPH_INFO_NODE_TYPE: &str = "^";
const GRAPH_UUID_FACT_PREFIX: &str = "graph_uuid=";
const GRAPH_SCHEMA_VERSION: u32 = 2;
const GRAPH_SCHEMA_VERSION_FACT_PREFIX: &str = "schema_version=";
const KG_TEXT_COMPRESSION_MIN_LEN: usize = 7;

/// Write `data` to `dest` atomically:
/// 1. Write to `dest.tmp`
/// 2. If `dest` already exists, copy it to `dest.bak`
/// 3. Rename `dest.tmp` -> `dest`
fn atomic_write(dest: &Path, data: &str) -> Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dest.with_extension(format!("tmp.{}.{}", std::process::id(), unique));
    fs::write(&tmp, data).with_context(|| format!("failed to write tmp: {}", tmp.display()))?;
    if dest.exists() {
        let bak = backup_bak_path(dest)?;
        if should_refresh_bak(&bak)? {
            fs::copy(dest, &bak)
                .with_context(|| format!("failed to create backup: {}", bak.display()))?;
        }
    }
    fs::rename(&tmp, dest).with_context(|| format!("failed to rename tmp to {}", dest.display()))
}

const BACKUP_BAK_STALE_SECS: u64 = 5 * 60;
const BACKUP_STALE_SECS: u64 = 60 * 60;

fn should_refresh_bak(bak_path: &Path) -> Result<bool> {
    if !bak_path.exists() {
        return Ok(true);
    }
    let modified = fs::metadata(bak_path)
        .and_then(|m| m.modified())
        .with_context(|| format!("failed to read backup mtime: {}", bak_path.display()))?;
    let age_secs = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        .as_secs();
    Ok(age_secs >= BACKUP_BAK_STALE_SECS)
}

fn backup_graph_if_stale(path: &Path, data: &str) -> Result<()> {
    let cache_dir = backup_cache_dir(path)?;
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => stem,
        None => return Ok(()),
    };
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
    let backup_prefix = format!("{stem}.{ext}");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("time went backwards")?
        .as_secs();
    if let Some(latest) = latest_backup_ts(&cache_dir, &backup_prefix)? {
        if now.saturating_sub(latest) < BACKUP_STALE_SECS {
            return Ok(());
        }
    }

    let backup_path = cache_dir.join(format!("{backup_prefix}.bck.{now}.gz"));
    let tmp_path = backup_path.with_extension("tmp");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_bytes())?;
    let encoded = encoder.finish()?;
    fs::write(&tmp_path, encoded)
        .with_context(|| format!("failed to write tmp: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &backup_path)
        .with_context(|| format!("failed to rename tmp to {}", backup_path.display()))?;
    Ok(())
}

fn backup_cache_dir(path: &Path) -> Result<PathBuf> {
    let dir = crate::cache_paths::cache_root_for_graph(path);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache directory: {}", dir.display()))?;
    Ok(dir)
}

fn backup_bak_path(dest: &Path) -> Result<PathBuf> {
    let cache_dir = backup_cache_dir(dest)?;
    let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("graph");
    let ext = dest.extension().and_then(|s| s.to_str()).unwrap_or("json");
    Ok(cache_dir.join(format!("{stem}.{ext}.bak")))
}

fn latest_backup_ts(dir: &Path, stem: &str) -> Result<Option<u64>> {
    let prefix = format!("{stem}.bck.");
    let suffix = ".gz";
    let mut latest = None;
    for entry in fs::read_dir(dir).with_context(|| format!("read dir: {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) || !name.ends_with(suffix) {
            continue;
        }
        let ts_part = &name[prefix.len()..name.len() - suffix.len()];
        if let Ok(ts) = ts_part.parse::<u64>() {
            match latest {
                Some(current) => {
                    if ts > current {
                        latest = Some(ts);
                    }
                }
                None => latest = Some(ts),
            }
        }
    }
    Ok(latest)
}

fn node_type_to_code(node_type: &str) -> &str {
    match node_type {
        "Feature" => "F",
        "Concept" => "K",
        "Interface" => "I",
        "Process" => "P",
        "DataStore" => "D",
        "Attribute" => "A",
        "Entity" => "Y",
        "Note" => "N",
        "Rule" => "R",
        "Convention" => "C",
        "Bug" => "B",
        "Decision" => "Z",
        "OpenQuestion" => "O",
        "Claim" => "Q",
        "Insight" => "W",
        "Reference" => "M",
        "Term" => "T",
        "Status" => "S",
        "Doubt" => "L",
        _ => node_type,
    }
}

fn encode_node_type_token(node_type: &str) -> String {
    let code = node_type_to_code(node_type);
    if code != node_type {
        return code.to_owned();
    }
    if code_to_node_type(node_type) != node_type {
        return format!("={node_type}");
    }
    node_type.to_owned()
}

fn code_to_node_type(code: &str) -> &str {
    match code {
        "F" => "Feature",
        "K" => "Concept",
        "I" => "Interface",
        "P" => "Process",
        "D" => "DataStore",
        "A" => "Attribute",
        "Y" => "Entity",
        "N" => "Note",
        "R" => "Rule",
        "C" => "Convention",
        "B" => "Bug",
        "Z" => "Decision",
        "O" => "OpenQuestion",
        "Q" => "Claim",
        "W" => "Insight",
        "M" => "Reference",
        "T" => "Term",
        "S" => "Status",
        "L" => "Doubt",
        _ => code,
    }
}

fn decode_node_type_token(token: &str) -> String {
    token
        .strip_prefix('=')
        .map(str::to_owned)
        .unwrap_or_else(|| code_to_node_type(token).to_owned())
}

fn relation_to_code(relation: &str) -> &str {
    match relation {
        "DOCUMENTED_IN" | "DOCUMENTS" => "D",
        "HAS" => "H",
        "TRIGGERS" => "T",
        "AFFECTED_BY" | "AFFECTS" => "A",
        "READS_FROM" | "READS" => "R",
        "GOVERNED_BY" | "GOVERNS" => "G",
        "DEPENDS_ON" => "O",
        "AVAILABLE_IN" => "I",
        "SUPPORTS" => "S",
        "SUMMARIZES" => "U",
        "RELATED_TO" => "L",
        "CONTRADICTS" => "V",
        "CREATED_BY" | "CREATES" => "C",
        _ => relation,
    }
}

fn code_to_relation(code: &str) -> &str {
    match code {
        "D" => "DOCUMENTED_IN",
        "H" => "HAS",
        "T" => "TRIGGERS",
        "A" => "AFFECTED_BY",
        "R" => "READS_FROM",
        "G" => "GOVERNED_BY",
        "O" => "DEPENDS_ON",
        "I" => "AVAILABLE_IN",
        "S" => "SUPPORTS",
        "U" => "SUMMARIZES",
        "L" => "RELATED_TO",
        "V" => "CONTRADICTS",
        "C" => "CREATED_BY",
        _ => code,
    }
}

fn canonicalize_bidirectional_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

fn is_score_component_label(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('C'))
        && chars.clone().next().is_some()
        && chars.all(|ch| ch.is_ascii_digit())
}

fn sort_case_insensitive(values: &[String]) -> Vec<String> {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| {
        let la = a.to_ascii_lowercase();
        let lb = b.to_ascii_lowercase();
        la.cmp(&lb).then_with(|| a.cmp(b))
    });
    sorted
}

fn decode_kg_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn escape_kg_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_text_field(value: &str) -> String {
    decode_kg_text(value)
}

fn push_text_line(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push(' ');
    out.push_str(&escape_kg_text(value));
    out.push('\n');
}

#[derive(Debug, Clone)]
struct KgCompressionCandidate {
    token: usize,
    value: String,
    first_line: usize,
    first_col: usize,
}

#[derive(Debug, Default, Clone, Copy)]
struct KgCompressionStats {
    original_bytes: usize,
    compressed_bytes: usize,
    dictionary_entries: usize,
}

#[derive(Debug, Clone)]
struct LineOccurrence {
    line_idx: usize,
    col_idx: usize,
}

fn decode_kg_token_reference_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('`')?;
    let (token, value) = rest.split_once(' ')?;
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((token.to_owned(), value.to_owned()))
}

fn expand_kg_tokens_in_line(
    line: &str,
    dictionary: &std::collections::HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut idx = 0;

    while idx < chars.len() {
        if chars[idx] != '`' {
            out.push(chars[idx]);
            idx += 1;
            continue;
        }

        let start = idx;
        idx += 1;
        let mut token = String::new();
        while idx < chars.len() && chars[idx].is_ascii_digit() {
            token.push(chars[idx]);
            idx += 1;
        }

        if !token.is_empty() && idx < chars.len() && chars[idx] == '`' {
            idx += 1;
            if let Some(value) = dictionary.get(&token) {
                out.push_str(value);
            } else {
                out.push('`');
                out.push_str(&token);
                out.push('`');
            }
            continue;
        }

        out.push('`');
        out.push_str(&token);
        if idx < chars.len() {
            out.push(chars[idx]);
            idx += 1;
        } else if start + 1 < chars.len() {
            // Keep the literal backtick when it does not form a token.
        }
    }

    out
}

fn expand_kg_tokens(raw: &str) -> String {
    let mut dictionary = std::collections::HashMap::new();
    let mut out = String::new();

    for line in raw.lines() {
        if let Some((token, value)) = decode_kg_token_reference_line(line) {
            dictionary.insert(token, value);
            continue;
        }
        out.push_str(&expand_kg_tokens_in_line(line, &dictionary));
        out.push('\n');
    }

    out
}

fn node_header_type_token(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("@ ")?;
    let (type_token, _) = rest.split_once(':')?;
    Some(type_token.trim())
}

fn is_generated_node_block_header(line: &str) -> bool {
    node_header_type_token(line).is_some_and(|token| token.starts_with('G'))
}

fn collect_generated_text_lines(raw: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut in_block = false;
    let mut generated_block = false;

    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            in_block = false;
            generated_block = false;
            continue;
        }

        if trimmed.starts_with("@ ") {
            in_block = true;
            generated_block = is_generated_node_block_header(trimmed);
            continue;
        }

        if in_block && generated_block {
            lines.push((idx, line.to_owned()));
        }
    }

    lines
}

fn extend_repeated_seed(
    seed: &str,
    occurrences: &[LineOccurrence],
    source_lines: &[(usize, String)],
) -> Option<String> {
    let seed_chars: Vec<char> = seed.chars().collect();
    let mut candidate = seed_chars.clone();

    loop {
        let mut next_char: Option<char> = None;

        for occurrence in occurrences {
            let (_, line) = source_lines
                .iter()
                .find(|(line_idx, _)| *line_idx == occurrence.line_idx)?;
            let chars: Vec<char> = line.chars().collect();
            let next_index = occurrence.col_idx + candidate.len();
            let Some(&ch) = chars.get(next_index) else {
                return Some(candidate.into_iter().collect());
            };
            if ch == '`' {
                return Some(candidate.into_iter().collect());
            }
            match next_char {
                Some(prev) if prev != ch => return Some(candidate.into_iter().collect()),
                None => next_char = Some(ch),
                _ => {}
            }
        }

        let Some(ch) = next_char else {
            return Some(candidate.into_iter().collect());
        };
        candidate.push(ch);
        if candidate.len() > seed_chars.len() + 256 {
            return Some(candidate.into_iter().collect());
        }
    }
}

fn discover_kg_compression_candidates(
    source_lines: &[(usize, String)],
    min_len: usize,
) -> Vec<KgCompressionCandidate> {
    let mut seeds: std::collections::HashMap<String, Vec<LineOccurrence>> =
        std::collections::HashMap::new();

    for (line_idx, line) in source_lines {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < min_len {
            continue;
        }

        for start in 0..=chars.len() - min_len {
            if chars[start..start + min_len].iter().any(|ch| *ch == '`') {
                continue;
            }
            let seed: String = chars[start..start + min_len].iter().collect();
            seeds.entry(seed).or_default().push(LineOccurrence {
                line_idx: *line_idx,
                col_idx: start,
            });
        }
    }

    let mut discovered: std::collections::HashMap<String, KgCompressionCandidate> =
        std::collections::HashMap::new();

    for (seed, occurrences) in seeds {
        if occurrences.len() < 2 {
            continue;
        }

        let Some(value) = extend_repeated_seed(&seed, &occurrences, source_lines) else {
            continue;
        };
        if value.chars().count() < min_len || value.contains('`') {
            continue;
        }

        let first = occurrences
            .iter()
            .min_by_key(|occ| (occ.line_idx, occ.col_idx))
            .expect("at least one occurrence");

        discovered
            .entry(value.clone())
            .and_modify(|candidate| {
                let first_pos = (first.line_idx, first.col_idx);
                let current_pos = (candidate.first_line, candidate.first_col);
                if first_pos < current_pos {
                    candidate.first_line = first.line_idx;
                    candidate.first_col = first.col_idx;
                }
            })
            .or_insert(KgCompressionCandidate {
                token: 0,
                value,
                first_line: first.line_idx,
                first_col: first.col_idx,
            });
    }

    let mut candidates: Vec<KgCompressionCandidate> = discovered.into_values().collect();
    candidates.sort_by(|a, b| {
        b.value
            .chars()
            .count()
            .cmp(&a.value.chars().count())
            .then_with(|| a.first_line.cmp(&b.first_line))
            .then_with(|| a.first_col.cmp(&b.first_col))
            .then_with(|| a.value.cmp(&b.value))
    });

    let mut filtered: Vec<KgCompressionCandidate> = Vec::new();
    'candidate: for candidate in candidates {
        for kept in &filtered {
            if kept.value.contains(&candidate.value) {
                continue 'candidate;
            }
        }
        filtered.push(candidate);
    }

    filtered.sort_by(|a, b| {
        a.first_line
            .cmp(&b.first_line)
            .then_with(|| b.value.chars().count().cmp(&a.value.chars().count()))
            .then_with(|| a.first_col.cmp(&b.first_col))
            .then_with(|| a.value.cmp(&b.value))
    });

    for (idx, candidate) in filtered.iter_mut().enumerate() {
        candidate.token = idx + 1;
    }

    filtered
}

fn replace_kg_text_with_tokens(line: &str, candidates: &[KgCompressionCandidate]) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut idx = 0;

    while idx < chars.len() {
        let mut best: Option<&KgCompressionCandidate> = None;

        for candidate in candidates {
            let candidate_chars: Vec<char> = candidate.value.chars().collect();
            if idx + candidate_chars.len() > chars.len() {
                continue;
            }
            if chars[idx..idx + candidate_chars.len()] != candidate_chars[..] {
                continue;
            }
            match best {
                Some(current) if current.value.chars().count() >= candidate_chars.len() => {}
                _ => best = Some(candidate),
            }
        }

        if let Some(candidate) = best {
            out.push('`');
            out.push_str(&candidate.token.to_string());
            out.push('`');
            idx += candidate.value.chars().count();
            continue;
        }

        out.push(chars[idx]);
        idx += 1;
    }

    out
}

fn compress_kg_text(raw: &str, min_len: usize) -> (String, KgCompressionStats) {
    let source_lines = collect_generated_text_lines(raw);
    let candidates = discover_kg_compression_candidates(&source_lines, min_len);

    let mut defs_by_line: std::collections::HashMap<usize, Vec<&KgCompressionCandidate>> =
        std::collections::HashMap::new();
    for candidate in &candidates {
        defs_by_line
            .entry(candidate.first_line)
            .or_default()
            .push(candidate);
    }
    for defs in defs_by_line.values_mut() {
        defs.sort_by(|a, b| {
            b.value
                .chars()
                .count()
                .cmp(&a.value.chars().count())
                .then_with(|| a.token.cmp(&b.token))
        });
    }

    let compressed_source_lines: std::collections::HashSet<usize> =
        source_lines.iter().map(|(idx, _)| *idx).collect();
    let mut compressed = String::new();

    for (idx, line) in raw.lines().enumerate() {
        if let Some(defs) = defs_by_line.get(&idx) {
            for def in defs {
                compressed.push('`');
                compressed.push_str(&def.token.to_string());
                compressed.push(' ');
                compressed.push_str(&def.value);
                compressed.push('\n');
            }
        }

        let rendered = if compressed_source_lines.contains(&idx) {
            replace_kg_text_with_tokens(line, &candidates)
        } else {
            line.to_owned()
        };
        compressed.push_str(&rendered);
        compressed.push('\n');
    }

    let original_bytes = raw.len();
    let compressed_bytes = compressed.len();
    let dictionary_entries = candidates.len();

    (
        if compressed_bytes < original_bytes {
            compressed
        } else {
            raw.to_owned()
        },
        KgCompressionStats {
            original_bytes,
            compressed_bytes,
            dictionary_entries,
        },
    )
}

fn dedupe_case_insensitive(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for value in values {
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(value);
        }
    }
    out
}

fn parse_utc_timestamp(value: &str) -> bool {
    if value.len() != 20 {
        return false;
    }
    let bytes = value.as_bytes();
    let is_digit = |idx: usize| bytes.get(idx).is_some_and(|b| b.is_ascii_digit());
    if !(is_digit(0)
        && is_digit(1)
        && is_digit(2)
        && is_digit(3)
        && bytes.get(4) == Some(&b'-')
        && is_digit(5)
        && is_digit(6)
        && bytes.get(7) == Some(&b'-')
        && is_digit(8)
        && is_digit(9)
        && bytes.get(10) == Some(&b'T')
        && is_digit(11)
        && is_digit(12)
        && bytes.get(13) == Some(&b':')
        && is_digit(14)
        && is_digit(15)
        && bytes.get(16) == Some(&b':')
        && is_digit(17)
        && is_digit(18)
        && bytes.get(19) == Some(&b'Z'))
    {
        return false;
    }

    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let hour = value[11..13].parse::<u32>().ok();
    let minute = value[14..16].parse::<u32>().ok();
    let second = value[17..19].parse::<u32>().ok();
    matches!(month, Some(1..=12))
        && matches!(day, Some(1..=31))
        && matches!(hour, Some(0..=23))
        && matches!(minute, Some(0..=59))
        && matches!(second, Some(0..=59))
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn strict_kg_mode() -> bool {
    let Ok(value) = std::env::var("KG_STRICT_FORMAT") else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn abbreviated_line(line: &str) -> String {
    const MAX_CHARS: usize = 160;
    let trimmed = line.trim();
    let mut out = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= MAX_CHARS {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn line_fragment(line: &str) -> String {
    let snippet = abbreviated_line(line);
    if snippet.is_empty() {
        "fragment: <empty line>".to_owned()
    } else {
        format!("fragment: {snippet}")
    }
}

fn json_error_detail(label: &str, path: &Path, raw: &str, error: &serde_json::Error) -> String {
    let line_no = error.line();
    let column = error.column();
    let fragment = raw
        .lines()
        .nth(line_no.saturating_sub(1))
        .map(line_fragment)
        .unwrap_or_else(|| "fragment: <unavailable>".to_owned());
    format!(
        "{label}: {} at line {line_no}, column {column}: {error}\n{fragment}",
        path.display()
    )
}

fn validate_len(
    line_no: usize,
    field: &str,
    value: &str,
    raw_line: &str,
    min: usize,
    max: usize,
    strict: bool,
) -> Result<()> {
    let len = value.chars().count();
    if strict && (len < min || len > max) {
        return Err(anyhow::anyhow!(
            "invalid {field} length at line {line_no}: expected {min}..={max}, got {len}\n{}",
            line_fragment(raw_line)
        ));
    }
    Ok(())
}

fn enforce_field_order(
    line_no: usize,
    key: &str,
    rank: u8,
    last_rank: &mut u8,
    section: &str,
    raw_line: &str,
    strict: bool,
) -> Result<()> {
    if strict && rank < *last_rank {
        return Err(anyhow::anyhow!(
            "invalid field order at line {line_no}: {key} in {section} block\n{}",
            line_fragment(raw_line)
        ));
    }
    if rank > *last_rank {
        *last_rank = rank;
    }
    Ok(())
}

fn field_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if line == key {
        Some("")
    } else {
        line.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(' '))
    }
}

fn fail_or_warn(strict: bool, warnings: &mut Vec<String>, message: String) -> Result<()> {
    if strict {
        Err(anyhow::anyhow!(message))
    } else {
        warnings.push(message);
        Ok(())
    }
}

#[cfg(test)]
fn parse_kg(raw: &str, graph_name: &str, strict: bool) -> Result<GraphFile> {
    Ok(parse_kg_with_warnings(raw, graph_name, strict)?.0)
}

fn parse_kg_with_warnings(
    raw: &str,
    graph_name: &str,
    strict: bool,
) -> Result<(GraphFile, Vec<String>)> {
    let mut graph = GraphFile::new(graph_name);
    let mut warnings = Vec::new();
    let mut current_node: Option<Node> = None;
    let mut current_note: Option<Note> = None;
    let mut current_edge_index: Option<usize> = None;
    let mut last_node_rank: u8 = 0;
    let mut last_note_rank: u8 = 0;
    let mut last_edge_rank: u8 = 0;

    for (idx, line) in raw.lines().enumerate() {
        let line_no = idx + 1;
        let raw_line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("@ ") {
            if let Some(note) = current_note.take() {
                graph.notes.push(note);
            }
            if let Some(node) = current_node.take() {
                graph.nodes.push(node);
            }
            let Some((type_code, node_id)) = rest.split_once(':') else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("invalid node header at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            let decoded_type = decode_node_type_token(type_code.trim());
            let parsed_id = {
                let raw_id = node_id.trim();
                if crate::validate::is_generated_node_type(&decoded_type) {
                    if let Some((head, suffix)) = raw_id.split_once(':') {
                        if head == decoded_type {
                            suffix.to_owned()
                        } else {
                            raw_id.to_owned()
                        }
                    } else {
                        raw_id.to_owned()
                    }
                } else if type_code.trim().starts_with('=') && raw_id.contains(':') {
                    raw_id.to_owned()
                } else if raw_id.contains(':') {
                    crate::validate::normalize_node_id(raw_id)
                } else if code_to_node_type(type_code.trim()) != type_code.trim() {
                    crate::validate::normalize_node_id(&format!("{}:{raw_id}", type_code.trim()))
                } else {
                    format!("{}:{raw_id}", decoded_type)
                }
            };
            current_node = Some(Node {
                id: parsed_id,
                r#type: decoded_type,
                name: String::new(),
                properties: NodeProperties::default(),
                source_files: Vec::new(),
            });
            current_edge_index = None;
            last_node_rank = 0;
            last_edge_rank = 0;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("! ") {
            if let Some(node) = current_node.take() {
                graph.nodes.push(node);
            }
            if let Some(note) = current_note.take() {
                graph.notes.push(note);
            }
            let mut parts = rest.split_whitespace();
            let Some(id) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("invalid note header at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            let Some(node_id) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("invalid note header at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            current_note = Some(Note {
                id: id.to_owned(),
                node_id: node_id.to_owned(),
                ..Default::default()
            });
            current_edge_index = None;
            last_note_rank = 0;
            continue;
        }

        if let Some(note) = current_note.as_mut() {
            if let Some(rest) = field_value(raw_line, "b") {
                enforce_field_order(
                    line_no,
                    "b",
                    1,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                note.body = parse_text_field(rest);
                continue;
            }
            if let Some(rest) = field_value(raw_line, "t") {
                enforce_field_order(
                    line_no,
                    "t",
                    2,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                let value = parse_text_field(rest);
                if !value.is_empty() {
                    note.tags.push(value);
                }
                continue;
            }
            if let Some(rest) = field_value(raw_line, "a") {
                enforce_field_order(
                    line_no,
                    "a",
                    3,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                note.author = parse_text_field(rest);
                continue;
            }
            if let Some(rest) = field_value(raw_line, "e") {
                enforce_field_order(
                    line_no,
                    "e",
                    4,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                note.created_at = rest.trim().to_owned();
                continue;
            }
            if let Some(rest) = field_value(raw_line, "p") {
                enforce_field_order(
                    line_no,
                    "p",
                    5,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                note.provenance = parse_text_field(rest);
                continue;
            }
            if let Some(rest) = field_value(raw_line, "s") {
                enforce_field_order(
                    line_no,
                    "s",
                    6,
                    &mut last_note_rank,
                    "note",
                    raw_line,
                    strict,
                )?;
                let value = parse_text_field(rest);
                if !value.is_empty() {
                    note.source_files.push(value);
                }
                continue;
            }
            fail_or_warn(
                strict,
                &mut warnings,
                format!("unrecognized note line at {line_no}: {trimmed}"),
            )?;
            continue;
        }

        let Some(node) = current_node.as_mut() else {
            fail_or_warn(
                strict,
                &mut warnings,
                format!("unexpected line before first node at line {line_no}: {trimmed}"),
            )?;
            continue;
        };

        if let Some(rest) = field_value(raw_line, "N") {
            enforce_field_order(
                line_no,
                "N",
                1,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = parse_text_field(rest);
            validate_len(line_no, "N", &value, raw_line, 1, 120, strict)?;
            node.name = value;
            continue;
        }
        if let Some(rest) = field_value(raw_line, "D") {
            enforce_field_order(
                line_no,
                "D",
                2,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = parse_text_field(rest);
            validate_len(line_no, "D", &value, raw_line, 1, 200, strict)?;
            node.properties.description = value;
            continue;
        }
        if let Some(rest) = field_value(raw_line, "A") {
            enforce_field_order(
                line_no,
                "A",
                3,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = parse_text_field(rest);
            validate_len(line_no, "A", &value, raw_line, 1, 80, strict)?;
            node.properties.alias.push(value);
            continue;
        }
        if let Some(rest) = field_value(raw_line, "F") {
            enforce_field_order(
                line_no,
                "F",
                4,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = parse_text_field(rest);
            validate_len(line_no, "F", &value, raw_line, 1, 200, strict)?;
            node.properties.key_facts.push(value);
            continue;
        }
        if let Some(rest) = field_value(raw_line, "E") {
            enforce_field_order(
                line_no,
                "E",
                5,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = rest.trim();
            if !value.is_empty() && !parse_utc_timestamp(value) {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "invalid E timestamp at line {line_no}: expected YYYY-MM-DDTHH:MM:SSZ\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            }
            node.properties.created_at = value.to_owned();
            continue;
        }
        if let Some(rest) = field_value(raw_line, "C") {
            enforce_field_order(
                line_no,
                "C",
                6,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            if !rest.trim().is_empty() {
                node.properties.confidence = rest.trim().parse::<f64>().ok();
            }
            continue;
        }
        if let Some(rest) = field_value(raw_line, "V") {
            enforce_field_order(
                line_no,
                "V",
                7,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            if let Ok(value) = rest.trim().parse::<f64>() {
                node.properties.importance = value;
            }
            continue;
        }
        if let Some(rest) = field_value(raw_line, "P") {
            enforce_field_order(
                line_no,
                "P",
                8,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            node.properties.provenance = parse_text_field(rest);
            continue;
        }
        if let Some(rest) = field_value(raw_line, "S") {
            enforce_field_order(
                line_no,
                "S",
                10,
                &mut last_node_rank,
                "node",
                raw_line,
                strict,
            )?;
            let value = parse_text_field(rest);
            validate_len(line_no, "S", &value, raw_line, 1, 200, strict)?;
            node.source_files.push(value);
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("> ") {
            let mut parts = rest.split_whitespace();
            let Some(relation) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("missing relation in edge at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            let Some(target_id) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("missing target id in edge at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            graph.edges.push(Edge {
                source_id: node.id.clone(),
                relation: code_to_relation(relation).to_owned(),
                target_id: target_id.to_owned(),
                properties: EdgeProperties::default(),
            });
            current_edge_index = Some(graph.edges.len() - 1);
            last_edge_rank = 0;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("= ") {
            let mut parts = rest.split_whitespace();
            let Some(relation) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("missing relation in bidirectional edge at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            let Some(target_id) = parts.next() else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!("missing target id in bidirectional edge at line {line_no}: {trimmed}"),
                )?;
                current_edge_index = None;
                continue;
            };
            let relation = code_to_relation(relation).to_owned();
            if relation != "~" {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "invalid bidirectional relation at line {line_no}: expected '~', got '{}'",
                        relation
                    ),
                )?;
                current_edge_index = None;
                continue;
            }

            let target_id = target_id.to_owned();
            let (source_id, target_id) = canonicalize_bidirectional_pair(&node.id, &target_id);
            graph.edges.push(Edge {
                source_id,
                relation,
                target_id,
                properties: EdgeProperties {
                    bidirectional: true,
                    ..EdgeProperties::default()
                },
            });
            current_edge_index = Some(graph.edges.len() - 1);
            last_edge_rank = 0;
            continue;
        }

        if let Some(rest) = field_value(raw_line, "d") {
            enforce_field_order(
                line_no,
                "d",
                1,
                &mut last_edge_rank,
                "edge",
                raw_line,
                strict,
            )?;
            let Some(edge_idx) = current_edge_index else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "edge detail without preceding edge at line {line_no}\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            };
            let trimmed_rest = rest.trim();
            let mut parts = trimmed_rest.split_whitespace();
            if let (Some(label), Some(raw_score), None) = (parts.next(), parts.next(), parts.next())
            {
                if is_score_component_label(label) {
                    let score = raw_score.parse::<f64>().map_err(|_| {
                        anyhow::anyhow!(
                            "invalid score component value at line {line_no}: expected number in '{}', got '{}'",
                            line_fragment(raw_line),
                            raw_score
                        )
                    })?;
                    graph.edges[edge_idx]
                        .properties
                        .score_components
                        .insert(label.to_owned(), score);
                    continue;
                }
            }

            let value = parse_text_field(rest);
            validate_len(line_no, "d", &value, raw_line, 1, 200, strict)?;
            graph.edges[edge_idx].properties.detail = value;
            continue;
        }

        if let Some(rest) = field_value(raw_line, "i") {
            enforce_field_order(
                line_no,
                "i",
                2,
                &mut last_edge_rank,
                "edge",
                raw_line,
                strict,
            )?;
            let Some(edge_idx) = current_edge_index else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "edge valid_from without preceding edge at line {line_no}\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            };
            let value = rest.trim();
            if !value.is_empty() && !parse_utc_timestamp(value) {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "invalid i timestamp at line {line_no}: expected YYYY-MM-DDTHH:MM:SSZ\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            }
            graph.edges[edge_idx].properties.valid_from = value.to_owned();
            continue;
        }

        if let Some(rest) = field_value(raw_line, "x") {
            enforce_field_order(
                line_no,
                "x",
                3,
                &mut last_edge_rank,
                "edge",
                raw_line,
                strict,
            )?;
            let Some(edge_idx) = current_edge_index else {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "edge valid_to without preceding edge at line {line_no}\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            };
            let value = rest.trim();
            if !value.is_empty() && !parse_utc_timestamp(value) {
                fail_or_warn(
                    strict,
                    &mut warnings,
                    format!(
                        "invalid x timestamp at line {line_no}: expected YYYY-MM-DDTHH:MM:SSZ\n{}",
                        line_fragment(raw_line)
                    ),
                )?;
                continue;
            }
            graph.edges[edge_idx].properties.valid_to = value.to_owned();
            continue;
        }

        if let Some(rest) = field_value(raw_line, "-") {
            let (key, value) = rest
                .split_once(char::is_whitespace)
                .map(|(key, value)| (key.trim(), value))
                .unwrap_or((rest.trim(), ""));
            let is_edge_custom = matches!(
                key,
                "edge_feedback_score" | "edge_feedback_count" | "edge_feedback_last_ts_ms"
            );
            if is_edge_custom {
                enforce_field_order(
                    line_no,
                    "-",
                    4,
                    &mut last_edge_rank,
                    "edge",
                    raw_line,
                    strict,
                )?;
            } else {
                enforce_field_order(
                    line_no,
                    "-",
                    9,
                    &mut last_node_rank,
                    "node",
                    raw_line,
                    strict,
                )?;
            }
            match key {
                "domain_area" => node.properties.domain_area = parse_text_field(value),
                "scan" => {
                    node.properties.scan = parse_boolish(value);
                }
                "scan_ignore_unknown" => {
                    node.properties.scan_ignore_unknown = parse_boolish(value);
                }
                "feedback_score" => {
                    node.properties.feedback_score = value.trim().parse::<f64>().unwrap_or(0.0)
                }
                "feedback_count" => {
                    node.properties.feedback_count = value.trim().parse::<u64>().unwrap_or(0)
                }
                "feedback_last_ts_ms" => {
                    node.properties.feedback_last_ts_ms = value.trim().parse::<u64>().ok()
                }
                "edge_feedback_score" => {
                    if let Some(edge_idx) = current_edge_index {
                        graph.edges[edge_idx].properties.feedback_score =
                            value.trim().parse::<f64>().unwrap_or(0.0);
                    }
                }
                "edge_feedback_count" => {
                    if let Some(edge_idx) = current_edge_index {
                        graph.edges[edge_idx].properties.feedback_count =
                            value.trim().parse::<u64>().unwrap_or(0);
                    }
                }
                "edge_feedback_last_ts_ms" => {
                    if let Some(edge_idx) = current_edge_index {
                        graph.edges[edge_idx].properties.feedback_last_ts_ms =
                            value.trim().parse::<u64>().ok();
                    }
                }
                _ => {}
            }
            continue;
        }

        fail_or_warn(
            strict,
            &mut warnings,
            format!("unrecognized line at {line_no}: {trimmed}"),
        )?;
    }

    if let Some(node) = current_node.take() {
        graph.nodes.push(node);
    }
    if let Some(note) = current_note.take() {
        graph.notes.push(note);
    }

    for node in &mut graph.nodes {
        node.properties.alias =
            sort_case_insensitive(&dedupe_case_insensitive(node.properties.alias.clone()));
        node.properties.key_facts =
            sort_case_insensitive(&dedupe_case_insensitive(node.properties.key_facts.clone()));
        node.source_files =
            sort_case_insensitive(&dedupe_case_insensitive(node.source_files.clone()));
    }

    graph.edges.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.relation.cmp(&b.relation))
            .then_with(|| a.target_id.cmp(&b.target_id))
            .then_with(|| a.properties.bidirectional.cmp(&b.properties.bidirectional))
            .then_with(|| a.properties.detail.cmp(&b.properties.detail))
    });

    for note in &mut graph.notes {
        note.tags = sort_case_insensitive(&dedupe_case_insensitive(note.tags.clone()));
        note.source_files =
            sort_case_insensitive(&dedupe_case_insensitive(note.source_files.clone()));
    }
    graph.notes.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.node_id.cmp(&b.node_id))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    graph.refresh_counts();
    Ok((graph, warnings))
}

fn serialize_kg(graph: &GraphFile) -> String {
    let mut out = String::new();
    let mut nodes = graph.nodes.clone();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    for node in nodes {
        let generated = crate::validate::is_generated_node_type(&node.r#type);
        out.push_str(&format!(
            "@ {}:{}\n",
            encode_node_type_token(&node.r#type),
            display_node_id(&node.id, &node.r#type)
        ));
        if !node.name.is_empty() {
            push_text_line(&mut out, "N", &node.name);
        }
        if !node.properties.description.is_empty() {
            push_text_line(&mut out, "D", &node.properties.description);
        }

        for alias in sort_case_insensitive(&node.properties.alias) {
            push_text_line(&mut out, "A", &alias);
        }
        for fact in sort_case_insensitive(&node.properties.key_facts) {
            push_text_line(&mut out, "F", &fact);
        }

        if !generated {
            if !node.properties.created_at.is_empty() {
                out.push_str(&format!("E {}\n", node.properties.created_at));
            }
            if let Some(confidence) = node.properties.confidence {
                out.push_str(&format!("C {}\n", confidence));
            }
            out.push_str(&format!("V {}\n", node.properties.importance));
            if !node.properties.provenance.is_empty() {
                push_text_line(&mut out, "P", &node.properties.provenance);
            }
            if !node.properties.domain_area.is_empty() {
                out.push_str("- domain_area ");
                out.push_str(&escape_kg_text(&node.properties.domain_area));
                out.push('\n');
            }
            if let Some(scan) = node.properties.scan {
                out.push_str(&format!("- scan {}\n", scan));
            }
            if let Some(scan_ignore_unknown) = node.properties.scan_ignore_unknown {
                out.push_str(&format!("- scan_ignore_unknown {}\n", scan_ignore_unknown));
            }
            if node.properties.feedback_score != 0.0 {
                out.push_str(&format!(
                    "- feedback_score {}\n",
                    node.properties.feedback_score
                ));
            }
            if node.properties.feedback_count != 0 {
                out.push_str(&format!(
                    "- feedback_count {}\n",
                    node.properties.feedback_count
                ));
            }
            if let Some(ts) = node.properties.feedback_last_ts_ms {
                out.push_str(&format!("- feedback_last_ts_ms {}\n", ts));
            }

            for source in sort_case_insensitive(&node.source_files) {
                push_text_line(&mut out, "S", &source);
            }
        }

        let mut edges: Vec<Edge> = graph
            .edges
            .iter()
            .filter(|edge| edge.source_id == node.id)
            .cloned()
            .collect();
        edges.sort_by(|a, b| {
            a.relation
                .cmp(&b.relation)
                .then_with(|| a.target_id.cmp(&b.target_id))
                .then_with(|| a.properties.bidirectional.cmp(&b.properties.bidirectional))
                .then_with(|| a.properties.detail.cmp(&b.properties.detail))
        });

        for edge in edges {
            let op = if edge.properties.bidirectional && edge.relation == "~" {
                "="
            } else {
                ">"
            };
            out.push_str(&format!(
                "{} {} {}\n",
                op,
                relation_to_code(&edge.relation),
                canonical_node_id_for_storage(&edge.target_id)
            ));
            for (label, score) in &edge.properties.score_components {
                out.push_str(&format!("d {} {:.6}\n", label, score));
            }
            if !edge.properties.detail.is_empty() {
                push_text_line(&mut out, "d", &edge.properties.detail);
            }
            if !edge.properties.valid_from.is_empty() {
                out.push_str(&format!("i {}\n", edge.properties.valid_from));
            }
            if !edge.properties.valid_to.is_empty() {
                out.push_str(&format!("x {}\n", edge.properties.valid_to));
            }
            if edge.properties.feedback_score != 0.0 {
                out.push_str(&format!(
                    "- edge_feedback_score {}\n",
                    edge.properties.feedback_score
                ));
            }
            if edge.properties.feedback_count != 0 {
                out.push_str(&format!(
                    "- edge_feedback_count {}\n",
                    edge.properties.feedback_count
                ));
            }
            if let Some(ts) = edge.properties.feedback_last_ts_ms {
                out.push_str(&format!("- edge_feedback_last_ts_ms {}\n", ts));
            }
        }

        out.push('\n');
    }

    let mut notes = graph.notes.clone();
    notes.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.node_id.cmp(&b.node_id))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    for note in notes {
        out.push_str(&format!(
            "! {} {}\n",
            note.id,
            canonical_node_id_for_storage(&note.node_id)
        ));
        push_text_line(&mut out, "b", &note.body);
        for tag in sort_case_insensitive(&note.tags) {
            push_text_line(&mut out, "t", &tag);
        }
        if !note.author.is_empty() {
            push_text_line(&mut out, "a", &note.author);
        }
        if !note.created_at.is_empty() {
            out.push_str(&format!("e {}\n", note.created_at));
        }
        if !note.provenance.is_empty() {
            push_text_line(&mut out, "p", &note.provenance);
        }
        for source in sort_case_insensitive(&note.source_files) {
            push_text_line(&mut out, "s", &source);
        }
        out.push('\n');
    }

    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFile {
    pub metadata: Metadata,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    #[serde(default = "default_graph_schema_version")]
    pub schema_version: u32,
    pub version: String,
    pub description: String,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub name: String,
    #[serde(default)]
    pub properties: NodeProperties,
    #[serde(default)]
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProperties {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub domain_area: String,
    #[serde(default)]
    pub provenance: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default = "default_importance")]
    pub importance: f64,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub alias: Vec<String>,
    #[serde(default)]
    pub valid_from: String,
    #[serde(default)]
    pub valid_to: String,
    #[serde(default)]
    pub scan: Option<bool>,
    #[serde(default)]
    pub scan_ignore_unknown: Option<bool>,
    #[serde(default)]
    pub feedback_score: f64,
    #[serde(default)]
    pub feedback_count: u64,
    #[serde(default)]
    pub feedback_last_ts_ms: Option<u64>,
}

fn default_importance() -> f64 {
    0.5
}

fn default_graph_schema_version() -> u32 {
    1
}

impl Default for NodeProperties {
    fn default() -> Self {
        Self {
            description: String::new(),
            domain_area: String::new(),
            provenance: String::new(),
            confidence: None,
            created_at: String::new(),
            importance: default_importance(),
            key_facts: Vec::new(),
            alias: Vec::new(),
            valid_from: String::new(),
            valid_to: String::new(),
            scan: None,
            scan_ignore_unknown: None,
            feedback_score: 0.0,
            feedback_count: 0,
            feedback_last_ts_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
    #[serde(default)]
    pub properties: EdgeProperties,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgeProperties {
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub valid_from: String,
    #[serde(default)]
    pub valid_to: String,
    #[serde(default)]
    pub feedback_score: f64,
    #[serde(default)]
    pub feedback_count: u64,
    #[serde(default)]
    pub feedback_last_ts_ms: Option<u64>,
    #[serde(default)]
    pub bidirectional: bool,
    #[serde(default)]
    pub score_components: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub node_id: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub provenance: String,
    #[serde(default)]
    pub source_files: Vec<String>,
}

impl GraphFile {
    pub fn new(name: &str) -> Self {
        Self {
            metadata: Metadata {
                name: name.to_owned(),
                schema_version: default_graph_schema_version(),
                version: "1.0".to_owned(),
                description: format!("Knowledge graph: {name}"),
                node_count: 0,
                edge_count: 0,
            },
            nodes: Vec::new(),
            edges: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read graph: {}", path.display()))?;
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("json");
        let mut graph = if ext == "kg" {
            if raw.trim_start().starts_with('{') {
                serde_json::from_str(&raw).map_err(|error| {
                    anyhow::anyhow!(json_error_detail(
                        "invalid legacy JSON payload in .kg file",
                        path,
                        &raw,
                        &error,
                    ))
                })?
            } else {
                let graph_name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("graph");
                let decompressed = expand_kg_tokens(&raw);
                let (graph, warnings) =
                    parse_kg_with_warnings(&decompressed, graph_name, strict_kg_mode())
                        .with_context(|| {
                            format!("failed to parse .kg graph: {}", path.display())
                        })?;
                for warning in warnings {
                    let _ = crate::kg_sidecar::append_warning(
                        path,
                        &format!(
                            "ignored invalid graph entry in {}: {warning}",
                            path.display()
                        ),
                    );
                }
                graph
            }
        } else {
            serde_json::from_str(&raw).map_err(|error| {
                anyhow::anyhow!(json_error_detail("invalid JSON", path, &raw, &error))
            })?
        };
        let schema_version_before = graph_schema_version(&graph);
        normalize_graph_ids(&mut graph);
        let created_graph_info = ensure_graph_info_node(&mut graph);
        graph.metadata.schema_version = GRAPH_SCHEMA_VERSION;
        graph.refresh_counts();
        if created_graph_info || schema_version_before < GRAPH_SCHEMA_VERSION {
            graph.save(path)?;
        }
        Ok(graph)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut graph = self.clone();
        ensure_graph_info_node(&mut graph);
        graph.metadata.schema_version = GRAPH_SCHEMA_VERSION;
        graph.refresh_counts();
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("json");
        let raw = if ext == "kg" {
            let serialized = serialize_kg(&graph);
            let (compressed, stats) = compress_kg_text(&serialized, KG_TEXT_COMPRESSION_MIN_LEN);
            let saved_bytes = serialized.len().saturating_sub(compressed.len());
            let saved_percent = if serialized.is_empty() {
                0.0
            } else {
                (saved_bytes as f64 * 100.0) / serialized.len() as f64
            };
            if saved_bytes > 0 {
                eprintln!(
                    "kg compression: {:.1}% saved ({} -> {} bytes, {} dictionary entries)",
                    saved_percent,
                    stats.original_bytes,
                    stats.compressed_bytes.min(stats.original_bytes),
                    stats.dictionary_entries
                );
            }
            compressed
        } else {
            serde_json::to_string_pretty(&graph).context("failed to serialize graph")?
        };
        atomic_write(path, &raw)?;
        backup_graph_if_stale(path, &raw)
    }

    pub fn refresh_counts(&mut self) {
        self.metadata.node_count = self.nodes.len();
        self.metadata.edge_count = self.edges.len();
    }

    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_by_id_sorted(&self, id: &str) -> Option<&Node> {
        self.nodes
            .binary_search_by(|node| node.id.as_str().cmp(id))
            .ok()
            .and_then(|idx| self.nodes.get(idx))
    }

    pub fn node_by_id_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn has_edge(&self, source_id: &str, relation: &str, target_id: &str) -> bool {
        self.edges.iter().any(|edge| {
            edge.source_id == source_id && edge.relation == relation && edge.target_id == target_id
        })
    }
}

fn normalize_graph_ids(graph: &mut GraphFile) {
    let mut remap: HashMap<String, String> = HashMap::new();
    for node in &mut graph.nodes {
        let normalized = crate::validate::canonicalize_node_id_for_type(&node.id, &node.r#type)
            .unwrap_or_else(|_| crate::validate::normalize_node_id(&node.id));
        if normalized != node.id {
            remap.insert(node.id.clone(), normalized.clone());
            node.id = normalized;
        }
    }

    let known_ids: std::collections::HashSet<&str> =
        graph.nodes.iter().map(|node| node.id.as_str()).collect();

    for edge in &mut graph.edges {
        edge.source_id = remap.get(&edge.source_id).cloned().unwrap_or_else(|| {
            if known_ids.contains(edge.source_id.as_str()) {
                edge.source_id.clone()
            } else {
                crate::validate::normalize_node_id(&edge.source_id)
            }
        });
        edge.target_id = remap.get(&edge.target_id).cloned().unwrap_or_else(|| {
            if known_ids.contains(edge.target_id.as_str()) {
                edge.target_id.clone()
            } else {
                crate::validate::normalize_node_id(&edge.target_id)
            }
        });
        if edge.properties.bidirectional {
            let (source_id, target_id) =
                canonicalize_bidirectional_pair(&edge.source_id, &edge.target_id);
            edge.source_id = source_id;
            edge.target_id = target_id;
        }
    }

    for note in &mut graph.notes {
        note.node_id = remap.get(&note.node_id).cloned().unwrap_or_else(|| {
            if known_ids.contains(note.node_id.as_str()) {
                note.node_id.clone()
            } else {
                crate::validate::normalize_node_id(&note.node_id)
            }
        });
    }
}

fn ensure_graph_info_node(graph: &mut GraphFile) -> bool {
    if let Some(node) = graph.node_by_id_mut(GRAPH_INFO_NODE_ID) {
        let mut changed = false;
        if node.r#type != GRAPH_INFO_NODE_TYPE {
            node.r#type = GRAPH_INFO_NODE_TYPE.to_owned();
            changed = true;
        }
        if node.name.is_empty() {
            node.name = "Graph Metadata".to_owned();
            changed = true;
        }
        if node.properties.description.is_empty() {
            node.properties.description =
                "Internal graph metadata for cross-graph linking".to_owned();
            changed = true;
        }
        if !node
            .properties
            .key_facts
            .iter()
            .any(|fact| fact.starts_with(GRAPH_UUID_FACT_PREFIX))
        {
            node.properties
                .key_facts
                .push(format!("{GRAPH_UUID_FACT_PREFIX}{}", generate_graph_uuid()));
            changed = true;
        }
        let schema_fact = format!("{GRAPH_SCHEMA_VERSION_FACT_PREFIX}{GRAPH_SCHEMA_VERSION}");
        let had_schema_fact = node
            .properties
            .key_facts
            .iter()
            .any(|fact| fact.starts_with(GRAPH_SCHEMA_VERSION_FACT_PREFIX));
        if !had_schema_fact {
            node.properties.key_facts.push(schema_fact);
            changed = true;
        } else {
            let mut replaced = false;
            for fact in &mut node.properties.key_facts {
                if fact.starts_with(GRAPH_SCHEMA_VERSION_FACT_PREFIX) {
                    if *fact != schema_fact {
                        *fact = schema_fact.clone();
                        replaced = true;
                    }
                }
            }
            if replaced {
                changed = true;
            }
        }
        return changed;
    }

    graph.nodes.push(Node {
        id: GRAPH_INFO_NODE_ID.to_owned(),
        r#type: GRAPH_INFO_NODE_TYPE.to_owned(),
        name: "Graph Metadata".to_owned(),
        properties: NodeProperties {
            description: "Internal graph metadata for cross-graph linking".to_owned(),
            domain_area: "internal_metadata".to_owned(),
            provenance: "A".to_owned(),
            importance: 1.0,
            key_facts: vec![
                format!("{GRAPH_UUID_FACT_PREFIX}{}", generate_graph_uuid()),
                format!("{GRAPH_SCHEMA_VERSION_FACT_PREFIX}{GRAPH_SCHEMA_VERSION}"),
            ],
            ..NodeProperties::default()
        },
        source_files: vec!["DOC .kg/internal/graph_info".to_owned()],
    });
    true
}

fn graph_schema_version(graph: &GraphFile) -> u32 {
    graph
        .node_by_id(GRAPH_INFO_NODE_ID)
        .and_then(|node| {
            node.properties.key_facts.iter().find_map(|fact| {
                fact.strip_prefix(GRAPH_SCHEMA_VERSION_FACT_PREFIX)
                    .and_then(|value| value.parse::<u32>().ok())
            })
        })
        .unwrap_or(graph.metadata.schema_version)
}

fn display_node_id(id: &str, node_type: &str) -> String {
    let Some((head, suffix)) = id.split_once(':') else {
        return id.to_owned();
    };
    if head == node_type
        || crate::validate::canonical_type_code_for(node_type).is_some_and(|code| code == head)
        || crate::validate::TYPE_TO_PREFIX
            .iter()
            .any(|(typ, prefix)| *typ == node_type && *prefix == head)
    {
        return suffix.to_owned();
    }
    id.to_owned()
}

fn canonical_node_id_for_storage(id: &str) -> String {
    let Some((head, suffix)) = id.split_once(':') else {
        return id.to_owned();
    };
    let Some(node_type) = crate::validate::TYPE_TO_PREFIX
        .iter()
        .find(|(typ, prefix)| {
            crate::validate::canonical_type_code_for(typ).is_some_and(|code| code == head)
                || *prefix == head
        })
        .map(|(typ, _)| *typ)
    else {
        return id.to_owned();
    };
    crate::validate::canonical_type_code_for(node_type)
        .map(|code| format!("{code}:{suffix}"))
        .unwrap_or_else(|| id.to_owned())
}

fn generate_graph_uuid() -> String {
    let mut bytes = [0u8; 10];
    if fs::File::open("/dev/urandom")
        .and_then(|mut file| {
            use std::io::Read;
            file.read_exact(&mut bytes)
        })
        .is_err()
    {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id() as u128;
        let mixed = nanos ^ (pid << 64) ^ (nanos.rotate_left(17));
        bytes.copy_from_slice(&mixed.to_be_bytes()[6..16]);
    }
    let mut out = String::with_capacity(20);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        GRAPH_INFO_NODE_ID, GRAPH_INFO_NODE_TYPE, GRAPH_SCHEMA_VERSION, GRAPH_UUID_FACT_PREFIX,
        GraphFile, KG_TEXT_COMPRESSION_MIN_LEN, compress_kg_text, expand_kg_tokens, parse_kg,
    };

    #[test]
    fn save_and_load_kg_roundtrip_keeps_core_fields() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("graph.kg");

        let mut graph = GraphFile::new("graph");
        graph.nodes.push(crate::Node {
            id: "concept:refrigerator".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Lodowka".to_owned(),
            properties: crate::NodeProperties {
                description: "Urzadzenie chlodzace".to_owned(),
                provenance: "U".to_owned(),
                created_at: "2026-04-04T12:00:00Z".to_owned(),
                importance: 5.0,
                key_facts: vec!["A".to_owned(), "b".to_owned()],
                alias: vec!["Fridge".to_owned()],
                scan: Some(true),
                scan_ignore_unknown: Some(true),
                ..Default::default()
            },
            source_files: vec!["docs/fridge.md".to_owned()],
        });
        graph.edges.push(crate::Edge {
            source_id: "concept:refrigerator".to_owned(),
            relation: "READS_FROM".to_owned(),
            target_id: "datastore:settings".to_owned(),
            properties: crate::EdgeProperties {
                detail: "runtime read".to_owned(),
                valid_from: "2026-04-04T12:00:00Z".to_owned(),
                valid_to: "2026-04-05T12:00:00Z".to_owned(),
                ..Default::default()
            },
        });

        graph.save(&path).expect("save kg");
        let raw = std::fs::read_to_string(&path).expect("read kg");
        assert!(raw.contains("@ K:refrigerator"));
        assert!(raw.contains("> R D:settings"));

        let loaded = GraphFile::load(&path).expect("load kg");
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.edges.len(), 1);
        let node = loaded
            .node_by_id("concept:refrigerator")
            .expect("domain node");
        assert_eq!(node.properties.importance, 5.0);
        assert_eq!(node.properties.provenance, "U");
        assert_eq!(node.properties.scan, Some(true));
        assert_eq!(node.properties.scan_ignore_unknown, Some(true));
        assert_eq!(node.name, "Lodowka");
        assert_eq!(loaded.edges[0].relation, "READS_FROM");
        assert_eq!(loaded.edges[0].properties.detail, "runtime read");
        assert_eq!(
            loaded.edges[0].properties.valid_from,
            "2026-04-04T12:00:00Z"
        );
        assert_eq!(loaded.edges[0].properties.valid_to, "2026-04-05T12:00:00Z");
        assert_eq!(loaded.metadata.schema_version, GRAPH_SCHEMA_VERSION);
    }

    #[test]
    fn load_supports_legacy_json_payload_with_kg_extension() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("legacy.kg");
        std::fs::write(
            &path,
            r#"{
  "metadata": {"name": "legacy", "version": "1.0", "description": "x", "node_count": 0, "edge_count": 0},
  "nodes": [],
  "edges": [],
  "notes": []
}"#,
        )
        .expect("write legacy payload");

        let loaded = GraphFile::load(&path).expect("load legacy kg");
        assert_eq!(loaded.metadata.name, "legacy");
        assert_eq!(loaded.metadata.schema_version, GRAPH_SCHEMA_VERSION);
        assert_eq!(loaded.nodes.len(), 1);
        assert!(loaded.node_by_id(GRAPH_INFO_NODE_ID).is_some());
    }

    #[test]
    fn load_kg_auto_migrates_legacy_id_prefixes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("legacy-ids.kg");
        std::fs::write(
            &path,
            "@ K:concept:x\nN X\nD Desc\nV 0.5\nP U\nS docs/a.md\n> R datastore:y\n",
        )
        .expect("write kg");

        let loaded = GraphFile::load(&path).expect("load kg");
        assert_eq!(loaded.metadata.schema_version, GRAPH_SCHEMA_VERSION);
        assert!(loaded.node_by_id("concept:x").is_some());

        let persisted = std::fs::read_to_string(&path).expect("read migrated kg");
        assert!(persisted.contains("@ K:x"));
        assert!(persisted.contains("> R D:y"));
        assert!(persisted.contains(&format!("schema_version={GRAPH_SCHEMA_VERSION}")));
    }

    #[test]
    fn load_kg_ignores_invalid_timestamp_format() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("invalid-ts.kg");
        std::fs::write(
            &path,
            "@ K:concept:x\nN X\nD Desc\nE 2026-04-04 12:00:00\nV 4\nP U\n",
        )
        .expect("write kg");

        let loaded = GraphFile::load(&path).expect("invalid timestamp should be ignored");
        assert_eq!(loaded.nodes.len(), 2);
        assert!(
            loaded
                .node_by_id("concept:x")
                .expect("concept node")
                .properties
                .created_at
                .is_empty()
        );
    }

    #[test]
    fn load_kg_ignores_invalid_edge_timestamp_format() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("invalid-edge-ts.kg");
        std::fs::write(
            &path,
            "@ K:concept:x\nN X\nD Desc\nE 2026-04-04T12:00:00Z\nV 4\nP U\nS docs/a.md\n> H concept:y\ni 2026-04-04 12:00:00\n",
        )
        .expect("write kg");

        let loaded = GraphFile::load(&path).expect("invalid edge timestamp should be ignored");
        assert_eq!(loaded.edges.len(), 1);
        assert!(loaded.edges[0].properties.valid_from.is_empty());
    }

    #[test]
    fn load_kg_preserves_whitespace_and_dedupes_exact_duplicates() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("normalize.kg");
        std::fs::write(
            &path,
            "@ K:concept:x\nN  Name   With   Spaces \nD  Desc   with   spaces \nA Alias\nA Alias\nF fact one\nF FACT   one\nS docs/a.md\nS docs/a.md\nE 2026-04-04T12:00:00Z\nV 4\nP U\n",
        )
        .expect("write kg");

        let loaded = GraphFile::load(&path).expect("load kg");
        let node = loaded.node_by_id("concept:x").expect("concept node");
        assert_eq!(node.name, " Name   With   Spaces ");
        assert_eq!(node.properties.description, " Desc   with   spaces ");
        assert_eq!(node.properties.alias.len(), 1);
        assert_eq!(node.properties.key_facts.len(), 2);
        assert_eq!(node.source_files.len(), 1);
    }

    #[test]
    fn save_and_load_kg_roundtrip_keeps_notes_without_json_fallback() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("graph-notes.kg");

        let mut graph = GraphFile::new("graph-notes");
        graph.nodes.push(crate::Node {
            id: "concept:refrigerator".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Lodowka".to_owned(),
            properties: crate::NodeProperties {
                description: "Urzadzenie chlodzace".to_owned(),
                provenance: "U".to_owned(),
                created_at: "2026-04-04T12:00:00Z".to_owned(),
                ..Default::default()
            },
            source_files: vec!["docs/fridge.md".to_owned()],
        });
        graph.notes.push(crate::Note {
            id: "note:1".to_owned(),
            node_id: "concept:refrigerator".to_owned(),
            body: "Important maintenance insight".to_owned(),
            tags: vec!["Maintenance".to_owned(), "maintenance".to_owned()],
            author: "alice".to_owned(),
            created_at: "1712345678".to_owned(),
            provenance: "U".to_owned(),
            source_files: vec!["docs/a.md".to_owned(), "docs/a.md".to_owned()],
        });

        graph.save(&path).expect("save kg");
        let raw = std::fs::read_to_string(&path).expect("read kg");
        assert!(raw.contains("! note:1 K:refrigerator"));
        assert!(!raw.trim_start().starts_with('{'));

        let loaded = GraphFile::load(&path).expect("load kg");
        assert_eq!(loaded.notes.len(), 1);
        let note = &loaded.notes[0];
        assert_eq!(note.id, "note:1");
        assert_eq!(note.node_id, "concept:refrigerator");
        assert_eq!(note.body, "Important maintenance insight");
        assert_eq!(note.tags.len(), 1);
        assert_eq!(note.source_files.len(), 1);
    }

    #[test]
    fn save_and_load_kg_roundtrip_preserves_multiline_text_fields() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("graph-multiline.kg");

        let mut graph = GraphFile::new("graph-multiline");
        graph.nodes.push(crate::Node {
            id: "concept:refrigerator".to_owned(),
            r#type: "Concept".to_owned(),
            name: "Lodowka\nSmart".to_owned(),
            properties: crate::NodeProperties {
                description: "Linia 1\nLinia 2\\nliteral".to_owned(),
                provenance: "user\nimport".to_owned(),
                created_at: "2026-04-04T12:00:00Z".to_owned(),
                importance: 5.0,
                key_facts: vec!["Fakt 1\nFakt 2".to_owned()],
                alias: vec!["Alias\nA".to_owned()],
                domain_area: "ops\nfield".to_owned(),
                ..Default::default()
            },
            source_files: vec!["docs/fridge\nnotes.md".to_owned()],
        });
        graph.edges.push(crate::Edge {
            source_id: "concept:refrigerator".to_owned(),
            relation: "READS_FROM".to_owned(),
            target_id: "datastore:settings".to_owned(),
            properties: crate::EdgeProperties {
                detail: "runtime\nread".to_owned(),
                valid_from: "2026-04-04T12:00:00Z".to_owned(),
                valid_to: "2026-04-05T12:00:00Z".to_owned(),
                ..Default::default()
            },
        });
        graph.notes.push(crate::Note {
            id: "note:1".to_owned(),
            node_id: "concept:refrigerator".to_owned(),
            body: "line1\nline2\\nkeep".to_owned(),
            tags: vec!["multi\nline".to_owned()],
            author: "alice\nbob".to_owned(),
            created_at: "1712345678".to_owned(),
            provenance: "manual\nentry".to_owned(),
            source_files: vec!["docs/a\nb.md".to_owned()],
        });

        graph.save(&path).expect("save kg");
        let raw = std::fs::read_to_string(&path).expect("read kg");
        assert!(raw.contains("@ K:refrigerator"));
        assert!(raw.contains("> R D:settings"));
        assert!(raw.contains("! note:1 K:refrigerator"));
        assert!(raw.contains("N Lodowka\\nSmart"));
        assert!(raw.contains("D Linia 1\\nLinia 2\\\\nliteral"));
        assert!(raw.contains("- domain_area ops\\nfield"));
        assert!(raw.contains("d runtime\\nread"));
        assert!(raw.contains("b line1\\nline2\\\\nkeep"));

        let loaded = GraphFile::load(&path).expect("load kg");
        let node = loaded
            .node_by_id("concept:refrigerator")
            .expect("domain node");
        assert_eq!(node.name, "Lodowka\nSmart");
        assert_eq!(node.properties.description, "Linia 1\nLinia 2\\nliteral");
        assert_eq!(node.properties.provenance, "user\nimport");
        assert_eq!(node.properties.alias, vec!["Alias\nA".to_owned()]);
        assert_eq!(node.properties.key_facts, vec!["Fakt 1\nFakt 2".to_owned()]);
        assert_eq!(node.properties.domain_area, "ops\nfield");
        assert_eq!(node.source_files, vec!["docs/fridge\nnotes.md".to_owned()]);
        assert_eq!(loaded.edges[0].properties.detail, "runtime\nread");
        let note = &loaded.notes[0];
        assert_eq!(note.body, "line1\nline2\\nkeep");
        assert_eq!(note.tags, vec!["multi\nline".to_owned()]);
        assert_eq!(note.author, "alice\nbob");
        assert_eq!(note.provenance, "manual\nentry");
        assert_eq!(note.source_files, vec!["docs/a\nb.md".to_owned()]);
    }

    #[test]
    fn compress_kg_text_only_touches_generated_node_blocks() {
        let raw = concat!(
            "@ GDIR:src\n",
            "N alpha beta gamma\n",
            "D alpha beta gamma and more\n",
            "\n",
            "@ K:concept:plain\n",
            "N alpha beta gamma\n",
            "D alpha beta gamma and more\n",
            "E 2026-04-04T12:00:00Z\n",
            "V 4\n",
            "P U\n",
            "S docs/plain.md\n",
            "\n",
        );

        let (compressed, stats) = compress_kg_text(raw, KG_TEXT_COMPRESSION_MIN_LEN);
        assert!(stats.dictionary_entries > 0);
        assert!(compressed.contains("`1 "));
        assert!(compressed.contains("N`1`"));
        assert!(compressed.contains("D`1` and more"));

        let manual_block = compressed
            .split("@ K:concept:plain")
            .nth(1)
            .expect("manual block");
        assert!(!manual_block.contains("`1`"));

        let decompressed = expand_kg_tokens(&compressed);
        assert_eq!(decompressed, raw);
    }

    #[test]
    fn load_kg_expands_backtick_tokens_before_parsing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("compressed.kg");
        std::fs::write(
            &path,
            concat!(
                "`1 alpha beta gamma\n",
                "@ GDIR:src\n",
                "N `1`\n",
                "D `1` and more\n",
                "\n",
            ),
        )
        .expect("write kg");

        let loaded = GraphFile::load(&path).expect("load kg");
        let node = loaded.node_by_id("GDIR:src").expect("generated node");
        assert_eq!(node.name, "alpha beta gamma");
        assert_eq!(node.properties.description, "alpha beta gamma and more");
    }

    #[test]
    fn parse_bidirectional_similarity_edge_is_canonical_and_scored() {
        let raw = "@ ~:dedupe_b\nN B\nD Desc\nV 0.5\nP U\nS docs/b.md\n= ~ ~:dedupe_a\nd C1 0.11\nd C2 0.83\nd 0.91\n\n@ ~:dedupe_a\nN A\nD Desc\nV 0.5\nP U\nS docs/a.md\n";
        let graph = parse_kg(raw, "virt", true).expect("parse kg");

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.relation, "~");
        assert_eq!(edge.source_id, "~:dedupe_a");
        assert_eq!(edge.target_id, "~:dedupe_b");
        assert_eq!(edge.properties.detail, "0.91");
        assert!(edge.properties.bidirectional);
        assert_eq!(edge.properties.score_components.get("C1"), Some(&0.11));
        assert_eq!(edge.properties.score_components.get("C2"), Some(&0.83));
    }

    #[test]
    fn serialize_bidirectional_similarity_edge_uses_equals_operator() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("virt.kg");
        let mut graph = GraphFile::new("virt");
        graph.nodes.push(crate::Node {
            id: "~:dedupe_a".to_owned(),
            r#type: "~".to_owned(),
            name: "A".to_owned(),
            properties: crate::NodeProperties {
                description: "Desc".to_owned(),
                provenance: "U".to_owned(),
                created_at: "2026-04-10T00:00:00Z".to_owned(),
                importance: 0.6,
                ..Default::default()
            },
            source_files: vec!["docs/a.md".to_owned()],
        });
        graph.nodes.push(crate::Node {
            id: "~:dedupe_b".to_owned(),
            r#type: "~".to_owned(),
            name: "B".to_owned(),
            properties: crate::NodeProperties {
                description: "Desc".to_owned(),
                provenance: "U".to_owned(),
                created_at: "2026-04-10T00:00:00Z".to_owned(),
                importance: 0.6,
                ..Default::default()
            },
            source_files: vec!["docs/b.md".to_owned()],
        });
        graph.edges.push(crate::Edge {
            source_id: "~:dedupe_a".to_owned(),
            relation: "~".to_owned(),
            target_id: "~:dedupe_b".to_owned(),
            properties: crate::EdgeProperties {
                detail: "0.75".to_owned(),
                bidirectional: true,
                score_components: std::collections::BTreeMap::from([
                    ("C1".to_owned(), 0.2),
                    ("C2".to_owned(), 0.8),
                ]),
                ..Default::default()
            },
        });

        graph.save(&path).expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(raw.contains("= ~ ~:dedupe_b"));
        assert!(raw.contains("d C1 0.200000"));
        assert!(raw.contains("d C2 0.800000"));
        assert!(!raw.contains("> ~ ~:dedupe_b"));

        let loaded = GraphFile::load(&path).expect("load");
        assert_eq!(loaded.edges.len(), 1);
        assert!(loaded.edges[0].properties.bidirectional);
        assert_eq!(loaded.edges[0].properties.detail, "0.75");
        assert_eq!(
            loaded.edges[0].properties.score_components.get("C1"),
            Some(&0.2)
        );
        assert_eq!(
            loaded.edges[0].properties.score_components.get("C2"),
            Some(&0.8)
        );
    }

    #[test]
    fn strict_mode_rejects_bidirectional_relation_other_than_similarity() {
        let raw = "@ K:concept:a\nN A\nD Desc\nV 0.5\nP U\nS docs/a.md\n= HAS concept:b\n";
        let err = parse_kg(raw, "x", true).expect_err("strict mode should reject invalid '='");
        assert!(format!("{err:#}").contains("expected '~'"));
    }

    #[test]
    fn strict_mode_rejects_out_of_order_node_fields() {
        let raw = "@ K:concept:x\nD Desc\nN Name\nE 2026-04-04T12:00:00Z\nV 4\nP U\nS docs/a.md\n";
        let err = parse_kg(raw, "x", true).expect_err("strict mode should fail on field order");
        assert!(format!("{err:#}").contains("invalid field order"));
    }

    #[test]
    fn strict_mode_rejects_overlong_name_but_compat_mode_allows_it() {
        let long_name = "N ".to_owned() + &"X".repeat(121);
        let raw = format!(
            "@ K:concept:x\n{}\nD Desc\nE 2026-04-04T12:00:00Z\nV 4\nP U\nS docs/a.md\n",
            long_name
        );

        let strict_err = parse_kg(&raw, "x", true).expect_err("strict mode should fail on length");
        assert!(format!("{strict_err:#}").contains("invalid N length"));

        parse_kg(&raw, "x", false).expect("compat mode keeps permissive behavior");
    }

    #[test]
    fn save_kg_skips_empty_e_and_p_fields() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("no-empty-ep.kg");

        let mut graph = GraphFile::new("graph");
        graph.nodes.push(crate::Node {
            id: "concept:x".to_owned(),
            r#type: "Concept".to_owned(),
            name: "X".to_owned(),
            properties: crate::NodeProperties {
                description: "Desc".to_owned(),
                provenance: String::new(),
                created_at: String::new(),
                ..Default::default()
            },
            source_files: vec!["docs/a.md".to_owned()],
        });

        graph.save(&path).expect("save kg");
        let raw = std::fs::read_to_string(&path).expect("read kg");
        assert!(!raw.contains("\nE \n"));
        assert!(!raw.contains("\nP \n"));
    }

    #[test]
    fn load_generates_graph_info_node_when_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("meta.kg");
        let raw = "@ K:concept:x\nN X\nD Desc\nV 0.5\nP U\nS docs/a.md\n";
        std::fs::write(&path, raw).expect("write kg");

        let loaded = GraphFile::load(&path).expect("load kg");
        let info = loaded
            .node_by_id(GRAPH_INFO_NODE_ID)
            .expect("graph info node should be generated");
        assert_eq!(info.r#type, GRAPH_INFO_NODE_TYPE);
        assert!(
            info.properties
                .key_facts
                .iter()
                .any(|fact| fact.starts_with(GRAPH_UUID_FACT_PREFIX))
        );

        let persisted = std::fs::read_to_string(&path).expect("read persisted kg");
        assert!(persisted.contains("graph_info"));
        assert!(persisted.contains("graph_uuid="));
        assert!(persisted.contains("schema_version="));
    }
}
