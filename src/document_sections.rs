use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSection {
    pub path: Vec<String>,
    pub id_path: Vec<String>,
    pub legacy_id_path: Vec<String>,
    pub title: String,
    pub level: usize,
    pub ordinal: usize,
    pub content: String,
}

pub fn is_document_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    matches!(
        name,
        "README" | "README.md" | "README.markdown" | "CHANGELOG.md" | "LICENSE" | "COPYING"
    ) || matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "txt" | "rst" | "adoc")
    )
}

pub fn parse_document_sections(source: &str) -> Vec<DocumentSection> {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut stack: Vec<OpenSection> = Vec::new();
    let mut sibling_counts: HashMap<(Vec<String>, String), usize> = HashMap::new();
    let mut in_fence = false;
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];

        if is_fence_delimiter(line) {
            if let Some(current) = stack.last_mut() {
                current.content.push(line.to_owned());
            }
            in_fence = !in_fence;
            index += 1;
            continue;
        }

        if !in_fence {
            if let Some((level, title, skip_next)) = detect_heading(&lines, index) {
                finalize_until(&mut stack, level, &mut out);
                let parent = stack.last();
                let parent_path = parent
                    .map(|section| section.path.clone())
                    .unwrap_or_default();
                let parent_id_path = parent
                    .map(|section| section.id_path.clone())
                    .unwrap_or_default();
                let ordinal = next_sibling_ordinal(&mut sibling_counts, &parent_path, &title);
                let mut path = parent_path;
                path.push(title.clone());
                let mut id_path = parent_id_path;
                id_path.push(section_id_segment(&title, ordinal));
                let mut legacy_id_path = parent
                    .map(|section| section.legacy_id_path.clone())
                    .unwrap_or_default();
                legacy_id_path.push(legacy_section_id_segment(&title, ordinal));
                stack.push(OpenSection {
                    level,
                    title,
                    path,
                    id_path,
                    legacy_id_path,
                    ordinal,
                    content: Vec::new(),
                });
                index += skip_next;
                continue;
            }
        }

        if let Some(current) = stack.last_mut() {
            current.content.push(line.to_owned());
        }
        index += 1;
    }

    finalize_until(&mut stack, 0, &mut out);
    out
}

fn detect_heading(lines: &[&str], index: usize) -> Option<(usize, String, usize)> {
    let line = lines[index].trim_end();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(level) = atx_heading_level(trimmed) {
        let title = trimmed[level + 1..]
            .trim()
            .trim_end_matches('#')
            .trim()
            .to_owned();
        if !title.is_empty() {
            return Some((level, title, 1));
        }
    }

    if let Some((level, title, skip_next)) = setext_heading(lines, index) {
        return Some((level, title, skip_next));
    }

    if let Some(level) = asciidoc_heading_level(trimmed) {
        let title = trimmed[level..].trim().to_owned();
        if !title.is_empty() {
            return Some((level, title, 1));
        }
    }

    None
}

fn atx_heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes)
        && line
            .chars()
            .nth(hashes)
            .is_some_and(|ch| ch.is_whitespace())
    {
        Some(hashes)
    } else {
        None
    }
}

fn asciidoc_heading_level(line: &str) -> Option<usize> {
    let equals = line.chars().take_while(|ch| *ch == '=').count();
    if (1..=6).contains(&equals)
        && line
            .chars()
            .nth(equals)
            .is_some_and(|ch| ch.is_whitespace())
    {
        Some(equals)
    } else {
        None
    }
}

fn setext_heading(lines: &[&str], index: usize) -> Option<(usize, String, usize)> {
    let title = lines.get(index)?.trim();
    if title.is_empty() {
        return None;
    }
    let underline = lines.get(index + 1)?.trim();
    if underline.len() < 3 || !underline.chars().all(|ch| ch == '=' || ch == '-') {
        return None;
    }
    let level = if underline.starts_with('=') { 1 } else { 2 };
    Some((level, title.to_owned(), 2))
}

