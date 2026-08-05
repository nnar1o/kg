#![allow(clippy::too_many_arguments)]
// `SclError` intentionally carries structured diagnostics in public results,
// so its size is part of the deliberate error API design.
#![allow(clippy::result_large_err)]

//! Simple Command Language (SCL) — a thin translation layer over the kg CLI.
//!
//! SCL converts LLM-friendly commands like `find fridge` into canonical CLI
//! args like `graph <g> node find "fridge"` for dispatch via `run_args_safe`.
//!
//! Backward compatible: lines whose first token is not a known verb are
//! returned as `Ok(None)` so the caller can fall through to the existing
//! CLI path.

use std::ffi::OsString;

use crate::validate;

// ---------------------------------------------------------------------------
// Shared tokenizers (extracted from kg-mcp.rs)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteMode {
    None,
    Single,
    Double,
}

/// Split a script on `;` or newline, respecting single/double quotes and
/// backslash escapes.
pub fn split_script(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut mode = QuoteMode::None;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            buf.push(ch);
            escape = false;
            continue;
        }

        match mode {
            QuoteMode::None => match ch {
                '\\' => {
                    buf.push(ch);
                    escape = true;
                }
                '\'' => {
                    mode = QuoteMode::Single;
                    buf.push(ch);
                }
                '"' => {
                    mode = QuoteMode::Double;
                    buf.push(ch);
                }
                ';' | '\n' => {
                    parts.push(std::mem::take(&mut buf));
                }
                _ => buf.push(ch),
            },
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                }
                buf.push(ch);
            }
            QuoteMode::Double => match ch {
                '\\' => {
                    buf.push(ch);
                    escape = true;
                }
                '"' => {
                    mode = QuoteMode::None;
                    buf.push(ch);
                }
                _ => buf.push(ch),
            },
        }
    }

    parts.push(buf);
    parts
}

/// Tokenize a single command string, respecting quotes and backslash escapes.
/// Returns an error on unterminated quotes.
pub fn tokenize_command(cmd: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut mode = QuoteMode::None;
    let mut escape = false;

    for ch in cmd.chars() {
        if escape {
            buf.push(ch);
            escape = false;
            continue;
        }

        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !buf.is_empty() {
                        tokens.push(std::mem::take(&mut buf));
                    }
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else {
                    buf.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    buf.push(ch);
                }
            }
            QuoteMode::Double => {
                if ch == '"' {
                    mode = QuoteMode::None;
                } else if ch == '\\' {
                    escape = true;
                } else {
                    buf.push(ch);
                }
            }
        }
    }

    if escape {
        buf.push('\\');
    }

    if mode != QuoteMode::None {
        return Err("unterminated quote".to_owned());
    }

    if !buf.is_empty() {
        tokens.push(buf);
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Parser context that is threaded through `parse_script`.
pub struct Ctx {
    pub graph: String,
    pub strict: bool,
}

impl Ctx {
    pub fn new(graph: String, strict: bool) -> Self {
        Ctx { graph, strict }
    }
}

/// Error categories (used as `SclError.category`).
pub mod category {
    pub const UNKNOWN_VERB: &str = "unknown_verb";
    pub const BAD_ID_FORMAT: &str = "bad_id_format";
    pub const UNKNOWN_TYPE: &str = "unknown_type";
    pub const UNKNOWN_RELATION: &str = "unknown_relation";
    pub const EDGE_TYPE_MISMATCH: &str = "edge_type_mismatch";
    pub const MISSING_REQUIRED_FIELD: &str = "missing_required_field";
    pub const BAD_VALUE_RANGE: &str = "bad_value_range";
    pub const GRAPH_NOT_FOUND: &str = "graph_not_found";
    pub const NODE_NOT_FOUND: &str = "node_not_found";
    pub const AMBIGUOUS: &str = "ambiguous";
}

/// A structured error from SCL parsing / translation.
#[derive(Debug, Clone)]
pub struct SclError {
    pub category: String,
    pub message: String,
    pub input: String,
    pub expected_grammar: String,
    pub fix_example: String,
    pub canonical_equivalent: String,
}

impl std::fmt::Display for SclError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}\n  input: {}\n  expected: {}\n  fix: {}\n  canonical: {}",
            self.category,
            self.message,
            self.input,
            self.expected_grammar,
            self.fix_example,
            self.canonical_equivalent
        )
    }
}

impl std::error::Error for SclError {}

