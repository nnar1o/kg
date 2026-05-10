use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScannedSymbol {
    pub kind: String,
    pub name: String,
}

#[derive(Clone)]
struct LanguageSpec {
    language: tree_sitter::Language,
    tags_query: &'static str,
}

pub fn extract_code_symbols(path: &Path, source: &str) -> Result<Vec<ScannedSymbol>> {
    let Some(spec) = language_spec(path) else {
        return Ok(Vec::new());
    };

    let mut parser = Parser::new();
    parser
        .set_language(&spec.language)
        .with_context(|| format!("failed to set parser language for {}", path.display()))?;

    let tree = parser
        .parse(source, None)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    let query = Query::new(&spec.language, spec.tags_query)
        .with_context(|| format!("failed to build symbol query for {}", path.display()))?;
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    while let Some(matched) = matches.next() {
        for capture in matched.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let Some(raw_kind) = capture_name
                .split_once("definition.")
                .map(|(_, rest)| rest)
                .or_else(|| capture_name.strip_prefix("definition."))
            else {
                continue;
            };
            let Some(kind) = normalize_symbol_kind(raw_kind) else {
                continue;
            };
            let name_node = capture
                .node
                .child_by_field_name("name")
                .unwrap_or(capture.node);
            let Ok(raw_name) = name_node.utf8_text(source.as_bytes()) else {
                continue;
            };
            let Some(name) = normalize_symbol_name(raw_name) else {
                continue;
            };
            let key = (kind.to_owned(), name.to_owned());
            if seen.insert(key) {
                out.push(ScannedSymbol {
                    kind: kind.to_owned(),
                    name,
                });
            }
        }
    }

    Ok(out)
}

fn language_spec(path: &Path) -> Option<LanguageSpec> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Some(LanguageSpec {
            language: tree_sitter_rust::LANGUAGE.into(),
            tags_query: tree_sitter_rust::TAGS_QUERY,
        }),
        "java" => Some(LanguageSpec {
            language: tree_sitter_java::LANGUAGE.into(),
            tags_query: tree_sitter_java::TAGS_QUERY,
        }),
        "py" => Some(LanguageSpec {
            language: tree_sitter_python::LANGUAGE.into(),
            tags_query: tree_sitter_python::TAGS_QUERY,
        }),
        "js" | "jsx" | "mjs" | "cjs" => Some(LanguageSpec {
            language: tree_sitter_javascript::LANGUAGE.into(),
            tags_query: tree_sitter_javascript::TAGS_QUERY,
        }),
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some(LanguageSpec {
            language: tree_sitter_cpp::LANGUAGE.into(),
            tags_query: tree_sitter_cpp::TAGS_QUERY,
        }),
        _ => None,
    }
}

fn normalize_symbol_kind(raw_kind: &str) -> Option<&'static str> {
    let kind = raw_kind.split('.').next().unwrap_or(raw_kind);
    match kind {
        "function" => Some("fn"),
        "method" => Some("method"),
        "class" => Some("class"),
        "interface" => Some("interface"),
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "record" => Some("record"),
        "namespace" => Some("namespace"),
        "module" => Some("mod"),
        "mod" => Some("mod"),
        "trait" => Some("trait"),
        "type" | "type_alias" => Some("type"),
        "constant" | "const" => Some("const"),
        "static" => Some("static"),
        "variable" | "var" => Some("var"),
        "field" => Some("field"),
        "property" => Some("property"),
        "enum_member" => Some("enum_member"),
        "macro" => Some("macro"),
        _ => None,
    }
}

fn normalize_symbol_name(raw_name: &str) -> Option<String> {
    let trimmed = raw_name
        .trim()
        .trim_end_matches(|ch: char| matches!(ch, '{' | '}' | '(' | ')' | ';' | ',' | ':'))
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = trimmed.split_whitespace().last().unwrap_or(trimmed);
    let candidate = candidate.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '$');
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::extract_code_symbols;
    use std::path::Path;

    #[test]
    fn extracts_java_symbols() {
        let source = r#"
public class Greeter {
    public void hello() {}
}
"#;
        let symbols = extract_code_symbols(Path::new("Greeter.java"), source).unwrap();
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Greeter"));
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "hello"));
    }

    #[test]
    fn extracts_javascript_symbols() {
        let source = r#"
export class Greeter {}
export function hello() {}
"#;
        let symbols = extract_code_symbols(Path::new("greeter.js"), source).unwrap();
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Greeter"));
        assert!(symbols.iter().any(|s| s.kind == "fn" && s.name == "hello"));
    }

    #[test]
    fn extracts_python_symbols() {
        let source = r#"
class Greeter:
    def hello(self):
        pass
"#;
        let symbols = extract_code_symbols(Path::new("greeter.py"), source).unwrap();
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Greeter"));
        assert!(symbols.iter().any(|s| s.kind == "fn" && s.name == "hello"));
    }

    #[test]
    fn extracts_cpp_symbols() {
        let source = r#"
class Greeter {
public:
    void hello();
};

void hello() {}
"#;
        let symbols = extract_code_symbols(Path::new("greeter.cpp"), source).unwrap();
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Greeter"));
        assert!(symbols.iter().any(|s| s.kind == "fn" && s.name == "hello"));
    }
}