fn is_fence_delimiter(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn finalize_until(stack: &mut Vec<OpenSection>, level: usize, out: &mut Vec<DocumentSection>) {
    while stack.last().is_some_and(|section| section.level >= level) {
        let current = stack.pop().expect("stack checked above");
        out.push(DocumentSection {
            path: current.path,
            id_path: current.id_path,
            legacy_id_path: current.legacy_id_path,
            title: current.title,
            level: current.level,
            ordinal: current.ordinal,
            content: current.content.join("\n").trim().to_owned(),
        });
    }
}

fn next_sibling_ordinal(
    counts: &mut HashMap<(Vec<String>, String), usize>,
    parent_path: &[String],
    title: &str,
) -> usize {
    let key = (parent_path.to_vec(), title.to_owned());
    let count = counts.entry(key).or_insert(0);
    *count += 1;
    *count
}

fn section_id_segment(title: &str, ordinal: usize) -> String {
    let mut escaped = escape_section_title(title);
    if ordinal > 1 {
        escaped.push('~');
        escaped.push_str(&ordinal.to_string());
        escaped
    } else {
        escaped
    }
}

fn legacy_section_id_segment(title: &str, ordinal: usize) -> String {
    let mut slug = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    slug = slug.trim_matches('_').to_owned();
    if slug.is_empty() {
        slug = "section".to_owned();
    }
    if ordinal > 1 {
        format!("{}~{}", slug, ordinal)
    } else {
        slug
    }
}

fn escape_section_title(title: &str) -> String {
    let mut out = String::new();
    for byte in title.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' => out.push(*byte as char),
            _ => {
                out.push('~');
                out.push_str(&format!("{:02X}", byte));
            }
        }
    }
    if out.is_empty() {
        out.push_str("section");
    }
    out
}

#[derive(Debug, Clone)]
struct OpenSection {
    level: usize,
    title: String,
    path: Vec<String>,
    id_path: Vec<String>,
    legacy_id_path: Vec<String>,
    ordinal: usize,
    content: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{is_document_source, parse_document_sections};
    use std::path::Path;

    #[test]
    fn detects_document_sources() {
        assert!(is_document_source(Path::new("README.md")));
        assert!(is_document_source(Path::new("guide.adoc")));
        assert!(!is_document_source(Path::new("main.rs")));
    }

    #[test]
    fn parses_nested_markdown_sections() {
        let source = r#"
# Intro
Hello world.

## Details
More text.
"#;
        let sections = parse_document_sections(source);
        assert_eq!(sections.len(), 2);
        assert!(sections.iter().any(|section| section.path == vec!["Intro"]));
        assert!(
            sections
                .iter()
                .any(|section| section.path == vec!["Intro", "Details"])
        );
        assert!(sections
            .iter()
            .any(|section| section.title == "Details" && section.content.contains("More text.")));
    }

    #[test]
    fn ignores_headings_inside_code_fences() {
        let source = r#"
# Intro
```md
## Not a chapter
```
Body.
"#;
        let sections = parse_document_sections(source);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Intro");
        assert!(sections[0].content.contains("## Not a chapter"));
    }

    #[test]
    fn keeps_duplicate_sibling_ordinals_stable() {
        let source = r#"
# Intro
Text

## Same
One

## Same
Two
"#;
        let sections = parse_document_sections(source);
        let ordinals: Vec<_> = sections
            .iter()
            .filter(|section| section.title == "Same")
            .map(|section| section.ordinal)
            .collect();
        assert_eq!(ordinals, vec![1, 2]);
    }

    #[test]
    fn uses_lossless_section_ids_for_similar_headings() {
        let source = r#"
# A+B
One

# A B
Two
"#;
        let sections = parse_document_sections(source);
        let ids: Vec<_> = sections
            .iter()
            .filter(|section| section.level == 1)
            .map(|section| section.id_path.last().cloned().expect("id segment"))
            .collect();

        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(ids.iter().any(|id| id.contains("~2B")));
        assert!(ids.iter().any(|id| id.contains("~20")));
    }
}