impl SclError {
    fn new(
        category: &'static str,
        message: String,
        input: String,
        expected_grammar: &str,
        fix_example: &str,
        canonical_equivalent: &str,
    ) -> Self {
        SclError {
            category: category.to_owned(),
            message,
            input,
            expected_grammar: expected_grammar.to_owned(),
            fix_example: fix_example.to_owned(),
            canonical_equivalent: canonical_equivalent.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// CanonicalLine typed enum
// ---------------------------------------------------------------------------

/// Represents a parsed SCL line as a typed command.
///
/// Each variant carries only the fields relevant to that command.
/// `to_args(ctx)` produces the canonical CLI argument vector for dispatch
/// via `run_args_safe`.
///
/// Some variants (`ListTypes`, `ListRelations`, `ListGraphs`, `Help`,
/// `Feedback`) are expected to be handled directly by the MCP server rather
/// than dispatched through the CLI. For those, `to_args()` returns an empty
/// vector — the caller should detect this and handle appropriately.
#[derive(Debug, Clone, PartialEq)]
pub enum CanonicalLine {
    NodeFind {
        query: String,
        limit: Option<usize>,
        mode: Option<String>,
        full: bool,
        output_size: Option<usize>,
    },
    NodeGet {
        id: String,
        full: bool,
        output_size: Option<usize>,
    },
    NodeAdd {
        id: String,
        node_type: String,
        name: Option<String>,
        desc: Option<String>,
        domain: Option<String>,
        importance: Option<f64>,
        confidence: Option<f64>,
        provenance: Option<String>,
        source: Option<String>,
        facts: Vec<String>,
        aliases: Vec<String>,
    },
    NodeModify {
        id: String,
        name: Option<String>,
        desc: Option<String>,
        importance: Option<f64>,
        confidence: Option<f64>,
        facts: Vec<String>,
        aliases: Vec<String>,
    },
    NodeRemove {
        id: String,
    },
    EdgeAdd {
        src: String,
        relation: String,
        tgt: String,
        detail: Option<String>,
    },
    EdgeRemove {
        src: String,
        relation: String,
        tgt: String,
    },
    ListTypes,
    ListRelations,
    ListGraphs,
    ListNodes,
    ListEdges,
    Stats {
        graph: Option<String>,
    },
    Help {
        topic: Option<String>,
    },
    Feedback {
        uid: String,
        verdict: String,
        pick: Option<u32>,
    },
}

impl CanonicalLine {
    /// Convert this canonical line into a CLI argument vector suitable for
    /// `run_args_safe`.
    ///
    /// For `ListTypes`, `ListRelations`, `ListGraphs`, `Help`, and `Feedback`,
    /// returns an empty vector because those are handled directly by the MCP
    /// server without CLI dispatch.
    pub fn to_args(&self, ctx: &Ctx) -> Vec<OsString> {
        match self {
            CanonicalLine::NodeFind {
                query,
                limit,
                mode,
                full,
                output_size,
            } => {
                let mut args = vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("node"),
                    OsString::from("find"),
                    OsString::from(query),
                ];
                if let Some(n) = limit {
                    args.push(OsString::from("--limit"));
                    args.push(OsString::from(n.to_string()));
                }
                if let Some(m) = mode {
                    args.push(OsString::from("--mode"));
                    args.push(OsString::from(m));
                }
                if *full {
                    args.push(OsString::from("--full"));
                }
                if let Some(s) = output_size {
                    args.push(OsString::from("--output-size"));
                    args.push(OsString::from(s.to_string()));
                }
                args
            }
            CanonicalLine::NodeGet {
                id,
                full,
                output_size,
            } => {
                let mut args = vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("node"),
                    OsString::from("get"),
                    OsString::from(id),
                ];
                if *full {
                    args.push(OsString::from("--full"));
                }
                if let Some(s) = output_size {
                    args.push(OsString::from("--output-size"));
                    args.push(OsString::from(s.to_string()));
                }
                args
            }
            CanonicalLine::NodeAdd {
                id,
                node_type,
                name,
                desc,
                domain,
                importance,
                confidence,
                provenance,
                source,
                facts,
                aliases,
            } => {
                let mut args = vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("node"),
                    OsString::from("add"),
                    OsString::from(id),
                    OsString::from("--type"),
                    OsString::from(node_type),
                ];
                if let Some(v) = name {
                    args.push(OsString::from("--name"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = desc {
                    args.push(OsString::from("--description"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = domain {
                    args.push(OsString::from("--domain-area"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = importance {
                    args.push(OsString::from("--importance"));
                    args.push(OsString::from(v.to_string()));
                }
                if let Some(v) = confidence {
                    args.push(OsString::from("--confidence"));
                    args.push(OsString::from(v.to_string()));
                }
                if let Some(v) = provenance {
                    args.push(OsString::from("--provenance"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = source {
                    args.push(OsString::from("--source"));
                    args.push(OsString::from(v));
                }
                for f in facts {
                    args.push(OsString::from("--fact"));
                    args.push(OsString::from(f));
                }
                for a in aliases {
                    args.push(OsString::from("--alias"));
                    args.push(OsString::from(a));
                }
                args
            }
            CanonicalLine::NodeModify {
                id,
                name,
                desc,
                importance,
                confidence,
                facts,
                aliases,
            } => {
                let mut args = vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("node"),
                    OsString::from("modify"),
                    OsString::from(id),
                ];
                if let Some(v) = name {
                    args.push(OsString::from("--name"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = desc {
                    args.push(OsString::from("--description"));
                    args.push(OsString::from(v));
                }
                if let Some(v) = importance {
                    args.push(OsString::from("--importance"));
                    args.push(OsString::from(v.to_string()));
                }
                if let Some(v) = confidence {
                    args.push(OsString::from("--confidence"));
                    args.push(OsString::from(v.to_string()));
                }
                for f in facts {
                    args.push(OsString::from("--fact"));
                    args.push(OsString::from(f));
                }
                for a in aliases {
                    args.push(OsString::from("--alias"));
                    args.push(OsString::from(a));
                }
                args
            }
            CanonicalLine::NodeRemove { id } => {
                vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("node"),
                    OsString::from("remove"),
                    OsString::from(id),
                ]
            }
            CanonicalLine::EdgeAdd {
                src,
                relation,
                tgt,
                detail,
            } => {
                let mut args = vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("edge"),
                    OsString::from("add"),
                    OsString::from(src),
                    OsString::from(relation),
                    OsString::from(tgt),
                ];
                if let Some(d) = detail {
                    args.push(OsString::from("--detail"));
                    args.push(OsString::from(d));
                }
                args
            }
            CanonicalLine::EdgeRemove { src, relation, tgt } => {
                vec![
                    OsString::from("graph"),
                    OsString::from(&ctx.graph),
                    OsString::from("edge"),
                    OsString::from("remove"),
                    OsString::from(src),
                    OsString::from(relation),
                    OsString::from(tgt),
                ]
            }
            CanonicalLine::Stats { graph } => {
                let g = graph.as_deref().unwrap_or(&ctx.graph);
                vec![
                    OsString::from("graph"),
                    OsString::from(g),
                    OsString::from("stats"),
                ]
            }
            // These are handled directly by the MCP server, not dispatched.
            CanonicalLine::ListTypes
            | CanonicalLine::ListRelations
            | CanonicalLine::ListGraphs
            | CanonicalLine::ListNodes
            | CanonicalLine::ListEdges
            | CanonicalLine::Help { .. }
            | CanonicalLine::Feedback { .. } => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// parse_script / parse_line
// ---------------------------------------------------------------------------

/// Parse a full SCL script (one or more lines separated by `;` or newlines).
///
/// - Empty/whitespace-only script → returns `vec![CanonicalLine::Help { topic: None }]`.
/// - Lines starting with `strict` toggle `ctx.strict = true`.
/// - Lines matching `use <graph>` set `ctx.graph`.
/// - Other lines are parsed via `parse_line`.
pub fn parse_script(script: &str, ctx: &mut Ctx) -> Result<Vec<CanonicalLine>, SclError> {
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return Ok(vec![CanonicalLine::Help { topic: None }]);
    }

    let raw_lines = split_script(script);
    let mut result = Vec::new();

    for raw in raw_lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // `strict` directive
        if line == "strict" {
            ctx.strict = true;
            continue;
        }

        // `use <graph>` directive — validation deferred to execution
        if let Some(graph_name) = line.strip_prefix("use ") {
            let g = graph_name.trim();
            if !g.is_empty() && !g.contains(char::is_whitespace) {
                ctx.graph = g.to_owned();
                continue;
            }
        }

        match parse_line(line, ctx)? {
            Some(canonical) => result.push(canonical),
            None => {
                // Unknown verb → passthrough. Return an error so the caller
                // can fall back to the existing CLI path.
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    format!("unknown verb in '{}'", line),
                    line.to_owned(),
                    "<verb> <args...>",
                    line,
                    line,
                ));
            }
        }
    }

    Ok(result)
}

/// Parse a single SCL line.
///
/// Returns `Ok(None)` when the first token is not a known verb
/// (signals passthrough to the existing CLI path).
pub fn parse_line(line: &str, ctx: &Ctx) -> Result<Option<CanonicalLine>, SclError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let tokens = tokenize_command(trimmed).map_err(|err| {
        SclError::new(
            category::UNKNOWN_VERB,
            format!("tokenization error: {err}"),
            trimmed.to_owned(),
            "<verb> <args...>",
            trimmed,
            trimmed,
        )
    })?;

    if tokens.is_empty() {
        return Ok(None);
    }

    let verb = tokens[0].to_ascii_lowercase();

    match verb.as_str() {
        "find" | "search" => parse_find(&tokens, trimmed).map(Some),
        "get" => parse_get(&tokens, trimmed),
        "add" | "create" => {
            // `add edge <src> <rel> <tgt>` is an alias for connect
            if tokens.len() > 1 && tokens[1] == "edge" {
                parse_connect(&tokens, trimmed).map(Some)
            } else {
                parse_add(&tokens, trimmed, ctx).map(Some)
            }
        }
        "modify" => parse_modify(&tokens, trimmed).map(Some),
        "remove" => parse_remove(&tokens, trimmed),
        "connect" | "link" => parse_connect(&tokens, trimmed).map(Some),
        "disconnect" => parse_disconnect(&tokens, trimmed).map(Some),
        "list" => parse_list(&tokens, trimmed),
        "stats" => parse_stats(&tokens, trimmed),
        "help" => Ok(Some(CanonicalLine::Help {
            topic: tokens.get(1).cloned(),
        })),
        "feedback" => parse_feedback(&tokens, trimmed),
        _ => {
            // Unknown verb — not an error, signals passthrough.
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Verb parsers
// ---------------------------------------------------------------------------

/// `find <query> [--limit N] [--mode X] [--full] [--output-size N]`
fn parse_find(tokens: &[String], input: &str) -> Result<CanonicalLine, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::UNKNOWN_VERB,
            "missing query".to_owned(),
            input.to_owned(),
            "find <query> [--limit N] [--mode hybrid|bm25|fuzzy] [--full] [--output-size N]",
            "find fridge",
            "",
        ));
    }

    let query = tokens[1].clone();
    let mut limit = None;
    let mut mode = None;
    let mut full = false;
    let mut output_size = None;

    let mut i = 2;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "--limit" => {
                i += 1;
                if i >= tokens.len() {
                    return Err(SclError::new(
                        category::BAD_VALUE_RANGE,
                        "missing value for --limit".to_owned(),
                        input.to_owned(),
                        "find <query> [--limit N]",
                        "find fridge --limit 5",
                        "",
                    ));
                }
                limit = Some(tokens[i].parse::<usize>().map_err(|_| {
                    SclError::new(
                        category::BAD_VALUE_RANGE,
                        format!("invalid --limit value '{}'", tokens[i]),
                        input.to_owned(),
                        "find <query> [--limit N]",
                        "find fridge --limit 5",
                        "",
                    )
                })?);
            }
            "--mode" => {
                i += 1;
                if i >= tokens.len() {
                    return Err(SclError::new(
                        category::BAD_VALUE_RANGE,
                        "missing value for --mode".to_owned(),
                        input.to_owned(),
                        "find <query> [--mode hybrid|bm25|fuzzy]",
                        "find fridge --mode bm25",
                        "",
                    ));
                }
                mode = Some(tokens[i].clone());
            }
            "--full" => {
                full = true;
            }
            "--output-size" => {
                i += 1;
                if i >= tokens.len() {
                    return Err(SclError::new(
                        category::BAD_VALUE_RANGE,
                        "missing value for --output-size".to_owned(),
                        input.to_owned(),
                        "find <query> [--output-size N]",
                        "find fridge --output-size 100",
                        "",
                    ));
                }
                output_size = Some(tokens[i].parse::<usize>().map_err(|_| {
                    SclError::new(
                        category::BAD_VALUE_RANGE,
                        format!("invalid --output-size value '{}'", tokens[i]),
                        input.to_owned(),
                        "find <query> [--output-size N]",
                        "find fridge --output-size 100",
                        "",
                    )
                })?);
            }
            other => {
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    format!("unknown flag '{other}'"),
                    input.to_owned(),
                    "find <query> [--limit N] [--mode X] [--full] [--output-size N]",
                    "find fridge",
                    "",
                ));
            }
        }
        i += 1;
    }

    Ok(CanonicalLine::NodeFind {
        query,
        limit,
        mode,
        full,
        output_size,
    })
}

/// `get <nodeid> [--full] [--output-size N]`
fn parse_get(tokens: &[String], input: &str) -> Result<Option<CanonicalLine>, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            "missing node id".to_owned(),
            input.to_owned(),
            "get <nodeid> [--full] [--output-size N]",
            "get concept:fridge",
            "",
        ));
    }

    let id = tokens[1].clone();

    // Validate id format.
    let id_has_colon = id.contains(':');
    if !id_has_colon {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            format!("node id '{}' must be in format <type>:snake_case", id),
            input.to_owned(),
            "get <nodeid>",
            "get concept:fridge",
            "",
        ));
    }

    // If we know the prefix, run full canonicalization validation
    if let Some((prefix, _)) = id.split_once(':') {
        if let Some(typ) = validate::type_for_prefix(prefix) {
            validate::canonicalize_node_id_for_type(&id, typ).map_err(|e| {
                SclError::new(
                    category::BAD_ID_FORMAT,
                    e,
                    input.to_owned(),
                    "get <nodeid>",
                    "get concept:fridge",
                    "",
                )
            })?;
        }
        // Unknown prefix: accept it but warn via id format — the prefix
        // might be a custom type. Just check suffix format.
        let suffix = id.split_once(':').map(|(_, s)| s).unwrap_or("");
        if suffix.is_empty() {
            return Err(SclError::new(
                category::BAD_ID_FORMAT,
                format!("node id '{}' has empty suffix", id),
                input.to_owned(),
                "get <nodeid>",
                "get concept:fridge",
                "",
            ));
        }
    }

    let mut full = false;
    let mut output_size = None;

    let mut i = 2;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "--full" => {
                full = true;
            }
            "--output-size" => {
                i += 1;
                if i >= tokens.len() {
                    return Err(SclError::new(
                        category::BAD_VALUE_RANGE,
                        "missing value for --output-size".to_owned(),
                        input.to_owned(),
                        "get <nodeid> [--output-size N]",
                        "get concept:fridge --output-size 100",
                        "",
                    ));
                }
                output_size = Some(tokens[i].parse::<usize>().map_err(|_| {
                    SclError::new(
                        category::BAD_VALUE_RANGE,
                        format!("invalid --output-size value '{}'", tokens[i]),
                        input.to_owned(),
                        "get <nodeid> [--output-size N]",
                        "get concept:fridge --output-size 100",
                        "",
                    )
                })?);
            }
            other => {
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    format!("unknown flag '{other}'"),
                    input.to_owned(),
                    "get <nodeid> [--full] [--output-size N]",
                    "get concept:fridge",
                    "",
                ));
            }
        }
        i += 1;
    }

    Ok(Some(CanonicalLine::NodeGet {
        id,
        full,
        output_size,
    }))
}

/// `add <nodeid> [as <type>] [--name ".."] [--desc ".."] [--domain ".."]`
///   `[--importance N] [--confidence N] [--provenance U|D|A]`
///   `[--source "TYPE ref"] [--fact ".."] [--alias ".."]`
fn parse_add(tokens: &[String], input: &str, ctx: &Ctx) -> Result<CanonicalLine, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            "missing node id".to_owned(),
            input.to_owned(),
            "add <nodeid> [as <type>] [--name \"..\"] [--desc \"..\"] [--domain \"..\"] [--importance 0.5] [--confidence 0.7] [--provenance A] [--source \"TYPE ref\"] [--fact \"..\"] [--alias \"..\"]",
            "add concept:fridge --name \"Fridge\"",
            "",
        ));
    }

    let id = tokens[1].clone();
    let mut node_type: Option<String> = None;
    let mut pos = 2;

    // Check for `as <type>`
    if pos < tokens.len() && tokens[pos] == "as" {
        pos += 1;
        if pos >= tokens.len() {
            return Err(SclError::new(
                category::UNKNOWN_TYPE,
                "missing type after 'as'".to_owned(),
                input.to_owned(),
                "add <nodeid> as <type>",
                "add concept:fridge as Concept",
                "",
            ));
        }
        let t = tokens[pos].to_owned();
        if !validate::is_valid_node_type(&t) {
            return Err(SclError::new(
                category::UNKNOWN_TYPE,
                format!("unknown node type '{t}'"),
                input.to_owned(),
                "add <nodeid> as <type>",
                "add concept:fridge as Concept",
                "",
            ));
        }
        node_type = Some(t);
        pos += 1;
    }

    // Infer type from id prefix if not explicitly given
    let node_type = node_type.unwrap_or_else(|| {
        id.split_once(':')
            .and_then(|(prefix, _)| validate::type_for_prefix(prefix))
            .map(|t| t.to_owned())
            .unwrap_or_else(|| {
                // Fallback: use prefix verbatim if it's a valid node type
                id.split_once(':')
                    .filter(|(prefix, _)| validate::is_valid_node_type(prefix))
                    .map(|(prefix, _)| prefix.to_owned())
                    .unwrap_or_else(|| {
                        // Last resort: just use "Concept" (will be validated downstream)
                        "Concept".to_owned()
                    })
            })
    });

    // Validate id format for the inferred/declared type
    validate::canonicalize_node_id_for_type(&id, &node_type).map_err(|e| {
        SclError::new(
            category::BAD_ID_FORMAT,
            e,
            input.to_owned(),
            "add <nodeid> [as <type>]",
            &format!("add {} as {}", id, node_type),
            "",
        )
    })?;

    // Parse flags
    let mut name: Option<String> = None;
    let mut desc: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut importance: Option<f64> = None;
    let mut confidence: Option<f64> = None;
    let mut provenance: Option<String> = None;
    let mut source: Option<String> = None;
    let mut facts: Vec<String> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();

    while pos < tokens.len() {
        let flag = &tokens[pos];
        match flag.as_str() {
            "--name" | "--desc" | "--description" | "--domain" | "--domain-area"
            | "--importance" | "--confidence" | "--provenance" | "--source" | "--fact"
            | "--alias" => {
                let (canonical_flag, _is_repeatable) = match flag.as_str() {
                    "--description" => ("--desc", false),
                    "--domain-area" => ("--domain", false),
                    _ => (flag.as_str(), matches!(flag.as_str(), "--fact" | "--alias")),
                };

                pos += 1;
                if pos >= tokens.len() {
                    return Err(SclError::new(
                        category::MISSING_REQUIRED_FIELD,
                        format!("missing value for {flag}"),
                        input.to_owned(),
                        "add <nodeid> [flags]",
                        "add concept:fridge --name \"Fridge\"",
                        "",
                    ));
                }
                let value = tokens[pos].clone();

                match canonical_flag {
                    "--name" => name = Some(value),
                    "--desc" => desc = Some(value),
                    "--domain" => domain = Some(value),
                    "--importance" => {
                        let v: f64 = value.parse().map_err(|_| {
                            SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("invalid importance '{value}', expected 0.0..1.0"),
                                input.to_owned(),
                                "add <nodeid> [--importance 0.0..1.0]",
                                "add concept:fridge --importance 0.5",
                                "",
                            )
                        })?;
                        if !(0.0..=1.0).contains(&v) {
                            return Err(SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("importance '{v}' out of range 0.0..1.0"),
                                input.to_owned(),
                                "add <nodeid> [--importance 0.0..1.0]",
                                "add concept:fridge --importance 0.5",
                                "",
                            ));
                        }
                        importance = Some(v);
                    }
                    "--confidence" => {
                        let v: f64 = value.parse().map_err(|_| {
                            SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("invalid confidence '{value}', expected 0.0..1.0"),
                                input.to_owned(),
                                "add <nodeid> [--confidence 0.0..1.0]",
                                "add concept:fridge --confidence 0.7",
                                "",
                            )
                        })?;
                        if !(0.0..=1.0).contains(&v) {
                            return Err(SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("confidence '{v}' out of range 0.0..1.0"),
                                input.to_owned(),
                                "add <nodeid> [--confidence 0.0..1.0]",
                                "add concept:fridge --confidence 0.7",
                                "",
                            ));
                        }
                        confidence = Some(v);
                    }
                    "--provenance" => {
                        let upper = value.to_ascii_uppercase();
                        if !validate::VALID_PROVENANCE_CODES.contains(&upper.as_str()) {
                            return Err(SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("invalid provenance '{value}', expected U, D, or A"),
                                input.to_owned(),
                                "add <nodeid> [--provenance U|D|A]",
                                "add concept:fridge --provenance A",
                                "",
                            ));
                        }
                        provenance = Some(upper);
                    }
                    "--source" => source = Some(value),
                    "--fact" => facts.push(value),
                    "--alias" => aliases.push(value),
                    _ => unreachable!(),
                }
            }
            other => {
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    format!("unknown flag '{other}' for add"),
                    input.to_owned(),
                    "add <nodeid> [as <type>] [--name..] [--desc..] [--domain..] [--importance..] [--confidence..] [--provenance..] [--source..] [--fact..] [--alias..]",
                    "add concept:fridge --name \"Fridge\"",
                    "",
                ));
            }
        }
        pos += 1;
    }

    // Apply defaults (unless strict mode)
    if ctx.strict {
        // In strict mode, provenance is required
        if provenance.is_none() {
            return Err(SclError::new(
                category::MISSING_REQUIRED_FIELD,
                "missing required field: --provenance (U|D|A)".to_owned(),
                input.to_owned(),
                "add <nodeid> --provenance U|D|A ...",
                "add concept:fridge --provenance A --name \"Fridge\"",
                "",
            ));
        }
    }

    let provenance = provenance.unwrap_or_else(|| "A".to_owned());
    let confidence = confidence.unwrap_or(0.7);
    let importance = importance.unwrap_or(0.5);
    let name = name.or_else(|| Some(humanize_id(&id)));
    let source = source.unwrap_or_else(|| format!("OTHER {id}"));

    Ok(CanonicalLine::NodeAdd {
        id,
        node_type,
        name,
        desc,
        domain,
        importance: Some(importance),
        confidence: Some(confidence),
        provenance: Some(provenance),
        source: Some(source),
        facts,
        aliases,
    })
}

/// `modify <nodeid> [--name ".."] [--desc ".."] [--importance N] [--confidence N]`
///   `[--fact ".."] [--alias ".."]`
///
/// NOTE: `--source` is intentionally NOT supported for `modify`. Source is a
/// creation-time provenance marker that should not be changed after the fact.
/// Changing source would break audit trails and confuse data lineage tracking.
fn parse_modify(tokens: &[String], input: &str) -> Result<CanonicalLine, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            "missing node id".to_owned(),
            input.to_owned(),
            "modify <nodeid> [--name \"..\"] [--desc \"..\"] [--importance N] [--confidence N] [--fact \"..\"] [--alias \"..\"]",
            "modify concept:fridge --importance 0.9",
            "",
        ));
    }

    let id = tokens[1].clone();

    // Validate id format (best-effort, infer type from prefix)
    if let Some((prefix, _)) = id.split_once(':') {
        if let Some(typ) = validate::type_for_prefix(prefix) {
            validate::canonicalize_node_id_for_type(&id, typ).map_err(|e| {
                SclError::new(
                    category::BAD_ID_FORMAT,
                    e,
                    input.to_owned(),
                    "modify <nodeid>",
                    "modify concept:fridge --importance 0.9",
                    "",
                )
            })?;
        }
    }

    let mut name: Option<String> = None;
    let mut desc: Option<String> = None;
    let mut importance: Option<f64> = None;
    let mut confidence: Option<f64> = None;
    let mut facts: Vec<String> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();

    let mut pos = 2;
    while pos < tokens.len() {
        let flag = &tokens[pos];
        match flag.as_str() {
            "--name" | "--desc" | "--description" | "--importance" | "--confidence" | "--fact"
            | "--alias" => {
                let canonical_flag = match flag.as_str() {
                    "--description" => "--desc",
                    _ => flag.as_str(),
                };

                pos += 1;
                if pos >= tokens.len() {
                    return Err(SclError::new(
                        category::MISSING_REQUIRED_FIELD,
                        format!("missing value for {flag}"),
                        input.to_owned(),
                        "modify <nodeid> [flags]",
                        "modify concept:fridge --importance 0.9",
                        "",
                    ));
                }
                let value = tokens[pos].clone();

                match canonical_flag {
                    "--name" => name = Some(value),
                    "--desc" => desc = Some(value),
                    "--importance" => {
                        let v: f64 = value.parse().map_err(|_| {
                            SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("invalid importance '{value}'"),
                                input.to_owned(),
                                "modify <nodeid> [--importance 0.0..1.0]",
                                "modify concept:fridge --importance 0.9",
                                "",
                            )
                        })?;
                        if !(0.0..=1.0).contains(&v) {
                            return Err(SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("importance '{v}' out of range 0.0..1.0"),
                                input.to_owned(),
                                "modify <nodeid> [--importance 0.0..1.0]",
                                "modify concept:fridge --importance 0.9",
                                "",
                            ));
                        }
                        importance = Some(v);
                    }
                    "--confidence" => {
                        let v: f64 = value.parse().map_err(|_| {
                            SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("invalid confidence '{value}'"),
                                input.to_owned(),
                                "modify <nodeid> [--confidence 0.0..1.0]",
                                "modify concept:fridge --confidence 0.7",
                                "",
                            )
                        })?;
                        if !(0.0..=1.0).contains(&v) {
                            return Err(SclError::new(
                                category::BAD_VALUE_RANGE,
                                format!("confidence '{v}' out of range 0.0..1.0"),
                                input.to_owned(),
                                "modify <nodeid> [--confidence 0.0..1.0]",
                                "modify concept:fridge --confidence 0.7",
                                "",
                            ));
                        }
                        confidence = Some(v);
                    }
                    "--fact" => facts.push(value),
                    "--alias" => aliases.push(value),
                    _ => unreachable!(),
                }
            }
            "--source" => {
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    "--source is not supported for modify; source is a creation-time marker"
                        .to_owned(),
                    input.to_owned(),
                    "modify <nodeid> [--name..] [--desc..] [--importance..] [--confidence..] [--fact..] [--alias..]",
                    "modify concept:fridge --importance 0.9",
                    "",
                ));
            }
            other => {
                return Err(SclError::new(
                    category::UNKNOWN_VERB,
                    format!("unknown flag '{other}' for modify"),
                    input.to_owned(),
                    "modify <nodeid> [--name..] [--desc..] [--importance..] [--confidence..] [--fact..] [--alias..]",
                    "modify concept:fridge --importance 0.9",
                    "",
                ));
            }
        }
        pos += 1;
    }

    Ok(CanonicalLine::NodeModify {
        id,
        name,
        desc,
        importance,
        confidence,
        facts,
        aliases,
    })
}

/// `remove` disambiguation (per spec §3 and oracle review):
/// - `remove edge <src> <relation> <tgt>` (4 tokens, keyword "edge") → EdgeRemove
/// - 3 barewords where middle is a valid uppercase relation → EdgeRemove
/// - 1 bareword → NodeRemove
/// - else → ambiguous error
fn parse_remove(tokens: &[String], input: &str) -> Result<Option<CanonicalLine>, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            "missing target".to_owned(),
            input.to_owned(),
            "remove <nodeid>  or  remove edge <src> <relation> <tgt>",
            "remove concept:fridge",
            "",
        ));
    }

    // Case 1: `remove edge <src> <relation> <tgt>` (5 tokens including "remove")
    if tokens.len() >= 5 && tokens[1] == "edge" {
        return Ok(Some(CanonicalLine::EdgeRemove {
            src: tokens[2].clone(),
            relation: tokens[3].clone(),
            tgt: tokens[4].clone(),
        }));
    }

    // Case 2: exactly 4 tokens: `remove <src> <relation> <tgt>` where relation is uppercase
    if tokens.len() == 4 && is_uppercase_relation(&tokens[2]) {
        return Ok(Some(CanonicalLine::EdgeRemove {
            src: tokens[1].clone(),
            relation: tokens[2].clone(),
            tgt: tokens[3].clone(),
        }));
    }

    // Case 3: exactly 2 tokens: `remove <nodeid>`
    if tokens.len() == 2 {
        return Ok(Some(CanonicalLine::NodeRemove {
            id: tokens[1].clone(),
        }));
    }

    // Ambiguous
    Err(SclError::new(
        category::AMBIGUOUS,
        format!(
            "ambiguous remove: '{}'. Use 'remove <nodeid>' for a node or 'remove edge <src> <relation> <tgt>' for an edge",
            input
        ),
        input.to_owned(),
        "remove <nodeid>  or  remove edge <src> <relation> <tgt>",
        "remove concept:fridge  or  remove edge concept:fridge HAS feature:door",
        "",
    ))
}

/// Check if a string looks like a valid uppercase relation.
fn is_uppercase_relation(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_uppercase() || c == '_') && s.len() >= 2
}

/// `connect <src> <relation> <tgt> [--detail ".."]`
/// `add edge <src> <relation> <tgt>` (handled as alias)
fn parse_connect(tokens: &[String], input: &str) -> Result<CanonicalLine, SclError> {
    // Handle `add edge` — already 3 positionals after verb
    let offset = if tokens.len() > 1 && tokens[0] == "add" && tokens[1] == "edge" {
        2
    } else {
        1
    };

    if tokens.len() < offset + 3 {
        return Err(SclError::new(
            category::UNKNOWN_VERB,
            "expected source, relation, and target".to_owned(),
            input.to_owned(),
            "connect <src> <relation> <tgt> [--detail \"..\"]",
            "connect concept:fridge USES process:comp",
            "",
        ));
    }

    let src = tokens[offset].clone();
    let relation = tokens[offset + 1].clone().to_ascii_uppercase();
    let tgt = tokens[offset + 2].clone();

    // Validate relation
    if !validate::is_valid_relation(&relation) {
        return Err(SclError::new(
            category::UNKNOWN_RELATION,
            format!("relation '{relation}' is not valid"),
            input.to_owned(),
            "connect <src> <relation> <tgt> [--detail \"..\"]",
            "connect concept:fridge USES process:comp",
            "",
        ));
    }

    // Validate edge type rule
    if let Some((valid_src_types, valid_tgt_types)) = validate::edge_type_rule(&relation) {
        // Infer source and target types from id prefixes
        let src_type = src
            .split_once(':')
            .and_then(|(prefix, _)| validate::type_for_prefix(prefix));
        let tgt_type = tgt
            .split_once(':')
            .and_then(|(prefix, _)| validate::type_for_prefix(prefix));

        if let Some(st) = src_type {
            if !valid_src_types.is_empty() && !valid_src_types.contains(&st) {
                return Err(SclError::new(
                    category::EDGE_TYPE_MISMATCH,
                    format!("source type '{st}' is not valid for relation '{relation}'"),
                    input.to_owned(),
                    "connect <src> <relation> <tgt>",
                    &format!("use a valid source type for relation '{relation}'"),
                    "",
                ));
            }
        }
        if let Some(tt) = tgt_type {
            if !valid_tgt_types.is_empty() && !valid_tgt_types.contains(&tt) {
                return Err(SclError::new(
                    category::EDGE_TYPE_MISMATCH,
                    format!("target type '{tt}' is not valid for relation '{relation}'"),
                    input.to_owned(),
                    "connect <src> <relation> <tgt>",
                    &format!("use a valid target type for relation '{relation}'"),
                    "",
                ));
            }
        }
    }

    let mut detail = None;
    let mut pos = offset + 3;
    while pos < tokens.len() {
        if tokens[pos] == "--detail" {
            pos += 1;
            if pos >= tokens.len() {
                return Err(SclError::new(
                    category::MISSING_REQUIRED_FIELD,
                    "missing value for --detail".to_owned(),
                    input.to_owned(),
                    "connect <src> <relation> <tgt> [--detail \"..\"]",
                    "connect concept:fridge USES process:comp --detail \"primary door\"",
                    "",
                ));
            }
            detail = Some(tokens[pos].clone());
        } else {
            return Err(SclError::new(
                category::UNKNOWN_VERB,
                format!("unknown flag '{}' for connect", tokens[pos]),
                input.to_owned(),
                "connect <src> <relation> <tgt> [--detail \"..\"]",
                "connect concept:fridge USES process:comp",
                "",
            ));
        }
        pos += 1;
    }

    Ok(CanonicalLine::EdgeAdd {
        src,
        relation,
        tgt,
        detail,
    })
}

/// `disconnect <src> <relation> <tgt>`
/// `remove edge <src> <relation> <tgt>` (handled in parse_remove)
fn parse_disconnect(tokens: &[String], input: &str) -> Result<CanonicalLine, SclError> {
    if tokens.len() < 4 {
        return Err(SclError::new(
            category::UNKNOWN_VERB,
            "expected source, relation, and target".to_owned(),
            input.to_owned(),
            "disconnect <src> <relation> <tgt>",
            "disconnect concept:fridge HAS feature:door",
            "",
        ));
    }

    Ok(CanonicalLine::EdgeRemove {
        src: tokens[1].clone(),
        relation: tokens[2].clone().to_ascii_uppercase(),
        tgt: tokens[3].clone(),
    })
}

/// `list nodes | edges | types | relations | graphs`
fn parse_list(tokens: &[String], input: &str) -> Result<Option<CanonicalLine>, SclError> {
    if tokens.len() < 2 {
        return Err(SclError::new(
            category::UNKNOWN_VERB,
            "missing list target".to_owned(),
            input.to_owned(),
            "list nodes|edges|types|relations|graphs",
            "list types",
            "",
        ));
    }

    match tokens[1].as_str() {
        "types" => Ok(Some(CanonicalLine::ListTypes)),
        "relations" => Ok(Some(CanonicalLine::ListRelations)),
        "graphs" => Ok(Some(CanonicalLine::ListGraphs)),
        "nodes" => Ok(Some(CanonicalLine::ListNodes)),
        "edges" => Ok(Some(CanonicalLine::ListEdges)),
        other => Err(SclError::new(
            category::UNKNOWN_VERB,
            format!("unknown list target '{other}'"),
            input.to_owned(),
            "list nodes|edges|types|relations|graphs",
            "list types",
            "",
        )),
    }
}

/// `stats [graph <name>]`
fn parse_stats(tokens: &[String], _input: &str) -> Result<Option<CanonicalLine>, SclError> {
    if tokens.len() >= 3 && tokens[1] == "graph" {
        Ok(Some(CanonicalLine::Stats {
            graph: Some(tokens[2].clone()),
        }))
    } else {
        Ok(Some(CanonicalLine::Stats { graph: None }))
    }
}

/// `feedback <uid> yes|no|nil|pick <N>`
fn parse_feedback(tokens: &[String], input: &str) -> Result<Option<CanonicalLine>, SclError> {
    if tokens.len() < 3 {
        return Err(SclError::new(
            category::BAD_ID_FORMAT,
            "missing uid or verdict".to_owned(),
            input.to_owned(),
            "feedback <uid> yes|no|nil|pick <N>",
            "feedback abc123 yes",
            "",
        ));
    }

    let uid = tokens[1].clone();
    let verdict = tokens[2].to_ascii_lowercase();
    let pick = match verdict.as_str() {
        "yes" | "no" | "nil" => None,
        "pick" => {
            if tokens.len() < 4 {
                return Err(SclError::new(
                    category::BAD_VALUE_RANGE,
                    "missing pick number".to_owned(),
                    input.to_owned(),
                    "feedback <uid> pick <N>",
                    "feedback abc123 pick 2",
                    "",
                ));
            }
            Some(tokens[3].parse::<u32>().map_err(|_| {
                SclError::new(
                    category::BAD_VALUE_RANGE,
                    format!("invalid pick number '{}'", tokens[3]),
                    input.to_owned(),
                    "feedback <uid> pick <N>",
                    "feedback abc123 pick 2",
                    "",
                )
            })?)
        }
        other => {
            return Err(SclError::new(
                category::BAD_VALUE_RANGE,
                format!("unknown verdict '{other}', expected yes|no|nil|pick <N>"),
                input.to_owned(),
                "feedback <uid> yes|no|nil|pick <N>",
                "feedback abc123 yes",
                "",
            ));
        }
    };

    Ok(Some(CanonicalLine::Feedback {
        uid,
        verdict: verdict.to_ascii_uppercase(),
        pick,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Naive ID humanization: take suffix after `:`, split on `_`, capitalize each word.
/// Best-effort; overridable via `--name`.
fn humanize_id(id: &str) -> String {
    let suffix = id.split_once(':').map(|(_, s)| s).unwrap_or(id);
    suffix
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> Ctx {
        Ctx::new("fridge".to_owned(), false)
    }

    fn strict_ctx() -> Ctx {
        Ctx::new("fridge".to_owned(), true)
    }

    // -----------------------------------------------------------------------
    // §5 Translation table tests
    // -----------------------------------------------------------------------

    #[test]
    fn find_bareword() {
        let line = parse_line("find fridge", &test_ctx()).unwrap().unwrap();
        assert_eq!(
            line,
            CanonicalLine::NodeFind {
                query: "fridge".to_owned(),
                limit: None,
                mode: None,
                full: false,
                output_size: None,
            }
        );
        let args = line.to_args(&test_ctx());
        assert_eq!(
            args,
            vec![
                OsString::from("graph"),
                OsString::from("fridge"),
                OsString::from("node"),
                OsString::from("find"),
                OsString::from("fridge"),
            ]
        );
    }

    #[test]
    fn search_with_limit() {
        let line = parse_line("search fridge --limit 5", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::NodeFind {
                query: "fridge".to_owned(),
                limit: Some(5),
                mode: None,
                full: false,
                output_size: None,
            }
        );
        let args = line.to_args(&test_ctx());
        assert!(args.contains(&OsString::from("--limit")));
        assert!(args.contains(&OsString::from("5")));
    }

    #[test]
    fn get_concept_id() {
        let line = parse_line("get concept:fridge", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::NodeGet {
                id: "concept:fridge".to_owned(),
                full: false,
                output_size: None,
            }
        );
    }

    #[test]
    fn add_with_name() {
        let ctx = test_ctx();
        let line = parse_line("add concept:fridge --name \"Fridge\"", &ctx)
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeAdd {
                id,
                node_type,
                name,
                ..
            } => {
                assert_eq!(id, "concept:fridge");
                assert_eq!(node_type, "Concept");
                assert_eq!(name.as_deref(), Some("Fridge"));
            }
            _ => panic!("expected NodeAdd"),
        }
        let args = line.to_args(&ctx);
        assert!(args.contains(&OsString::from("--type")));
        assert!(args.contains(&OsString::from("Concept")));
        assert!(args.contains(&OsString::from("--provenance")));
        assert!(args.contains(&OsString::from("A")));
        // Default source: "OTHER <id>"
        assert!(args.contains(&OsString::from("--source")));
    }

    #[test]
    fn create_bug_with_name() {
        let ctx = test_ctx();
        let line = parse_line("create bug:leak --name \"Leak\"", &ctx)
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeAdd {
                id,
                node_type,
                name,
                ..
            } => {
                assert_eq!(id, "bug:leak");
                // Bug is a valid type, not in TYPE_TO_PREFIX, but we use prefix verbatim
                assert_eq!(node_type, "Bug");
                assert_eq!(name.as_deref(), Some("Leak"));
            }
            _ => panic!("expected NodeAdd"),
        }
    }

    #[test]
    fn modify_importance() {
        let line = parse_line("modify concept:fridge --importance 0.9", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeModify { id, importance, .. } => {
                assert_eq!(id, "concept:fridge");
                assert_eq!(*importance, Some(0.9));
            }
            _ => panic!("expected NodeModify"),
        }
    }

    #[test]
    fn remove_node() {
        let line = parse_line("remove concept:fridge", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::NodeRemove {
                id: "concept:fridge".to_owned(),
            }
        );
    }

    #[test]
    fn connect_edge() {
        let line = parse_line("connect concept:fridge USES process:comp", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::EdgeAdd {
                src: "concept:fridge".to_owned(),
                relation: "USES".to_owned(),
                tgt: "process:comp".to_owned(),
                detail: None,
            }
        );
    }

    #[test]
    fn add_edge_alias() {
        let line = parse_line(
            "add edge concept:fridge HAS feature:door --detail \"primary door\"",
            &test_ctx(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            line,
            CanonicalLine::EdgeAdd {
                src: "concept:fridge".to_owned(),
                relation: "HAS".to_owned(),
                tgt: "feature:door".to_owned(),
                detail: Some("primary door".to_owned()),
            }
        );
    }

    #[test]
    fn disconnect_edge() {
        let line = parse_line("disconnect concept:fridge HAS feature:door", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::EdgeRemove {
                src: "concept:fridge".to_owned(),
                relation: "HAS".to_owned(),
                tgt: "feature:door".to_owned(),
            }
        );
    }

    #[test]
    fn list_types() {
        let line = parse_line("list types", &test_ctx()).unwrap().unwrap();
        assert_eq!(line, CanonicalLine::ListTypes);
        assert!(line.to_args(&test_ctx()).is_empty());
    }

    #[test]
    fn stats_default_graph() {
        let line = parse_line("stats", &test_ctx()).unwrap().unwrap();
        match &line {
            CanonicalLine::Stats { graph } => assert!(graph.is_none()),
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn feedback_yes() {
        let line = parse_line("feedback abc123 yes", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::Feedback { uid, verdict, pick } => {
                assert_eq!(uid, "abc123");
                assert_eq!(verdict, "YES");
                assert_eq!(*pick, None);
            }
            _ => panic!("expected Feedback"),
        }
    }

    #[test]
    fn feedback_pick() {
        let line = parse_line("feedback abc123 pick 2", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::Feedback { uid, verdict, pick } => {
                assert_eq!(uid, "abc123");
                assert_eq!(verdict, "PICK");
                assert_eq!(*pick, Some(2));
            }
            _ => panic!("expected Feedback"),
        }
    }

    #[test]
    fn use_graph() {
        let mut ctx = Ctx::new("default".to_owned(), false);
        let result = parse_script("use fridge", &mut ctx).unwrap();
        assert_eq!(ctx.graph, "fridge");
        assert!(result.is_empty());
    }

    #[test]
    fn strict_mode() {
        let mut ctx = Ctx::new("fridge".to_owned(), false);
        let result = parse_script("strict", &mut ctx).unwrap();
        assert!(ctx.strict);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn empty_script_returns_help() {
        let mut ctx = Ctx::new("fridge".to_owned(), false);
        let result = parse_script("", &mut ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], CanonicalLine::Help { topic: None });
    }

    #[test]
    fn whitespace_script_returns_help() {
        let mut ctx = Ctx::new("fridge".to_owned(), false);
        let result = parse_script("   \n  ", &mut ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], CanonicalLine::Help { topic: None });
    }

    #[test]
    fn unknown_verb_returns_none() {
        let result = parse_line("something weird", &test_ctx()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn quoted_multiword_args() {
        let line = parse_line("find \"smart fridge\" --limit 5 --full", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeFind {
                query, limit, full, ..
            } => {
                assert_eq!(query, "smart fridge");
                assert_eq!(*limit, Some(5));
                assert!(*full);
            }
            _ => panic!("expected NodeFind"),
        }
    }

    #[test]
    fn source_override_disables_default() {
        let ctx = test_ctx();
        let line = parse_line(
            "add concept:fridge --name \"Fridge\" --source \"DOC /docs/manual.pdf\"",
            &ctx,
        )
        .unwrap()
        .unwrap();
        match &line {
            CanonicalLine::NodeAdd { source, .. } => {
                assert_eq!(source.as_deref(), Some("DOC /docs/manual.pdf"));
            }
            _ => panic!("expected NodeAdd"),
        }
        // Verify the synthetic "OTHER" source is NOT present
        let args = line.to_args(&ctx);
        let source_idx = args.iter().position(|a| a == "--source");
        assert!(source_idx.is_some());
        let source_val = &args[source_idx.unwrap() + 1];
        assert_eq!(source_val, "DOC /docs/manual.pdf");
    }

    #[test]
    fn strict_mode_rejects_missing_provenance() {
        let ctx = strict_ctx();
        let err = parse_line("add concept:fridge --name \"Fridge\"", &ctx).unwrap_err();
        assert_eq!(err.category, category::MISSING_REQUIRED_FIELD);
        assert!(err.message.contains("--provenance"));
    }

    #[test]
    fn use_persists_across_semicolon() {
        let mut ctx = Ctx::new("default".to_owned(), false);
        let result = parse_script("use fridge; find compressor", &mut ctx).unwrap();
        assert_eq!(ctx.graph, "fridge");
        assert_eq!(result.len(), 1);
        match &result[0] {
            CanonicalLine::NodeFind { query, .. } => assert_eq!(query, "compressor"),
            _ => panic!("expected NodeFind"),
        }
    }

    #[test]
    fn feedback_inside_script() {
        let mut ctx = test_ctx();
        let result = parse_script("find compressor\nfeedback abc123 yes", &mut ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], CanonicalLine::NodeFind { .. }));
        assert!(matches!(result[1], CanonicalLine::Feedback { .. }));
    }

    #[test]
    fn unknown_id_prefix_error() {
        let ctx = test_ctx();
        // Missing colon → bad format error
        let err = parse_line("get nothing", &ctx).unwrap_err();
        assert_eq!(err.category, category::BAD_ID_FORMAT);
        assert!(err.message.contains("<type>:snake_case"));

        // Empty suffix → bad format error
        let err = parse_line("get concept:", &ctx).unwrap_err();
        assert_eq!(err.category, category::BAD_ID_FORMAT);

        // Unknown prefix with valid format is accepted (custom type)
        let line = parse_line("get unknown:thing", &ctx).unwrap();
        assert!(line.is_some());
        match line.unwrap() {
            CanonicalLine::NodeGet { id, .. } => assert_eq!(id, "unknown:thing"),
            _ => panic!("expected NodeGet"),
        }
    }

    #[test]
    fn invalid_relation_for_source_target() {
        let ctx = test_ctx();
        // STORED_IN requires source type Concept/Process/Rule and target DataStore
        let err = parse_line("connect concept:fridge STORED_IN process:comp", &ctx).unwrap_err();
        // process is not a valid target for STORED_IN (target should be DataStore)
        assert_eq!(err.category, category::EDGE_TYPE_MISMATCH);
    }

    #[test]
    fn remove_disambiguation_node() {
        // 1 bareword → NodeRemove
        let line = parse_line("remove concept:fridge", &test_ctx())
            .unwrap()
            .unwrap();
        assert!(matches!(line, CanonicalLine::NodeRemove { .. }));
    }

    #[test]
    fn remove_disambiguation_edge_3word() {
        // 3 barewords with uppercase relation → EdgeRemove
        let line = parse_line("remove concept:fridge HAS feature:door", &test_ctx())
            .unwrap()
            .unwrap();
        assert!(matches!(line, CanonicalLine::EdgeRemove { .. }));
    }

    #[test]
    fn remove_disambiguation_edge_keyword() {
        // `remove edge <src> <rel> <tgt>` → EdgeRemove
        let line = parse_line("remove edge concept:fridge HAS feature:door", &test_ctx())
            .unwrap()
            .unwrap();
        assert!(matches!(line, CanonicalLine::EdgeRemove { .. }));
    }

    #[test]
    fn remove_disambiguation_ambiguous() {
        // 3 barewords where middle is NOT a valid uppercase relation → error
        let err = parse_line("remove concept:fridge foo feature:door", &test_ctx()).unwrap_err();
        assert_eq!(err.category, category::AMBIGUOUS);
    }

    #[test]
    fn help_with_topic() {
        let line = parse_line("help find", &test_ctx()).unwrap().unwrap();
        match &line {
            CanonicalLine::Help { topic } => {
                assert_eq!(topic.as_deref(), Some("find"));
            }
            _ => panic!("expected Help"),
        }
    }

    #[test]
    fn help_no_topic() {
        let line = parse_line("help", &test_ctx()).unwrap().unwrap();
        match &line {
            CanonicalLine::Help { topic } => assert!(topic.is_none()),
            _ => panic!("expected Help"),
        }
    }

    #[test]
    fn list_relations() {
        let line = parse_line("list relations", &test_ctx()).unwrap().unwrap();
        assert_eq!(line, CanonicalLine::ListRelations);
    }

    #[test]
    fn list_graphs() {
        let line = parse_line("list graphs", &test_ctx()).unwrap().unwrap();
        assert_eq!(line, CanonicalLine::ListGraphs);
    }

    #[test]
    fn list_nodes() {
        let line = parse_line("list nodes", &test_ctx()).unwrap().unwrap();
        assert_eq!(line, CanonicalLine::ListNodes);
    }

    #[test]
    fn list_edges() {
        let line = parse_line("list edges", &test_ctx()).unwrap().unwrap();
        assert_eq!(line, CanonicalLine::ListEdges);
    }

    #[test]
    fn stats_with_graph() {
        let line = parse_line("stats graph other_graph", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::Stats { graph } => {
                assert_eq!(graph.as_deref(), Some("other_graph"));
            }
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn stats_with_graph_to_args() {
        let line = parse_line("stats graph other_graph", &test_ctx())
            .unwrap()
            .unwrap();
        let args = line.to_args(&test_ctx());
        // stats with explicit graph should use that graph
        assert!(args.contains(&OsString::from("other_graph")));
    }

    #[test]
    fn modify_with_source_rejected() {
        let err = parse_line(
            "modify concept:fridge --source \"DOC doc.pdf\"",
            &test_ctx(),
        )
        .unwrap_err();
        assert_eq!(err.category, category::UNKNOWN_VERB);
        assert!(err.message.contains("--source is not supported"));
    }

    #[test]
    fn get_with_output_size() {
        let line = parse_line("get concept:fridge --full --output-size 500", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeGet {
                id,
                full,
                output_size,
                ..
            } => {
                assert_eq!(id, "concept:fridge");
                assert!(*full);
                assert_eq!(*output_size, Some(500));
            }
            _ => panic!("expected NodeGet"),
        }
    }

    #[test]
    fn humanize_id_works() {
        assert_eq!(humanize_id("concept:smart_fridge"), "Smart Fridge");
        assert_eq!(humanize_id("bug:door_seal_leak"), "Door Seal Leak");
        assert_eq!(humanize_id("process:do_thing"), "Do Thing");
        assert_eq!(humanize_id("simple"), "Simple");
    }

    #[test]
    fn add_defaults_applied() {
        let ctx = test_ctx();
        let line = parse_line("add concept:fridge", &ctx).unwrap().unwrap();
        match &line {
            CanonicalLine::NodeAdd {
                name,
                provenance,
                confidence,
                importance,
                source,
                ..
            } => {
                assert_eq!(name.as_deref(), Some("Fridge"));
                assert_eq!(provenance.as_deref(), Some("A"));
                assert_eq!(*confidence, Some(0.7));
                assert_eq!(*importance, Some(0.5));
                assert_eq!(source.as_deref(), Some("OTHER concept:fridge"));
            }
            _ => panic!("expected NodeAdd"),
        }
    }

    #[test]
    fn create_with_as_type() {
        let ctx = test_ctx();
        // `as Process` explicitly overrides the inferred type.
        // The id prefix `process` matches the Process type.
        let line = parse_line("add process:comp as Process --name \"Compressor\"", &ctx)
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeAdd {
                id,
                node_type,
                name,
                ..
            } => {
                assert_eq!(id, "process:comp");
                assert_eq!(node_type, "Process");
                assert_eq!(name.as_deref(), Some("Compressor"));
            }
            _ => panic!("expected NodeAdd"),
        }
    }

    // -----------------------------------------------------------------------
    // split_script / tokenize_command tests (moved from kg-mcp.rs)
    // -----------------------------------------------------------------------

    #[test]
    fn split_script_handles_semicolons_and_newlines() {
        let parts = split_script("a;b\nc");
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_script_respects_quotes() {
        let parts = split_script("a; \"b;c\"; 'd;e'");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].trim(), "\"b;c\"");
        assert_eq!(parts[2].trim(), "'d;e'");
    }

    #[test]
    fn split_script_allows_escaped_delimiter() {
        let parts = split_script("a\\;b;c");
        assert_eq!(parts, vec!["a\\;b", "c"]);
    }

    #[test]
    fn tokenize_command_parses_quotes_and_escapes() {
        let tokens = tokenize_command("fridge node find \"smart fridge\"").expect("tokenize");
        assert_eq!(tokens, vec!["fridge", "node", "find", "smart fridge"]);
    }

    #[test]
    fn tokenize_command_handles_escaped_semicolon() {
        let tokens = tokenize_command("note\\;extra").expect("tokenize");
        assert_eq!(tokens, vec!["note;extra"]);
    }

    #[test]
    fn tokenize_command_errors_on_unterminated_quote() {
        let err = tokenize_command("fridge node find \"smart").unwrap_err();
        assert_eq!(err, "unterminated quote");
    }

    #[test]
    fn link_alias_works() {
        let line = parse_line("link x HAS y", &test_ctx()).unwrap().unwrap();
        assert!(matches!(line, CanonicalLine::EdgeAdd { .. }));
    }

    #[test]
    fn feedback_nil() {
        let line = parse_line("feedback uid123 nil", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::Feedback { uid, verdict, pick } => {
                assert_eq!(uid, "uid123");
                assert_eq!(verdict, "NIL");
                assert_eq!(*pick, None);
            }
            _ => panic!("expected Feedback"),
        }
    }

    #[test]
    fn feedback_no() {
        let line = parse_line("feedback uid123 no", &test_ctx())
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::Feedback { uid, verdict, pick } => {
                assert_eq!(uid, "uid123");
                assert_eq!(verdict, "NO");
                assert_eq!(*pick, None);
            }
            _ => panic!("expected Feedback"),
        }
    }

    #[test]
    fn add_edge_directly() {
        // `add edge` with no connect keyword
        let line = parse_line("add edge x HAS y", &test_ctx())
            .unwrap()
            .unwrap();
        assert_eq!(
            line,
            CanonicalLine::EdgeAdd {
                src: "x".to_owned(),
                relation: "HAS".to_owned(),
                tgt: "y".to_owned(),
                detail: None,
            }
        );
    }

    #[test]
    fn remove_edge_keyword_only() {
        let line = parse_line("remove edge concept:a HAS concept:b", &test_ctx())
            .unwrap()
            .unwrap();
        assert!(matches!(line, CanonicalLine::EdgeRemove { .. }));
    }

    #[test]
    fn parse_script_with_multiple_commands() {
        let mut ctx = test_ctx();
        let result = parse_script("find fridge; get concept:fridge; list types", &mut ctx).unwrap();
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], CanonicalLine::NodeFind { .. }));
        assert!(matches!(result[1], CanonicalLine::NodeGet { .. }));
        assert!(matches!(result[2], CanonicalLine::ListTypes));
    }

    #[test]
    fn add_with_facts_and_aliases() {
        let ctx = test_ctx();
        let line = parse_line(
            "add concept:fridge --fact \"cools food\" --fact \"has door\" --alias chlodnica --alias icebox",
            &ctx,
        )
        .unwrap()
        .unwrap();
        match &line {
            CanonicalLine::NodeAdd { facts, aliases, .. } => {
                assert_eq!(facts.len(), 2);
                assert_eq!(facts[0], "cools food");
                assert_eq!(facts[1], "has door");
                assert_eq!(aliases.len(), 2);
                assert_eq!(aliases[0], "chlodnica");
                assert_eq!(aliases[1], "icebox");
            }
            _ => panic!("expected NodeAdd"),
        }
    }

    #[test]
    fn add_with_domain() {
        let ctx = test_ctx();
        let line = parse_line("add concept:fridge --domain kitchen", &ctx)
            .unwrap()
            .unwrap();
        match &line {
            CanonicalLine::NodeAdd { domain, .. } => {
                assert_eq!(domain.as_deref(), Some("kitchen"));
            }
            _ => panic!("expected NodeAdd"),
        }
    }

    #[test]
    fn get_rejects_bad_id_format() {
        let ctx = test_ctx();
        let err = parse_line("get no_colon", &ctx).unwrap_err();
        assert_eq!(err.category, category::BAD_ID_FORMAT);
    }

    #[test]
    fn scl_error_display() {
        let err = SclError::new(
            category::UNKNOWN_RELATION,
            "relation 'OWNS' is not valid".to_owned(),
            "connect x OWNS y".to_owned(),
            "connect <src> <relation> <tgt>",
            "connect x HAS y",
            "<g> edge add x HAS y",
        );
        let display = err.to_string();
        assert!(display.contains("unknown_relation"));
        assert!(display.contains("OWNS"));
        assert!(display.contains("HAS"));
    }
}
