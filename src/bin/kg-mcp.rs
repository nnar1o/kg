use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, JsonSchema)]
struct KgCommandArgs {
    #[schemars(description = "Arguments passed to `kg` (without the binary name)")]
    args: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Enable per-request debug output")]
    debug: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KgScriptArgs {
    #[schemars(description = "Script with one or more kg commands separated by ';' or newlines")]
    script: String,
    /// best_effort (default) or strict
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    #[schemars(description = "Enable per-request debug output")]
    debug: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateGraphArgs {
    graph_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EmptyArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeFindArgs {
    graph: String,
    queries: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    output_size: Option<usize>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    full: bool,
    #[serde(default)]
    skip_feedback: bool,
    #[serde(default)]
    debug: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeGetArgs {
    graph: String,
    id: String,
    #[serde(default)]
    output_size: Option<usize>,
    #[serde(default)]
    full: bool,
    #[serde(default)]
    debug: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeAddArgs {
    graph: String,
    id: String,
    node_type: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    domain_area: Option<String>,
    #[serde(default)]
    provenance: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeModifyArgs {
    graph: String,
    id: String,
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    domain_area: Option<String>,
    #[serde(default)]
    provenance: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeRemoveArgs {
    graph: String,
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EdgeAddArgs {
    graph: String,
    source_id: String,
    relation: String,
    target_id: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EdgeAddBatchItem {
    source_id: String,
    relation: String,
    target_id: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EdgeAddBatchArgs {
    graph: String,
    edges: Vec<EdgeAddBatchItem>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EdgeRemoveArgs {
    graph: String,
    source_id: String,
    relation: String,
    target_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GraphStatsArgs {
    graph: String,
    #[serde(default)]
    include_features: bool,
    #[serde(default)]
    by_type: bool,
    #[serde(default)]
    by_relation: bool,
    #[serde(default)]
    show_sources: bool,
}

#[derive(Debug, Clone)]
struct EdgePreflightFailure {
    message: String,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GraphValidateArgs {
    graph: String,
    #[serde(default)]
    deep: bool,
    #[serde(default)]
    base_dir: Option<String>,
    #[serde(default)]
    errors_only: bool,
    #[serde(default)]
    warnings_only: bool,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GraphQualityArgs {
    graph: String,
    command: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    node_types: Vec<String>,
    #[serde(default)]
    include_features: Option<bool>,
    #[serde(default)]
    threshold: Option<f64>,
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    sort: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExportHtmlArgs {
    graph: String,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AccessLogArgs {
    graph: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    show_empty: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AccessStatsArgs {
    graph: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GapSummaryArgs {
    graph: String,
    #[serde(default = "default_gap_limit")]
    limit: usize,
}

fn default_gap_limit() -> usize {
    5
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct KgPromptArgs {
    graph: String,
    goal: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FeedbackBatchArgs {
    #[schemars(description = "Feedback lines, e.g. `uid=abc123 YES` or `uid=abc123 PICK 2`")]
    lines: Vec<String>,
    /// best_effort (default) or strict
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeAddBatchItem {
    id: String,
    node_type: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    domain_area: Option<String>,
    #[serde(default)]
    provenance: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeAddBatchArgs {
    graph: String,
    nodes: Vec<NodeAddBatchItem>,
    /// atomic (default) or best_effort
    #[serde(default)]
    mode: Option<String>,
    /// error (default) or skip
    #[serde(default)]
    on_conflict: Option<String>,
}

#[derive(Debug, Clone)]
struct FindContext {
    created_at_ms: u128,
    graph: String,
    queries: Vec<String>,
    candidate_ids: Vec<String>,
}

const VALID_PROVENANCE_CODES: &[&str] = &["U", "D", "A"];
const VALID_SOURCE_TYPES: &[&str] = &[
    "URL",
    "SVN",
    "SOURCECODE",
    "WIKI",
    "CONFLUENCE",
    "CONVERSATION",
    "GIT_COMMIT",
    "PULL_REQUEST",
    "ISSUE",
    "DOC",
    "LOG",
    "OTHER",
];

fn is_valid_iso_utc_timestamp(value: &str) -> bool {
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
    true
}

fn is_valid_iso_date(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let bytes = value.as_bytes();
    let is_digit = |idx: usize| bytes.get(idx).is_some_and(|b| b.is_ascii_digit());
    is_digit(0)
        && is_digit(1)
        && is_digit(2)
        && is_digit(3)
        && bytes.get(4) == Some(&b'-')
        && is_digit(5)
        && is_digit(6)
        && bytes.get(7) == Some(&b'-')
        && is_digit(8)
        && is_digit(9)
}

fn validate_source_reference(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("source entry cannot be empty".to_owned());
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("source must match '<TYPE> <LINK_OR_DATE> <OPTIONAL_DETAILS>'".to_owned());
    }
    if !VALID_SOURCE_TYPES.contains(&parts[0]) {
        return Err(format!(
            "invalid source type '{}'; valid types: {}",
            parts[0],
            VALID_SOURCE_TYPES.join(", ")
        ));
    }
    if parts[0] == "CONVERSATION" && !is_valid_iso_date(parts[1]) {
        return Err("CONVERSATION source must use date YYYY-MM-DD".to_owned());
    }
    if parts[0] == "GIT_COMMIT" && parts.len() < 3 {
        return Err("GIT_COMMIT source must include repository and commit SHA".to_owned());
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct QueryFeedbackStats {
    yes: u64,
    no: u64,
    nil: u64,
    pick: u64,
}

impl QueryFeedbackStats {
    fn events(&self) -> u64 {
        self.yes + self.no + self.nil + self.pick
    }

    fn negative_ratio(&self) -> f64 {
        let total = self.events();
        if total == 0 {
            0.0
        } else {
            (self.no + self.nil) as f64 / total as f64
        }
    }

    fn positive_ratio(&self) -> f64 {
        let total = self.events();
        if total == 0 {
            0.0
        } else {
            (self.yes + self.pick) as f64 / total as f64
        }
    }

    fn add_action(&mut self, action: &str) {
        match action {
            "YES" => self.yes = self.yes.saturating_add(1),
            "NO" => self.no = self.no.saturating_add(1),
            "NIL" => self.nil = self.nil.saturating_add(1),
            "PICK" => self.pick = self.pick.saturating_add(1),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GlobalFeedbackStats {
    events: u64,
    negative_events: u64,
}

impl GlobalFeedbackStats {
    fn negative_ratio(&self) -> f64 {
        if self.events == 0 {
            0.0
        } else {
            self.negative_events as f64 / self.events as f64
        }
    }

    fn add_action(&mut self, action: &str) {
        self.events = self.events.saturating_add(1);
        if matches!(action, "NO" | "NIL") {
            self.negative_events = self.negative_events.saturating_add(1);
        }
    }
}

#[derive(Debug, Default)]
struct FeedbackState {
    counter: u64,
    finds: HashMap<String, FindContext>,
    query_stats: HashMap<String, QueryFeedbackStats>,
    global_stats: GlobalFeedbackStats,
}

struct FeedbackBatchRun {
    ok: usize,
    failed: usize,
    items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct FeedbackUpdate {
    item_index: usize,
    graph: String,
    node_id: String,
    action: String,
    delta: f64,
    ts_ms: u128,
}

#[derive(Debug, Clone)]
struct PendingFindFeedback {
    uid: String,
    candidate_ids: Vec<String>,
}

#[derive(Clone)]
struct KgMcpServer {
    cwd: PathBuf,
    nudge_percent: u8,
    user_short_uid: String,
    exec_lock: Arc<Mutex<()>>,
    feedback_state: Arc<Mutex<FeedbackState>>,
    trace_counter: Arc<AtomicU64>,
    debug_from_env: bool,
    tool_router: ToolRouter<KgMcpServer>,
    prompt_router: PromptRouter<KgMcpServer>,
}

#[tool_router]
impl KgMcpServer {
    fn new(cwd: PathBuf) -> anyhow::Result<Self> {
        let nudge_percent = kg::feedback_nudge_percent(&cwd)?;
        let user_short_uid = kg::sidecar_user_short_uid(&cwd);
        let feedback_state = initialize_feedback_state(&cwd);
        Ok(Self {
            cwd,
            nudge_percent,
            user_short_uid,
            exec_lock: Arc::new(Mutex::new(())),
            feedback_state: Arc::new(Mutex::new(feedback_state)),
            trace_counter: Arc::new(AtomicU64::new(0)),
            debug_from_env: env_flag_enabled("KG_MCP_DEBUG"),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        })
    }

    fn run_kg(
        &self,
        os_args: Vec<OsString>,
        tool_name: &str,
        operation: &str,
        request_debug: bool,
    ) -> Result<String, McpError> {
        let trace_id = self.next_trace_id();
        let debug_enabled = self.debug_enabled(request_debug);
        let started_at = Instant::now();
        let raw_args: Vec<String> = os_args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        let redacted_args = redact_cli_args(&raw_args);
        let redacted_command = redacted_args.join(" ");

        let _guard = self.exec_lock.lock().map_err(|_| {
            let mut data = json!({
                "tool": tool_name,
                "operation": operation,
                "trace_id": trace_id,
                "cli_command": redacted_command,
                "reason": "previous command panicked",
            });
            if debug_enabled {
                data["debug"] = json!({
                    "cwd": self.cwd.display().to_string(),
                    "cli_args": redacted_args,
                    "duration_ms": started_at.elapsed().as_millis(),
                });
            }
            McpError::internal_error("kg command lock poisoned", Some(data))
        })?;

        let cwd = self.cwd.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            kg::run_args_safe(os_args, &cwd)
        }));

        match result {
            Ok(Ok(rendered)) => Ok(rendered),
            Ok(Err(err)) => {
                let stderr = kg::format_error_chain(&err);
                let duration_ms = started_at.elapsed().as_millis();
                let (code, message, kind, exit_code) = classify_kg_error(&stderr);
                let mut data = json!({
                    "tool": tool_name,
                    "operation": operation,
                    "trace_id": trace_id,
                    "error_kind": kind,
                    "cli_command": redacted_command,
                    "exit_code": exit_code,
                    "stdout_tail": "",
                    "stderr_tail": last_lines(&stderr, 30),
                    "hint": error_hint(kind),
                });
                if debug_enabled {
                    data["debug"] = json!({
                        "cwd": self.cwd.display().to_string(),
                        "cli_args": redacted_args,
                        "duration_ms": duration_ms,
                        "raw_error": last_lines(&stderr, 80),
                    });
                }
                Err(McpError::new(code, message, Some(data)))
            }
            Err(payload) => {
                let panic = panic_payload_to_string(payload);
                let duration_ms = started_at.elapsed().as_millis();
                let mut data = json!({
                    "tool": tool_name,
                    "operation": operation,
                    "trace_id": trace_id,
                    "error_kind": "panic",
                    "cli_command": redacted_command,
                    "exit_code": 101,
                    "stdout_tail": "",
                    "stderr_tail": last_lines(&panic, 30),
                    "hint": "Inspect panic payload and stack trace in logs.",
                });
                if debug_enabled {
                    data["debug"] = json!({
                        "cwd": self.cwd.display().to_string(),
                        "cli_args": redacted_args,
                        "duration_ms": duration_ms,
                        "raw_error": panic,
                    });
                }
                Err(McpError::internal_error("kg command panicked", Some(data)))
            }
        }
    }

    fn execute_kg(&self, args: Vec<String>) -> Result<CallToolResult, McpError> {
        self.execute_kg_for("kg_typed", args, false)
    }

    fn load_graph_file(&self, graph: &str) -> Result<kg::GraphFile, McpError> {
        let graph_root = kg::default_graph_root(&self.cwd);
        let path = kg::resolve_graph_path(&self.cwd, &graph_root, graph).map_err(|error| {
            McpError::invalid_params(
                "graph not found",
                Some(json!({
                    "graph": graph,
                    "hint": error.to_string(),
                })),
            )
        })?;

        kg::GraphFile::load(&path).map_err(|error| {
            let detail = kg::format_error_chain(&error);
            McpError::internal_error(
                format!("failed to load graph: {detail}"),
                Some(json!({
                    "graph": graph,
                    "path": path.display().to_string(),
                    "error": detail,
                })),
            )
        })
    }

    fn preflight_edge(
        &self,
        graph: &kg::GraphFile,
        source_id: &str,
        relation: &str,
        target_id: &str,
    ) -> Result<(), EdgePreflightFailure> {
        if !kg::VALID_RELATIONS.contains(&relation) {
            return Err(EdgePreflightFailure {
                message: format!(
                    "invalid relation '{}' (allowed: {})",
                    relation,
                    kg::VALID_RELATIONS.join(", ")
                ),
                data: json!({
                    "relation": relation,
                    "valid_relations": kg::VALID_RELATIONS,
                    "hint": "Call kg_schema before adding edges to inspect allowed relations and edge rules.",
                }),
            });
        }

        let source_node = graph.node_by_id(source_id);
        let target_node = graph.node_by_id(target_id);

        if source_node.is_none() || target_node.is_none() {
            return Err(EdgePreflightFailure {
                message: format!(
                    "source/target node missing: {} {} {}",
                    source_id, relation, target_id
                ),
                data: json!({
                    "source_id": source_id,
                    "target_id": target_id,
                    "source_found": source_node.is_some(),
                    "target_found": target_node.is_some(),
                }),
            });
        }

        let source_node = source_node.expect("source checked above");
        let target_node = target_node.expect("target checked above");

        if let Some((valid_src, valid_tgt)) = kg::edge_type_rule(relation) {
            if !valid_src.is_empty() && !valid_src.contains(&source_node.r#type.as_str()) {
                return Err(EdgePreflightFailure {
                    message: kg::format_edge_source_type_error(
                        &source_node.r#type,
                        relation,
                        valid_src,
                    ),
                    data: json!({
                        "relation": relation,
                        "source_id": source_id,
                        "source_type": source_node.r#type,
                        "valid_source_types": valid_src,
                        "hint": "Call kg_schema before adding edges to inspect allowed relations and edge rules.",
                    }),
                });
            }
            if !valid_tgt.is_empty() && !valid_tgt.contains(&target_node.r#type.as_str()) {
                return Err(EdgePreflightFailure {
                    message: kg::format_edge_target_type_error(
                        &target_node.r#type,
                        relation,
                        valid_tgt,
                    ),
                    data: json!({
                        "relation": relation,
                        "target_id": target_id,
                        "target_type": target_node.r#type,
                        "valid_target_types": valid_tgt,
                        "hint": "Call kg_schema before adding edges to inspect allowed relations and edge rules.",
                    }),
                });
            }
        }

        if graph.has_edge(source_id, relation, target_id) {
            return Err(EdgePreflightFailure {
                message: format!(
                    "edge already exists: {} {} {}",
                    source_id, relation, target_id
                ),
                data: json!({
                    "source_id": source_id,
                    "relation": relation,
                    "target_id": target_id,
                }),
            });
        }

        Ok(())
    }

    fn apply_edge_to_graph(graph: &mut kg::GraphFile, edge: &EdgeAddBatchItem) {
        graph.edges.push(kg::Edge {
            source_id: edge.source_id.clone(),
            relation: edge.relation.clone(),
            target_id: edge.target_id.clone(),
            properties: kg::EdgeProperties {
                detail: edge.detail.clone().unwrap_or_default(),
                ..kg::EdgeProperties::default()
            },
        });
        graph.refresh_counts();
    }

    fn format_edge_batch_output(
        &self,
        dry_run: bool,
        ok_count: usize,
        failed_count: usize,
        results: &[serde_json::Value],
    ) -> String {
        let mut lines = vec![if dry_run {
            format!(
                "OK (dry_run would_add={} failed={})",
                ok_count, failed_count
            )
        } else {
            format!("OK (added={} failed={})", ok_count, failed_count)
        }];

        for result in results.iter().filter(|result| result["ok"] == false) {
            lines.push(format!(
                "- edge {} {} {} failed: {}",
                result["source_id"].as_str().unwrap_or("?"),
                result["relation"].as_str().unwrap_or("?"),
                result["target_id"].as_str().unwrap_or("?"),
                result["error"].as_str().unwrap_or("unknown error")
            ));
        }

        format!("{}\n", lines.join("\n"))
    }

    fn execute_kg_for(
        &self,
        tool_name: &str,
        args: Vec<String>,
        request_debug: bool,
    ) -> Result<CallToolResult, McpError> {
        let operation = args
            .first()
            .map(|v| v.as_str())
            .unwrap_or("(empty command)")
            .to_owned();
        let mut os_args = Vec::with_capacity(args.len() + 1);
        os_args.push(OsString::from("kg"));
        os_args.extend(args.into_iter().map(OsString::from));

        let rendered = self.run_kg(os_args, tool_name, &operation, request_debug)?;

        let structured_content = if self.debug_enabled(request_debug) {
            Some(json!({
                "debug": {
                    "tool": tool_name,
                    "operation": operation,
                }
            }))
        } else {
            None
        };

        Ok(CallToolResult {
            content: vec![Content::text(rendered)],
            structured_content,
            is_error: Some(false),
            meta: None,
        })
    }

    fn debug_enabled(&self, request_debug: bool) -> bool {
        request_debug || self.debug_from_env
    }

    fn next_trace_id(&self) -> String {
        let seq = self.trace_counter.fetch_add(1, Ordering::Relaxed) + 1;
        format!("kgmcp-{}-{}", Self::now_ms(), to_base36(seq))
    }

    fn parse_mode(&self, mode: Option<String>) -> Result<String, McpError> {
        let mode = mode.unwrap_or_else(|| "best_effort".to_owned());
        if mode != "best_effort" && mode != "strict" {
            return Err(McpError::invalid_params(
                "invalid mode",
                Some(json!({
                    "expected": ["best_effort", "strict"],
                    "got": mode,
                })),
            ));
        }
        Ok(mode)
    }

    fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    fn next_uid(&self) -> Result<String, McpError> {
        let mut state = self.feedback_state.lock().map_err(|_| {
            McpError::internal_error(
                "kg feedback state lock poisoned",
                Some(json!({ "reason": "previous tool panicked" })),
            )
        })?;
        state.counter = state.counter.wrapping_add(1);
        let seed = (Self::now_ms() as u64) ^ state.counter.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut s = to_base36(seed);
        if s.len() > 6 {
            s = s.split_off(s.len() - 6);
        } else if s.len() < 6 {
            s = format!("{:0>6}", s);
        }
        Ok(s)
    }

    fn adaptive_nudge_percent(
        &self,
        queries: &[String],
        total_results: usize,
    ) -> Result<u8, McpError> {
        let state = self.feedback_state.lock().map_err(|_| {
            McpError::internal_error(
                "kg feedback state lock poisoned",
                Some(json!({ "reason": "previous tool panicked" })),
            )
        })?;
        Ok(compute_adaptive_nudge_percent(
            self.nudge_percent,
            queries,
            total_results,
            &state.query_stats,
            &state.global_stats,
        ))
    }

    fn append_feedback_log(&self, data: &str) {
        // Best-effort logging; never fail tool calls due to IO.
        let path = kg::feedback_log_path(&self.cwd);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(data.as_bytes());
        }
    }

    fn apply_feedback_updates(
        &self,
        updates: &[FeedbackUpdate],
    ) -> HashMap<usize, Result<(), String>> {
        let mut results: HashMap<usize, Result<(), String>> = HashMap::new();
        if updates.is_empty() {
            return results;
        }

        let graph_root = kg::default_graph_root(&self.cwd);
        let mut by_graph: HashMap<String, Vec<&FeedbackUpdate>> = HashMap::new();
        for update in updates {
            by_graph
                .entry(update.graph.clone())
                .or_default()
                .push(update);
        }

        for (graph, items) in by_graph {
            let path = match kg::resolve_graph_path(&self.cwd, &graph_root, &graph) {
                Ok(path) => path,
                Err(err) => {
                    let msg = format!("graph not found: {err}");
                    for item in items {
                        results.insert(item.item_index, Err(msg.clone()));
                    }
                    continue;
                }
            };

            let mut graph_file = match kg::GraphFile::load(&path) {
                Ok(graph) => graph,
                Err(err) => {
                    let msg = format!("failed to load graph: {err}");
                    for item in items {
                        results.insert(item.item_index, Err(msg.clone()));
                    }
                    continue;
                }
            };

            let mut changed = false;
            for item in &items {
                match graph_file.node_by_id_mut(&item.node_id) {
                    Some(node) => {
                        node.properties.feedback_score += item.delta;
                        node.properties.feedback_count =
                            node.properties.feedback_count.saturating_add(1);
                        node.properties.feedback_last_ts_ms = Some(item.ts_ms as u64);
                        changed = true;
                        results.insert(item.item_index, Ok(()));
                    }
                    None => {
                        results.insert(
                            item.item_index,
                            Err(format!("node not found: {}", item.node_id)),
                        );
                    }
                }
            }

            if changed {
                if let Err(err) = graph_file.save(&path) {
                    let msg = format!("failed to save graph: {err}");
                    for item in items {
                        if matches!(results.get(&item.item_index), Some(Ok(()))) {
                            results.insert(item.item_index, Err(msg.clone()));
                        }
                    }
                } else {
                    for item in items {
                        if matches!(results.get(&item.item_index), Some(Ok(()))) {
                            kg::append_kg_feedback(
                                &path,
                                &self.user_short_uid,
                                &item.node_id,
                                &item.action,
                            );
                        }
                    }
                }
            }
        }

        results
    }

    fn render_text_content(result: &CallToolResult) -> String {
        let mut out = String::new();
        for item in &result.content {
            if let Some(text) = item.as_text() {
                out.push_str(&text.text);
            }
        }
        out
    }

    fn run_feedback_batch(
        &self,
        lines: Vec<String>,
        mode: &str,
    ) -> Result<FeedbackBatchRun, McpError> {
        if mode != "best_effort" && mode != "strict" {
            return Err(McpError::invalid_params(
                "invalid mode",
                Some(json!({
                    "expected": ["best_effort", "strict"],
                    "got": mode,
                })),
            ));
        }
        let mut entries = Vec::new();
        let mut invalid = Vec::new();

        for line in lines {
            match parse_feedback_line(&line) {
                Some(parsed) => entries.push((line, Some(parsed))),
                None => {
                    invalid.push(line.clone());
                    entries.push((line, None));
                }
            }
        }

        if mode == "strict" && !invalid.is_empty() {
            return Err(McpError::invalid_params(
                "invalid feedback line(s)",
                Some(json!({
                    "count": invalid.len(),
                    "lines": invalid,
                    "expected": "uid=<id> YES|NO|NIL | uid=<id> PICK <n>",
                })),
            ));
        }

        let now_ms = Self::now_ms();
        let mut log_lines = String::new();
        let mut items = Vec::with_capacity(entries.len());
        let mut updates: Vec<FeedbackUpdate> = Vec::new();
        let mut ok = 0usize;

        {
            let mut state = self.feedback_state.lock().map_err(|_| {
                McpError::internal_error(
                    "kg feedback state lock poisoned",
                    Some(json!({ "reason": "previous tool panicked" })),
                )
            })?;
            cleanup_old_finds(&mut state.finds, now_ms, 10 * 60 * 1000);

            for (line, parsed) in entries {
                let source = if line.contains("passive=1") {
                    "passive"
                } else {
                    "active"
                };
                match parsed {
                    Some(parsed) => {
                        let (graph, queries, selected) =
                            if let Some(ctx) = state.finds.get(&parsed.uid) {
                                let selected = match (parsed.action.as_str(), parsed.pick) {
                                    ("PICK", Some(n)) if n >= 1 && n <= ctx.candidate_ids.len() => {
                                        Some(ctx.candidate_ids[n - 1].clone())
                                    }
                                    ("YES", None) if ctx.candidate_ids.len() == 1 => {
                                        Some(ctx.candidate_ids[0].clone())
                                    }
                                    _ => None,
                                };
                                (Some(ctx.graph.clone()), Some(ctx.queries.clone()), selected)
                            } else {
                                (None, None, None)
                            };

                        let graph_s = graph.clone().unwrap_or_else(|| "-".to_owned());
                        let queries_v = queries.clone().unwrap_or_default();
                        let selected_s = selected.clone().unwrap_or_else(|| "-".to_owned());
                        let uid = parsed.uid.clone();
                        let action = parsed.action.clone();
                        let delta = feedback_delta(parsed.action.as_str());
                        let pick = parsed.pick;

                        update_feedback_stats(&mut state, &queries_v, &action);

                        let log_line = format!(
                            "ts_ms={}\tuid={}\taction={}\tpick={}\tselected={}\tgraph={}\tqueries={}\tsource={}\n",
                            now_ms,
                            uid,
                            action,
                            parsed
                                .pick
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "-".to_owned()),
                            selected_s,
                            graph_s,
                            queries_v.join(" | ").replace('\t', " "),
                            source,
                        );
                        log_lines.push_str(&log_line);

                        ok += 1;
                        items.push(json!({
                            "line": line,
                            "status": "ok",
                            "uid": uid,
                            "action": action.clone(),
                            "pick": pick,
                            "selected": selected_s,
                            "graph": graph_s,
                            "queries": queries_v,
                            "source": source,
                        }));

                        if let (Some(graph), Some(selected), Some(delta)) = (graph, selected, delta)
                        {
                            if !graph.is_empty() && graph != "-" {
                                let index = items.len().saturating_sub(1);
                                updates.push(FeedbackUpdate {
                                    item_index: index,
                                    graph,
                                    node_id: selected,
                                    action,
                                    delta,
                                    ts_ms: now_ms,
                                });
                            }
                        }
                    }
                    None => {
                        items.push(json!({
                            "line": line,
                            "status": "error",
                            "error": "invalid feedback line",
                        }));
                    }
                }
            }
        }

        if !log_lines.is_empty() {
            self.append_feedback_log(&log_lines);
        }

        if !updates.is_empty() {
            let update_results = self.apply_feedback_updates(&updates);
            for update in updates {
                if let Some(item) = items.get_mut(update.item_index) {
                    if let Some(obj) = item.as_object_mut() {
                        match update_results.get(&update.item_index) {
                            Some(Ok(())) => {
                                obj.insert("graph_update".to_owned(), json!("ok"));
                            }
                            Some(Err(msg)) => {
                                obj.insert("graph_update".to_owned(), json!("error"));
                                obj.insert("graph_error".to_owned(), json!(msg));
                            }
                            None => {}
                        }
                    }
                }
            }
        }

        let failed = items
            .iter()
            .filter(|v| v.get("status") == Some(&json!("error")))
            .count();

        Ok(FeedbackBatchRun { ok, failed, items })
    }

    fn handle_node_find(&self, args: NodeFindArgs) -> Result<CallToolResult, McpError> {
        let graph = args.graph.clone();
        let queries = args.queries.clone();
        let skip_feedback = args.skip_feedback;
        let mut cmd = vec![args.graph, "node".to_owned(), "find".to_owned()];
        cmd.extend(args.queries);
        if let Some(limit) = args.limit {
            cmd.push("--limit".to_owned());
            cmd.push(limit.to_string());
        }
        if let Some(output_size) = args.output_size {
            cmd.push("--output-size".to_owned());
            cmd.push(output_size.to_string());
        }
        if let Some(mode) = args.mode {
            cmd.push("--mode".to_owned());
            cmd.push(mode);
        }
        if args.full {
            cmd.push("--full".to_owned());
        }

        let mut os_args = Vec::with_capacity(cmd.len() + 1);
        os_args.push(OsString::from("kg"));
        os_args.extend(cmd.into_iter().map(OsString::from));
        let rendered = self.run_kg(os_args, "kg_node_find", "node find", args.debug)?;

        let total = parse_find_total_results(&rendered).unwrap_or(0);
        let mut candidate_ids = parse_find_candidate_ids(&rendered);
        if candidate_ids.len() > 25 {
            candidate_ids.truncate(25);
        }

        if skip_feedback {
            let mut output = rendered;
            if !output.ends_with('\n') {
                output.push('\n');
            }
            return Ok(CallToolResult {
                content: vec![Content::text(output)],
                structured_content: Some(json!({
                    "results": {
                        "total": total,
                        "candidates": candidate_ids,
                    }
                })),
                is_error: Some(false),
                meta: None,
            });
        }

        let uid = self.next_uid()?;
        {
            let mut state = self.feedback_state.lock().map_err(|_| {
                McpError::internal_error(
                    "kg feedback state lock poisoned",
                    Some(json!({ "reason": "previous tool panicked" })),
                )
            })?;
            cleanup_old_finds(&mut state.finds, Self::now_ms(), 10 * 60 * 1000);
            state.finds.insert(
                uid.clone(),
                FindContext {
                    created_at_ms: Self::now_ms(),
                    graph,
                    queries: queries.clone(),
                    candidate_ids: candidate_ids.clone(),
                },
            );
        }

        let (nudge, feedback_mode, pick_max) = if total == 0 {
            (
                format!("NUDGE: No matches. Reply: kg_feedback_batch lines=[\"uid={uid} NIL\"]"),
                "miss".to_owned(),
                None,
            )
        } else if total == 1 {
            (
                format!(
                    "NUDGE: Useful? Reply: kg_feedback_batch lines=[\"uid={uid} YES\"] or [\"uid={uid} NO\"]"
                ),
                "yesno".to_owned(),
                Some(1usize),
            )
        } else {
            let pick_max = if !candidate_ids.is_empty() {
                candidate_ids.len()
            } else {
                total
            };
            (
                format!(
                    "NUDGE: Which one was useful? Reply: kg_feedback_batch lines=[\"uid={uid} PICK <1..{}>\"] or [\"uid={uid} NIL\"]",
                    pick_max
                ),
                "pick".to_owned(),
                Some(pick_max),
            )
        };

        let adaptive_percent = self.adaptive_nudge_percent(&queries, total)?;
        let show_nudge = should_emit_nudge(adaptive_percent, &uid);
        let mut output = String::new();
        if show_nudge {
            output.push_str(&nudge);
            output.push('\n');
        }
        output.push_str(&rendered);
        if !output.ends_with('\n') {
            output.push('\n');
        }

        let mut requires_feedback = match feedback_mode.as_str() {
            "miss" => json!({
                "required": true,
                "uid": uid,
                "mode": "miss",
                "allow_nil": true,
            }),
            "yesno" => json!({
                "required": true,
                "uid": uid,
                "mode": "yesno",
                "options": 2,
            }),
            _ => json!({
                "required": true,
                "uid": uid,
                "mode": "pick",
                "options": pick_max.unwrap_or(total),
                "allow_nil": true,
            }),
        };
        if show_nudge {
            requires_feedback["nudge"] = json!(nudge);
        }
        requires_feedback["sample_percent"] = json!(adaptive_percent);

        Ok(CallToolResult {
            content: vec![Content::text(output)],
            structured_content: Some(json!({
                "requires_feedback": requires_feedback,
                "results": {
                    "total": total,
                    "candidates": candidate_ids.len(),
                }
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    fn handle_node_get(&self, args: NodeGetArgs) -> Result<CallToolResult, McpError> {
        let graph = args.graph.clone();
        let node_id = args.id.clone();
        let mut cmd = vec![args.graph, "node".to_owned(), "get".to_owned(), args.id];
        if let Some(output_size) = args.output_size {
            cmd.push("--output-size".to_owned());
            cmd.push(output_size.to_string());
        }
        if args.full {
            cmd.push("--full".to_owned());
        }

        let mut os_args = Vec::with_capacity(cmd.len() + 1);
        os_args.push(OsString::from("kg"));
        os_args.extend(cmd.into_iter().map(OsString::from));
        let rendered = self.run_kg(os_args, "kg_node_get", "node get", args.debug)?;

        let uid = self.next_uid()?;
        {
            let mut state = self.feedback_state.lock().map_err(|_| {
                McpError::internal_error(
                    "kg feedback state lock poisoned",
                    Some(json!({ "reason": "previous tool panicked" })),
                )
            })?;
            cleanup_old_finds(&mut state.finds, Self::now_ms(), 10 * 60 * 1000);
            state.finds.insert(
                uid.clone(),
                FindContext {
                    created_at_ms: Self::now_ms(),
                    graph,
                    queries: vec![format!("node_get:{node_id}")],
                    candidate_ids: vec![node_id.clone()],
                },
            );
        }

        let nudge = format!(
            "NUDGE: Useful? Reply: kg_feedback_batch lines=[\"uid={uid} YES\"] or [\"uid={uid} NO\"]"
        );
        let adaptive_percent = self.adaptive_nudge_percent(&[format!("node_get:{node_id}")], 1)?;
        let show_nudge = should_emit_nudge(adaptive_percent, &uid);
        let mut output = String::new();
        if show_nudge {
            output.push_str(&nudge);
            output.push('\n');
        }
        output.push_str(&rendered);
        if !output.ends_with('\n') {
            output.push('\n');
        }

        let mut requires_feedback = json!({
            "required": true,
            "uid": uid,
            "mode": "yesno",
            "options": 2,
        });
        if show_nudge {
            requires_feedback["nudge"] = json!(nudge);
        }
        requires_feedback["sample_percent"] = json!(adaptive_percent);

        Ok(CallToolResult {
            content: vec![Content::text(output)],
            structured_content: Some(json!({
                "requires_feedback": requires_feedback,
                "results": {
                    "total": 1,
                    "candidates": 1,
                }
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    #[tool(
        name = "kg_command",
        description = "Run a single kg command by passing CLI arguments (without the binary name). Prefer `kg` when you have multiple actions."
    )]
    fn kg_command(
        &self,
        Parameters(args): Parameters<KgCommandArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_kg_for("kg_command", args.args, args.debug)
    }

    #[tool(
        name = "kg",
        description = "Run one or more kg commands separated by ';' or newlines. Supports batch workflows that mix `find`, `get`, `kql`, and feedback lines like `uid=abc123 YES`. Example: `graph fridge node find \"smart fridge\" --output-size 1200; uid=aa01bc YES; graph fridge node get concept:refrigerator --full`."
    )]
    fn kg(&self, Parameters(args): Parameters<KgScriptArgs>) -> Result<CallToolResult, McpError> {
        let request_debug = args.debug;
        let mode = self.parse_mode(args.mode)?;
        let commands = split_script(&args.script);
        let mut output = String::new();
        let mut steps: Vec<serde_json::Value> = Vec::new();
        let mut requires_feedback: Vec<serde_json::Value> = Vec::new();
        let mut feedback_buffer: Vec<String> = Vec::new();
        let mut pending_find_feedback: HashMap<String, PendingFindFeedback> = HashMap::new();
        let mut auto_resolved_feedback_uids: HashSet<String> = HashSet::new();

        let flush_feedback = |this: &KgMcpServer,
                              steps: &mut Vec<serde_json::Value>,
                              output: &mut String,
                              feedback_buffer: &mut Vec<String>|
         -> Result<(), McpError> {
            if feedback_buffer.is_empty() {
                return Ok(());
            }
            let lines = std::mem::take(feedback_buffer);
            match this.run_feedback_batch(lines.clone(), &mode) {
                Ok(result) => {
                    let content = format!("OK (ok={} failed={})\n", result.ok, result.failed);
                    output.push_str("> feedback\n");
                    output.push_str(&content);
                    steps.push(json!({
                        "cmd": "feedback",
                        "kind": "feedback",
                        "ok": result.failed == 0,
                        "stdout": content,
                        "items": result.items,
                    }));
                    Ok(())
                }
                Err(err) => {
                    if mode == "strict" {
                        Err(err)
                    } else {
                        let msg = err.to_string();
                        output.push_str("> feedback\n");
                        output.push_str(&format!("ERROR: {msg}\n"));
                        steps.push(json!({
                            "cmd": "feedback",
                            "kind": "feedback",
                            "ok": false,
                            "error": msg,
                        }));
                        Ok(())
                    }
                }
            }
        };

        for raw in commands {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                continue;
            }

            if parse_feedback_line(trimmed).is_some() {
                feedback_buffer.push(trimmed.to_owned());
                continue;
            }

            flush_feedback(self, &mut steps, &mut output, &mut feedback_buffer)?;

            let tokens = match tokenize_command(trimmed) {
                Ok(tokens) => tokens,
                Err(err) => {
                    if mode == "strict" {
                        return Err(McpError::invalid_params(
                            "invalid command",
                            Some(json!({ "command": trimmed, "error": err })),
                        ));
                    }
                    output.push_str("> ");
                    output.push_str(trimmed);
                    output.push('\n');
                    output.push_str(&format!("ERROR: {err}\n"));
                    steps.push(json!({
                        "cmd": trimmed,
                        "kind": "kg",
                        "ok": false,
                        "error": err,
                    }));
                    continue;
                }
            };

            if tokens.is_empty() {
                continue;
            }

            let mut args = tokens;
            while args.first().map(|v| v == "kg").unwrap_or(false) {
                args.remove(0);
            }
            if args.first().map(|v| v == "graph").unwrap_or(false) {
                args.remove(0);
            }
            if args.is_empty() {
                continue;
            }

            let mut handled = false;

            if let Some(find_args) = parse_node_find_args(&args) {
                let result = match find_args {
                    Ok(mut find_args) => {
                        find_args.debug = request_debug;
                        self.handle_node_find(find_args)
                    }
                    Err(err) => Err(McpError::invalid_params(
                        "invalid node find command",
                        Some(json!({ "command": trimmed, "error": err })),
                    )),
                };

                match result {
                    Ok(tool_result) => {
                        let stdout = Self::render_text_content(&tool_result);
                        output.push_str("> ");
                        output.push_str(trimmed);
                        output.push('\n');
                        output.push_str(&stdout);
                        steps.push(json!({
                            "cmd": trimmed,
                            "kind": "kg",
                            "ok": true,
                            "stdout": stdout,
                            "requires_feedback": tool_result
                                .structured_content
                                .as_ref()
                                .and_then(|v| v.get("requires_feedback"))
                                .cloned(),
                        }));
                        if let Some(req) = tool_result
                            .structured_content
                            .as_ref()
                            .and_then(|v| v.get("requires_feedback"))
                            .cloned()
                        {
                            if req
                                .get("mode")
                                .and_then(|v| v.as_str())
                                .is_some_and(|mode| mode == "pick")
                            {
                                if let Some(uid) = req
                                    .get("uid")
                                    .and_then(|v| v.as_str())
                                    .map(|v| v.to_owned())
                                {
                                    let find_candidate_ids = parse_find_candidate_ids(&stdout);
                                    if !find_candidate_ids.is_empty() {
                                        let graph_name = args.first().cloned().unwrap_or_default();
                                        pending_find_feedback.insert(
                                            graph_name,
                                            PendingFindFeedback {
                                                uid,
                                                candidate_ids: find_candidate_ids,
                                            },
                                        );
                                    }
                                }
                            }
                            requires_feedback.push(req);
                        }
                    }
                    Err(err) => {
                        if mode == "strict" {
                            return Err(err);
                        }
                        let msg = err.to_string();
                        output.push_str("> ");
                        output.push_str(trimmed);
                        output.push('\n');
                        output.push_str(&format!("ERROR: {msg}\n"));
                        steps.push(json!({
                            "cmd": trimmed,
                            "kind": "kg",
                            "ok": false,
                            "error": msg,
                        }));
                    }
                }
                handled = true;
            }

            if !handled {
                if let Some(get_args) = parse_node_get_args(&args) {
                    let result = match get_args {
                        Ok(mut get_args) => {
                            if let Some(pending) = pending_find_feedback.get(&get_args.graph) {
                                if let Some(index) = pending
                                    .candidate_ids
                                    .iter()
                                    .position(|candidate_id| candidate_id == &get_args.id)
                                {
                                    let passive_line =
                                        format!("uid={} PICK {} passive=1", pending.uid, index + 1);
                                    feedback_buffer.push(passive_line.clone());
                                    auto_resolved_feedback_uids.insert(pending.uid.clone());
                                    steps.push(json!({
                                        "cmd": trimmed,
                                        "kind": "passive_feedback",
                                        "ok": true,
                                        "source": "node_get_after_find",
                                        "line": passive_line,
                                    }));
                                }
                            }
                            pending_find_feedback.remove(&get_args.graph);
                            flush_feedback(self, &mut steps, &mut output, &mut feedback_buffer)?;
                            get_args.debug = request_debug;
                            self.handle_node_get(get_args)
                        }
                        Err(err) => Err(McpError::invalid_params(
                            "invalid node get command",
                            Some(json!({ "command": trimmed, "error": err })),
                        )),
                    };

                    match result {
                        Ok(tool_result) => {
                            let stdout = Self::render_text_content(&tool_result);
                            output.push_str("> ");
                            output.push_str(trimmed);
                            output.push('\n');
                            output.push_str(&stdout);
                            steps.push(json!({
                                "cmd": trimmed,
                                "kind": "kg",
                                "ok": true,
                                "stdout": stdout,
                                "requires_feedback": tool_result
                                    .structured_content
                                    .as_ref()
                                    .and_then(|v| v.get("requires_feedback"))
                                    .cloned(),
                            }));
                            if let Some(req) = tool_result
                                .structured_content
                                .as_ref()
                                .and_then(|v| v.get("requires_feedback"))
                                .cloned()
                            {
                                requires_feedback.push(req);
                            }
                        }
                        Err(err) => {
                            if mode == "strict" {
                                return Err(err);
                            }
                            let msg = err.to_string();
                            output.push_str("> ");
                            output.push_str(trimmed);
                            output.push('\n');
                            output.push_str(&format!("ERROR: {msg}\n"));
                            steps.push(json!({
                                "cmd": trimmed,
                                "kind": "kg",
                                "ok": false,
                                "error": msg,
                            }));
                        }
                    }
                    handled = true;
                }
            }

            if handled {
                continue;
            }

            match self.execute_kg_for("kg", args.clone(), request_debug) {
                Ok(tool_result) => {
                    let stdout = Self::render_text_content(&tool_result);
                    output.push_str("> ");
                    output.push_str(trimmed);
                    output.push('\n');
                    output.push_str(&stdout);
                    steps.push(json!({
                        "cmd": trimmed,
                        "kind": "kg",
                        "ok": true,
                        "stdout": stdout,
                    }));
                }
                Err(err) => {
                    if mode == "strict" {
                        return Err(err);
                    }
                    let msg = err.to_string();
                    output.push_str("> ");
                    output.push_str(trimmed);
                    output.push('\n');
                    output.push_str(&format!("ERROR: {msg}\n"));
                    steps.push(json!({
                        "cmd": trimmed,
                        "kind": "kg",
                        "ok": false,
                        "error": msg,
                    }));
                }
            }
        }

        flush_feedback(self, &mut steps, &mut output, &mut feedback_buffer)?;

        if !auto_resolved_feedback_uids.is_empty() {
            requires_feedback.retain(|item| {
                item.get("uid")
                    .and_then(|v| v.as_str())
                    .map(|uid| !auto_resolved_feedback_uids.contains(uid))
                    .unwrap_or(true)
            });
        }

        Ok(CallToolResult {
            content: vec![Content::text(output)],
            structured_content: Some(json!({
                "steps": steps,
                "requires_feedback": requires_feedback,
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    #[tool(name = "kg_create_graph", description = "Create a new graph")]
    fn kg_create_graph(
        &self,
        Parameters(args): Parameters<CreateGraphArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_kg(vec!["create".to_owned(), args.graph_name])
    }

    #[tool(
        name = "kg_node_find",
        description = "Find nodes by one or more search queries. Supports `limit`, `mode`, `full`, and `output_size`. Feature nodes are always included. Use skip_feedback=true for lookup-only queries; otherwise check structured_content.requires_feedback and respond via kg_feedback_batch before continuing. Prefer `kg` for batched search workflows."
    )]
    fn kg_node_find(
        &self,
        Parameters(args): Parameters<NodeFindArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.handle_node_find(args)
    }

    #[tool(
        name = "kg_feedback_batch",
        description = "Record multiple feedback events at once (best_effort or strict). Prefer `kg` when mixing feedback with other commands."
    )]
    fn kg_feedback_batch(
        &self,
        Parameters(args): Parameters<FeedbackBatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mode = args.mode.unwrap_or_else(|| "best_effort".to_owned());
        if mode != "best_effort" && mode != "strict" {
            return Err(McpError::invalid_params(
                "invalid mode",
                Some(json!({
                    "expected": ["best_effort", "strict"],
                    "got": mode,
                })),
            ));
        }

        let result = self.run_feedback_batch(args.lines, &mode)?;
        let content = format!("OK (ok={} failed={})\n", result.ok, result.failed);

        Ok(CallToolResult {
            content: vec![Content::text(content)],
            structured_content: Some(json!({
                "ok": result.ok,
                "failed": result.failed,
                "items": result.items,
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    #[tool(
        name = "kg_node_get",
        description = "Get a node by its id. Supports `full` and `output_size`. Feature nodes are always included. Always check structured_content.requires_feedback and respond via kg_feedback_batch before continuing. Prefer `kg` when batching get/search commands."
    )]
    fn kg_node_get(
        &self,
        Parameters(args): Parameters<NodeGetArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.handle_node_get(args)
    }

    #[tool(
        name = "kg_node_add_batch",
        description = "Add multiple nodes to a graph (atomic or best_effort) with optional on_conflict=skip. Prefer `kg` when mixing writes with other commands."
    )]
    fn kg_node_add_batch(
        &self,
        Parameters(args): Parameters<NodeAddBatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let graph = args.graph.clone();
        let mode = args.mode.unwrap_or_else(|| "atomic".to_owned());
        if mode != "atomic" && mode != "best_effort" {
            return Err(McpError::invalid_params(
                "invalid mode",
                Some(json!({
                    "expected": ["atomic", "best_effort"],
                    "got": mode,
                })),
            ));
        }
        let on_conflict = args.on_conflict.unwrap_or_else(|| "error".to_owned());
        if on_conflict != "error" && on_conflict != "skip" {
            return Err(McpError::invalid_params(
                "invalid on_conflict",
                Some(json!({
                    "expected": ["error", "skip"],
                    "got": on_conflict,
                })),
            ));
        }

        let graph_root = kg::default_graph_root(&self.cwd);
        let path = kg::resolve_graph_path(&self.cwd, &graph_root, &graph).map_err(|err| {
            McpError::invalid_params(
                "graph not found",
                Some(json!({ "graph": graph.clone(), "error": err.to_string() })),
            )
        })?;

        let mut graph_file = kg::GraphFile::load(&path).map_err(|err| {
            let detail = kg::format_error_chain(&err);
            McpError::internal_error(
                format!("failed to load graph: {detail}"),
                Some(json!({ "path": path.display().to_string(), "error": detail })),
            )
        })?;

        let mut created: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut failed: Vec<serde_json::Value> = Vec::new();

        for item in args.nodes {
            let id = match kg::canonicalize_node_id_for_type(&item.id, &item.node_type) {
                Ok(value) => value,
                Err(err) => {
                    if mode == "atomic" {
                        return Err(McpError::invalid_params(
                            "invalid node",
                            Some(json!({ "id": item.id, "error": err })),
                        ));
                    }
                    failed.push(json!({ "id": item.id, "error": err }));
                    continue;
                }
            };

            if !kg::VALID_TYPES.contains(&item.node_type.as_str()) {
                let err = format!(
                    "invalid node_type '{}': expected one of {:?}",
                    item.node_type,
                    kg::VALID_TYPES
                );
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }

            if item.sources.is_empty() {
                let err = "at least one source is required";
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }

            let description = item.description.unwrap_or_default();
            let domain_area = item.domain_area.unwrap_or_default();
            let provenance = item.provenance.unwrap_or_default();
            let created_at = item.created_at.unwrap_or_default();
            let Some(confidence) = item.confidence else {
                let err = "confidence is required and must be in range 0..1";
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            };
            let Some(importance) = item.importance else {
                let err = "importance is required and must be in range 0..1";
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            };

            if description.trim().is_empty()
                || domain_area.trim().is_empty()
                || provenance.trim().is_empty()
                || created_at.trim().is_empty()
            {
                let err = "description, domain_area, provenance and created_at are required";
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }
            if !VALID_PROVENANCE_CODES.contains(&provenance.as_str()) {
                let err = format!(
                    "provenance must be one of: {}",
                    VALID_PROVENANCE_CODES.join(", ")
                );
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }
            if !(0.0..=1.0).contains(&confidence) {
                let err = format!("confidence out of range: {confidence}");
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }
            if !(0.0..=1.0).contains(&importance) {
                let err = format!("importance out of range: {importance}");
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }
            if !is_valid_iso_utc_timestamp(created_at.trim()) {
                let err = "created_at must use UTC format YYYY-MM-DDTHH:MM:SSZ";
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }
            let mut source_error = None;
            for source in &item.sources {
                if let Err(err) = validate_source_reference(source) {
                    source_error = Some(format!("invalid source '{}': {err}", source));
                    break;
                }
            }
            if let Some(err) = source_error {
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "invalid node",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }

            if graph_file.node_by_id(&id).is_some() {
                if on_conflict == "skip" {
                    skipped.push(id);
                    continue;
                }
                let err = format!("node already exists: {id}");
                if mode == "atomic" {
                    return Err(McpError::invalid_params(
                        "conflict",
                        Some(json!({ "id": id, "error": err })),
                    ));
                }
                failed.push(json!({ "id": id, "error": err }));
                continue;
            }

            graph_file.nodes.push(kg::Node {
                id: id.clone(),
                r#type: item.node_type,
                name: item.name,
                properties: kg::NodeProperties {
                    description,
                    domain_area,
                    provenance,
                    confidence: Some(confidence),
                    created_at,
                    importance,
                    key_facts: item.facts,
                    alias: item.aliases,
                    ..kg::NodeProperties::default()
                },
                source_files: item.sources,
            });
            created.push(id);
        }

        // Persist once.
        if !created.is_empty() {
            graph_file.refresh_counts();
            graph_file.save(&path).map_err(|err| {
                McpError::internal_error(
                    "failed to save graph",
                    Some(json!({ "path": path.display().to_string(), "error": err.to_string() })),
                )
            })?;
        }

        if mode == "atomic" && !failed.is_empty() {
            return Err(McpError::internal_error(
                "atomic write invariant violated",
                Some(json!({ "failed": failed })),
            ));
        }

        Ok(CallToolResult {
            content: vec![Content::text(format!(
                "OK (created={} skipped={} failed={})\n",
                created.len(),
                skipped.len(),
                failed.len()
            ))],
            structured_content: Some(json!({
                "graph": graph,
                "path": path.display().to_string(),
                "created": created,
                "skipped": skipped,
                "failed": failed,
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    #[tool(
        name = "kg_node_add",
        description = "Add a new node to a graph. Valid node_type: Concept, Process, DataStore, Interface, Rule, Feature, Decision, Convention, Note, Bug. ID must match <type_code>:snake_case (legacy <prefix>:snake_case also accepted). Prefer `kg` when combining multiple actions."
    )]
    fn kg_node_add(
        &self,
        Parameters(args): Parameters<NodeAddArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !kg::VALID_TYPES.contains(&args.node_type.as_str()) {
            return Err(McpError::invalid_params(
                "invalid node_type",
                Some(json!({
                    "expected": kg::VALID_TYPES,
                    "got": args.node_type,
                })),
            ));
        }

        let mut cmd = vec![
            args.graph,
            "node".to_owned(),
            "add".to_owned(),
            args.id,
            "--type".to_owned(),
            args.node_type,
            "--name".to_owned(),
            args.name,
        ];
        if let Some(description) = args.description {
            cmd.push("--description".to_owned());
            cmd.push(description);
        }
        if let Some(domain_area) = args.domain_area {
            cmd.push("--domain-area".to_owned());
            cmd.push(domain_area);
        }
        if let Some(provenance) = args.provenance {
            cmd.push("--provenance".to_owned());
            cmd.push(provenance);
        }
        if let Some(confidence) = args.confidence {
            cmd.push("--confidence".to_owned());
            cmd.push(confidence.to_string());
        }
        if let Some(created_at) = args.created_at {
            cmd.push("--created-at".to_owned());
            cmd.push(created_at);
        }
        if let Some(importance) = args.importance {
            cmd.push("--importance".to_owned());
            cmd.push(importance.to_string());
        }
        for fact in args.facts {
            cmd.push("--fact".to_owned());
            cmd.push(fact);
        }
        for alias in args.aliases {
            cmd.push("--alias".to_owned());
            cmd.push(alias);
        }
        for source in args.sources {
            cmd.push("--source".to_owned());
            cmd.push(source);
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_node_modify",
        description = "Modify an existing node. Prefer `kg` when combining multiple actions."
    )]
    fn kg_node_modify(
        &self,
        Parameters(args): Parameters<NodeModifyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph, "node".to_owned(), "modify".to_owned(), args.id];
        if let Some(node_type) = args.node_type {
            cmd.push("--type".to_owned());
            cmd.push(node_type);
        }
        if let Some(name) = args.name {
            cmd.push("--name".to_owned());
            cmd.push(name);
        }
        if let Some(description) = args.description {
            cmd.push("--description".to_owned());
            cmd.push(description);
        }
        if let Some(domain_area) = args.domain_area {
            cmd.push("--domain-area".to_owned());
            cmd.push(domain_area);
        }
        if let Some(provenance) = args.provenance {
            cmd.push("--provenance".to_owned());
            cmd.push(provenance);
        }
        if let Some(confidence) = args.confidence {
            cmd.push("--confidence".to_owned());
            cmd.push(confidence.to_string());
        }
        if let Some(created_at) = args.created_at {
            cmd.push("--created-at".to_owned());
            cmd.push(created_at);
        }
        if let Some(importance) = args.importance {
            cmd.push("--importance".to_owned());
            cmd.push(importance.to_string());
        }
        for fact in args.facts {
            cmd.push("--fact".to_owned());
            cmd.push(fact);
        }
        for alias in args.aliases {
            cmd.push("--alias".to_owned());
            cmd.push(alias);
        }
        for source in args.sources {
            cmd.push("--source".to_owned());
            cmd.push(source);
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_node_remove",
        description = "Remove a node and its incident edges. Prefer `kg` when combining multiple actions."
    )]
    fn kg_node_remove(
        &self,
        Parameters(args): Parameters<NodeRemoveArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_kg(vec![
            args.graph,
            "node".to_owned(),
            "remove".to_owned(),
            args.id,
        ])
    }

    #[tool(
        name = "kg_edge_add",
        description = "Add an edge between two nodes. Valid relations: HAS, STORED_IN, TRIGGERS, CREATED_BY, AFFECTED_BY, AVAILABLE_IN, DOCUMENTED_IN, DEPENDS_ON, TRANSITIONS, DECIDED_BY, GOVERNED_BY, USES, READS_FROM. Prefer `kg` when combining multiple actions."
    )]
    fn kg_edge_add(
        &self,
        Parameters(args): Parameters<EdgeAddArgs>,
    ) -> Result<CallToolResult, McpError> {
        let graph_file = self.load_graph_file(&args.graph)?;
        if let Err(error) = self.preflight_edge(
            &graph_file,
            &args.source_id,
            &args.relation,
            &args.target_id,
        ) {
            return Err(McpError::invalid_params(error.message, Some(error.data)));
        }

        let mut cmd = vec![
            args.graph,
            "edge".to_owned(),
            "add".to_owned(),
            args.source_id,
            args.relation,
            args.target_id,
        ];
        if let Some(detail) = args.detail {
            cmd.push("--detail".to_owned());
            cmd.push(detail);
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_edge_add_batch",
        description = "Add multiple edges to a graph (atomic or best_effort) and optionally validate with dry_run=true before writing. Prefer `kg` when combining multiple actions."
    )]
    fn kg_edge_add_batch(
        &self,
        Parameters(args): Parameters<EdgeAddBatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let graph = args.graph.clone();
        let mode = args.mode.unwrap_or_else(|| "atomic".to_owned());
        if mode != "atomic" && mode != "best_effort" {
            return Err(McpError::invalid_params(
                "invalid mode",
                Some(json!({
                    "expected": ["atomic", "best_effort"],
                    "got": mode,
                })),
            ));
        }

        let dry_run = args.dry_run;
        let mut working_graph = self.load_graph_file(&graph)?;

        let mut results = Vec::new();

        for (i, edge) in args.edges.iter().enumerate() {
            if let Err(error) = self.preflight_edge(
                &working_graph,
                &edge.source_id,
                &edge.relation,
                &edge.target_id,
            ) {
                if mode == "atomic" && !dry_run {
                    return Err(McpError::invalid_params(error.message, Some(error.data)));
                }
                results.push(json!({
                    "index": i,
                    "source_id": edge.source_id,
                    "relation": edge.relation,
                    "target_id": edge.target_id,
                    "ok": false,
                    "error": error.message,
                    "dry_run": dry_run,
                }));
                continue;
            }

            if dry_run {
                Self::apply_edge_to_graph(&mut working_graph, edge);
                results.push(json!({
                    "index": i,
                    "source_id": edge.source_id,
                    "relation": edge.relation,
                    "target_id": edge.target_id,
                    "ok": true,
                    "dry_run": true,
                }));
                continue;
            }

            let mut cmd = vec![
                graph.clone(),
                "edge".to_owned(),
                "add".to_owned(),
                edge.source_id.clone(),
                edge.relation.clone(),
                edge.target_id.clone(),
            ];
            if let Some(detail) = &edge.detail {
                cmd.push("--detail".to_owned());
                cmd.push(detail.clone());
            }
            let result = self.execute_kg(cmd);

            match result {
                Ok(_) => {
                    Self::apply_edge_to_graph(&mut working_graph, edge);
                    results.push(json!({
                        "index": i,
                        "source_id": edge.source_id,
                        "relation": edge.relation,
                        "target_id": edge.target_id,
                        "ok": true,
                        "dry_run": false,
                    }));
                }
                Err(e) => {
                    if mode == "atomic" {
                        return Err(e);
                    }
                    results.push(json!({
                        "index": i,
                        "source_id": edge.source_id,
                        "relation": edge.relation,
                        "target_id": edge.target_id,
                        "ok": false,
                        "error": e.message,
                        "dry_run": false,
                    }));
                }
            }
        }

        let ok_count = results.iter().filter(|r| r["ok"] == true).count();
        let failed_count = results.len() - ok_count;
        let summary = self.format_edge_batch_output(dry_run, ok_count, failed_count, &results);

        Ok(CallToolResult {
            content: vec![Content::text(summary)],
            structured_content: Some(json!({
                "added": if dry_run { 0 } else { ok_count },
                "would_add": if dry_run { ok_count } else { 0 },
                "failed": failed_count,
                "dry_run": dry_run,
                "results": results,
            })),
            is_error: Some(failed_count > 0),
            meta: None,
        })
    }

    #[tool(
        name = "kg_edge_remove",
        description = "Remove an edge between two nodes. Prefer `kg` when combining multiple actions."
    )]
    fn kg_edge_remove(
        &self,
        Parameters(args): Parameters<EdgeRemoveArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_kg(vec![
            args.graph,
            "edge".to_owned(),
            "remove".to_owned(),
            args.source_id,
            args.relation,
            args.target_id,
        ])
    }

    #[tool(
        name = "kg_schema",
        description = "Get graph schema: valid node types, relations, ID prefixes, and edge rules. Use this to understand the data model before adding nodes or edges."
    )]
    fn kg_schema(
        &self,
        Parameters(_args): Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let type_to_prefix: HashMap<&str, &str> = kg::TYPE_TO_PREFIX.iter().copied().collect();
        let edge_rules: Vec<_> = kg::EDGE_TYPE_RULES
            .iter()
            .map(|(rel, src, tgt)| {
                json!({
                    "relation": rel,
                    "valid_source_types": src,
                    "valid_target_types": tgt,
                })
            })
            .collect();

        let schema_text = format!(
            "## Valid Node Types\n{}\n\n## Valid Relations\n{}\n\n## Type to ID Prefix\n{}\n\n## Edge Rules\n{}",
            kg::VALID_TYPES.join(", "),
            kg::VALID_RELATIONS.join(", "),
            type_to_prefix
                .iter()
                .map(|(t, p)| format!("{} -> {}", t, p))
                .collect::<Vec<_>>()
                .join("\n"),
            edge_rules
                .iter()
                .map(|r| format!(
                    "{}: {} -> {}",
                    r["relation"], r["valid_source_types"], r["valid_target_types"]
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );

        Ok(CallToolResult {
            content: vec![Content::text(schema_text)],
            structured_content: Some(json!({
                "valid_node_types": kg::VALID_TYPES,
                "valid_relations": kg::VALID_RELATIONS,
                "type_to_prefix": type_to_prefix,
                "edge_rules": edge_rules,
            })),
            is_error: Some(false),
            meta: None,
        })
    }

    #[tool(
        name = "kg_stats",
        description = "Show graph structure and usage statistics. Prefer `kg` when issuing multiple commands."
    )]
    fn kg_stats(
        &self,
        Parameters(args): Parameters<GraphStatsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph, "stats".to_owned()];
        if args.include_features {
            cmd.push("--include-features".to_owned());
        }
        if args.by_type {
            cmd.push("--by-type".to_owned());
        }
        if args.by_relation {
            cmd.push("--by-relation".to_owned());
        }
        if args.show_sources {
            cmd.push("--show-sources".to_owned());
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_check",
        description = "[DEPRECATED: use kg script] Run integrity validation checks. Prefer `kg <graph> check`."
    )]
    fn kg_check(
        &self,
        Parameters(args): Parameters<GraphValidateArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph.clone(), "check".to_owned()];
        append_validation_flags(&mut cmd, &args);
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_audit",
        description = "[DEPRECATED: use kg script] Run deep audit validation checks. Prefer `kg <graph> audit`."
    )]
    fn kg_audit(
        &self,
        Parameters(args): Parameters<GraphValidateArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph.clone(), "audit".to_owned()];
        append_validation_flags(&mut cmd, &args);
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_quality",
        description = "[DEPRECATED: use kg script] Run quality subcommands such as missing-facts or duplicates. Prefer `kg <graph> quality <command>`."
    )]
    fn kg_quality(
        &self,
        Parameters(args): Parameters<GraphQualityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph, "quality".to_owned(), args.command];

        for node_type in args.node_types {
            cmd.push("--type".to_owned());
            cmd.push(node_type);
        }
        if let Some(limit) = args.limit {
            cmd.push("--limit".to_owned());
            cmd.push(limit.to_string());
        }
        if let Some(include_features) = args.include_features
            && include_features
        {
            cmd.push("--include-features".to_owned());
        }
        if let Some(threshold) = args.threshold {
            cmd.push("--threshold".to_owned());
            cmd.push(threshold.to_string());
        }
        if let Some(relation) = args.relation {
            cmd.push("--relation".to_owned());
            cmd.push(relation);
        }
        if let Some(sort) = args.sort {
            cmd.push("--sort".to_owned());
            cmd.push(sort);
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_export_html",
        description = "[DEPRECATED: use kg script] Export graph as an interactive HTML file. Prefer `kg <graph> export-html`."
    )]
    fn kg_export_html(
        &self,
        Parameters(args): Parameters<ExportHtmlArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph, "export-html".to_owned()];
        if let Some(output) = args.output {
            cmd.push("--output".to_owned());
            cmd.push(output);
        }
        if let Some(title) = args.title {
            cmd.push("--title".to_owned());
            cmd.push(title);
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_access_log",
        description = "[DEPRECATED: use kg script] Read graph access/search history. Prefer `kg <graph> access-log`."
    )]
    fn kg_access_log(
        &self,
        Parameters(args): Parameters<AccessLogArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut cmd = vec![args.graph, "access-log".to_owned()];
        if let Some(limit) = args.limit {
            cmd.push("--limit".to_owned());
            cmd.push(limit.to_string());
        }
        if args.show_empty {
            cmd.push("--show-empty".to_owned());
        }
        self.execute_kg(cmd)
    }

    #[tool(
        name = "kg_access_stats",
        description = "[DEPRECATED: use kg script] Read aggregate access statistics. Prefer `kg <graph> access-stats`."
    )]
    fn kg_access_stats(
        &self,
        Parameters(args): Parameters<AccessStatsArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_kg(vec![args.graph, "access-stats".to_owned()])
    }

    #[tool(
        name = "kg_gap_summary",
        description = "Get all quality gaps at once: missing descriptions, missing facts, edge gaps, and duplicates. Use this for collaborative graph improvement sessions."
    )]
    fn kg_gap_summary(
        &self,
        Parameters(args): Parameters<GapSummaryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let limit = args.limit.to_string();

        let mut output = String::new();
        output.push_str("## GRAPH QUALITY GAPS\n\n");

        let commands: Vec<(&str, Vec<&str>)> = vec![
            ("STATS", vec![&args.graph, "stats", "--by-type"]),
            (
                "MISSING DESCRIPTIONS",
                vec![
                    &args.graph,
                    "quality",
                    "missing-descriptions",
                    "--limit",
                    &limit,
                ],
            ),
            (
                "MISSING FACTS",
                vec![&args.graph, "quality", "missing-facts", "--limit", &limit],
            ),
            (
                "EDGE GAPS",
                vec![&args.graph, "quality", "edge-gaps", "--limit", &limit],
            ),
            (
                "DUPLICATES",
                vec![&args.graph, "quality", "duplicates", "--limit", &limit],
            ),
        ];

        for (section, cmd) in commands {
            let os_args: Vec<OsString> = std::iter::once(OsString::from("kg"))
                .chain(cmd.into_iter().map(OsString::from))
                .collect();
            let result = self.run_kg(os_args, "kg_gap_summary", "quality sweep", false)?;

            output.push_str(&format!("### {}\n{}\n\n", section, result));
        }

        output.push_str("---\n**Suggested workflow:** Pick one gap category above, then for each item ask the user for the missing information.\n");

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[prompt_router]
impl KgMcpServer {
    #[prompt(
        name = "kg_workflow_prompt",
        description = "Prompt template for planning kg operations"
    )]
    async fn kg_workflow_prompt(
        &self,
        Parameters(args): Parameters<KgPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let text = format!(
            "Use kg MCP tools to achieve this goal on graph `{}`: {}. First inspect with kg_stats and kg_node_find, then do safe mutations, and finally run kg_check.",
            args.graph, args.goal
        );
        Ok(GetPromptResult {
            description: Some("Recommended workflow for safe kg edits".to_owned()),
            messages: vec![PromptMessage::new_text(PromptMessageRole::User, text)],
        })
    }

    #[prompt(
        name = "kg_collaborative_prompt",
        description = "Collaborative graph improvement session - analyze gaps and work with user to fill them"
    )]
    async fn kg_collaborative_prompt(
        &self,
        Parameters(args): Parameters<KgPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let text = format!(
            r#"You are helping improve the knowledge graph `{}`. Your goal: {}

WORKFLOW:
1. Use kg_gap_summary to get all quality gaps at once
2. Present the top priorities to the user in Polish, asking which they want to work on
3. For each gap, ask the user specific questions to gather missing information
4. Use kg_node_add or kg_node_modify to fill the gaps based on user input
5. Use kg_edge_add to create connections
6. Run kg_check to verify integrity

IMPORTANT RULES:
- Always ask ONE gap at a time - don't overwhelm the user
- Provide context: show what already exists in the graph
- Accept partial information - even 1 fact is better than none
- Mark user-provided content with --source "user-input"
- After adding nodes, verify with kg_node_get

Example question for missing description:
"Węzeł '{}' nie ma opisu. Co to jest w 1-2 zdaniach?"

Example question for missing facts:
"Jakie 2-3 najważniejsze rzeczy warto wiedzieć o '{}'?""#,
            args.graph, args.goal, "{node_id}", "{node_name}"
        );
        Ok(GetPromptResult {
            description: Some("Collaborative graph improvement with user".to_owned()),
            messages: vec![PromptMessage::new_text(PromptMessageRole::User, text)],
        })
    }

    #[prompt(
        name = "kg_feedback_retrospective_prompt",
        description = "Retrospective session using feedback + gaps to improve the graph"
    )]
    async fn kg_feedback_retrospective_prompt(
        &self,
        Parameters(args): Parameters<KgPromptArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let text = format!(
            r#"You are facilitating a retrospective improvement session for the knowledge graph `{}`. Goal: {}

WORKFLOW:
1. Run `kg {} feedback-summary --limit 30` to summarize feedback signals
2. Run `kg_gap_summary` for the same graph to discover quality gaps
3. Identify the top NIL queries and repeated NO responses; treat them as missing nodes or missing relations
4. Ask the user ONE targeted question at a time to fill the gap (description, facts, missing edges)
5. Apply updates with kg_node_add / kg_node_modify / kg_edge_add using --source "user-input"
6. Verify with kg_node_get and finish with kg_check

RULES:
- Keep the conversation in Polish
- Be concrete: always propose a specific node or relation to add/update
- Prefer small, safe changes over big edits

Example question:
"Widzę brak dla zapytania '{{query}}'. Czy to powinien być nowy węzeł? Jeśli tak, podaj nazwę i 1-2 fakty."
"#,
            args.graph, args.goal, args.graph
        );
        Ok(GetPromptResult {
            description: Some("Feedback-driven retrospective session".to_owned()),
            messages: vec![PromptMessage::new_text(PromptMessageRole::User, text)],
        })
    }
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for KgMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
            server_info: Implementation {
                name: "kg-mcp".to_owned(),
                title: Some("kg MCP server".to_owned()),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                description: Some("MCP server for the kg knowledge graph CLI".to_owned()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Use typed tools for common operations or kg_command for full CLI coverage."
                    .to_owned(),
            ),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = vec![
            RawResource::new("kg://cwd", "Current working directory").no_annotation(),
            RawResource::new("kg://graphs", "Discovered graph files").no_annotation(),
        ];
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![RawResourceTemplate {
                uri_template: "kg://graph/{graph}".to_owned(),
                name: "Graph summary by graph name".to_owned(),
                title: None,
                description: Some(
                    "Runs `kg <graph> stats --by-type --by-relation` and returns the text output"
                        .to_owned(),
                ),
                mime_type: Some("text".to_owned()),
                icons: None,
            }
            .no_annotation()],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri;
        let text = if uri.as_str() == "kg://cwd" {
            self.cwd.display().to_string()
        } else if uri.as_str() == "kg://graphs" {
            serde_json::to_string_pretty(&discover_graphs(&self.cwd)).map_err(|err| {
                McpError::internal_error(
                    "failed to encode graphs",
                    Some(json!({ "error": err.to_string() })),
                )
            })?
        } else if let Some(graph_name) = uri.as_str().strip_prefix("kg://graph/") {
            self.run_kg(
                vec![
                    OsString::from("kg"),
                    OsString::from(graph_name),
                    OsString::from("stats"),
                    OsString::from("--by-type"),
                    OsString::from("--by-relation"),
                ],
                "read_resource",
                "graph stats resource",
                false,
            )
            .map_err(|err| {
                McpError::internal_error(
                    "failed to render graph resource",
                    Some(json!({ "error": err.to_string(), "graph": graph_name })),
                )
            })?
        } else {
            return Err(McpError::resource_not_found(
                "resource_not_found",
                Some(json!({ "uri": uri.as_str() })),
            ));
        };

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(text, uri)],
        })
    }
}

fn append_validation_flags(cmd: &mut Vec<String>, args: &GraphValidateArgs) {
    if args.deep {
        cmd.push("--deep".to_owned());
    }
    if let Some(base_dir) = &args.base_dir {
        cmd.push("--base-dir".to_owned());
        cmd.push(base_dir.clone());
    }
    if args.errors_only {
        cmd.push("--errors-only".to_owned());
    }
    if args.warnings_only {
        cmd.push("--warnings-only".to_owned());
    }
    if let Some(limit) = args.limit {
        cmd.push("--limit".to_owned());
        cmd.push(limit.to_string());
    }
}

fn discover_graphs(cwd: &Path) -> Vec<String> {
    let mut paths = vec![default_graph_root(cwd), cwd.join(".kg").join("graphs")];
    paths.sort();
    paths.dedup();

    let mut graphs = Vec::new();
    for dir in paths {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                graphs.push(path.display().to_string());
            }
        }
    }
    graphs.sort();
    graphs.dedup();
    graphs
}

fn default_graph_root(cwd: &Path) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from));
    match home {
        Some(home) => home.join(".kg").join("graphs"),
        None => cwd.join(".kg").join("graphs"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteMode {
    None,
    Single,
    Double,
}

fn split_script(script: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut mode = QuoteMode::None;
    let mut escape = false;

    for ch in script.chars() {
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

fn tokenize_command(cmd: &str) -> Result<Vec<String>, String> {
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

fn parse_node_find_args(args: &[String]) -> Option<Result<NodeFindArgs, String>> {
    if args.len() < 3 {
        return None;
    }
    if args[1] != "node" || args[2] != "find" {
        return None;
    }
    if args.len() < 4 {
        return Some(Err("missing query".to_owned()));
    }

    let graph = args[0].clone();
    let mut queries = Vec::new();
    let mut limit = None;
    let mut output_size = None;
    let mut mode = None;
    let mut full = false;

    let mut i = 3;
    while i < args.len() {
        let token = &args[i];
        if token == "--limit" {
            i += 1;
            if i >= args.len() {
                return Some(Err("missing value for --limit".to_owned()));
            }
            let value = args[i]
                .parse::<usize>()
                .map_err(|_| "invalid value for --limit".to_owned());
            match value {
                Ok(n) => limit = Some(n),
                Err(err) => return Some(Err(err)),
            }
            i += 1;
            continue;
        }
        if token == "--output-size" {
            i += 1;
            if i >= args.len() {
                return Some(Err("missing value for --output-size".to_owned()));
            }
            let value = args[i]
                .parse::<usize>()
                .map_err(|_| "invalid value for --output-size".to_owned());
            match value {
                Ok(n) => output_size = Some(n),
                Err(err) => return Some(Err(err)),
            }
            i += 1;
            continue;
        }
        if token == "--mode" {
            i += 1;
            if i >= args.len() {
                return Some(Err("missing value for --mode".to_owned()));
            }
            mode = Some(args[i].clone());
            i += 1;
            continue;
        }
        if token == "--full" {
            full = true;
            i += 1;
            continue;
        }
        if token.starts_with("--") {
            return Some(Err(format!("unknown option: {token}")));
        }
        queries.push(token.clone());
        i += 1;
    }

    if queries.is_empty() {
        return Some(Err("missing query".to_owned()));
    }

    Some(Ok(NodeFindArgs {
        graph,
        queries,
        limit,
        output_size,
        mode,
        full,
        skip_feedback: false,
        debug: false,
    }))
}

fn parse_node_get_args(args: &[String]) -> Option<Result<NodeGetArgs, String>> {
    if args.len() < 4 {
        return None;
    }
    if args[1] != "node" || args[2] != "get" {
        return None;
    }

    let graph = args[0].clone();
    let id = args[3].clone();
    if id.is_empty() {
        return Some(Err("missing node id".to_owned()));
    }

    let mut output_size = None;
    let mut full = false;

    let mut i = 4;
    while i < args.len() {
        let token = &args[i];
        if token == "--output-size" {
            i += 1;
            if i >= args.len() {
                return Some(Err("missing value for --output-size".to_owned()));
            }
            let value = args[i]
                .parse::<usize>()
                .map_err(|_| "invalid value for --output-size".to_owned());
            match value {
                Ok(n) => output_size = Some(n),
                Err(err) => return Some(Err(err)),
            }
            i += 1;
            continue;
        }
        if token == "--full" {
            full = true;
            i += 1;
            continue;
        }
        if token.starts_with("--") {
            return Some(Err(format!("unknown option: {token}")));
        }
        return Some(Err(format!("unexpected argument: {token}")));
    }

    Some(Ok(NodeGetArgs {
        graph,
        id,
        output_size,
        full,
        debug: false,
    }))
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn classify_kg_error(message: &str) -> (ErrorCode, &'static str, &'static str, i32) {
    if looks_like_permission_error(message) {
        return (
            ErrorCode(-32012),
            "kg command permission denied",
            "permission_denied",
            1,
        );
    }
    if looks_like_parse_error(message) {
        return (
            ErrorCode(-32011),
            "kg command parse error",
            "parse_error",
            2,
        );
    }
    (ErrorCode(-32010), "kg command failed", "command_failed", 1)
}

fn looks_like_parse_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("usage:")
        || lower.contains("for more information, try")
        || lower.contains("unexpected argument")
        || lower.contains("unrecognized subcommand")
        || lower.contains("required arguments were not provided")
        || lower.contains("a value is required")
}

fn looks_like_permission_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission denied") || lower.contains("os error 13")
}

fn error_hint(kind: &str) -> &'static str {
    match kind {
        "parse_error" => "Check command syntax and required arguments.",
        "permission_denied" => "Verify file permissions and graph path access.",
        _ => "Inspect stderr_tail for details.",
    }
}

fn last_lines(input: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return input.trim().to_owned();
    }
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn env_flag_enabled(name: &str) -> bool {
    match std::env::var(name) {
        Ok(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn normalize_query_key(queries: &[String]) -> Option<String> {
    let key = queries
        .iter()
        .map(|q| q.trim().to_ascii_lowercase())
        .filter(|q| !q.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if key.is_empty() { None } else { Some(key) }
}

fn update_feedback_stats(state: &mut FeedbackState, queries: &[String], action: &str) {
    state.global_stats.add_action(action);
    if let Some(key) = normalize_query_key(queries) {
        state.query_stats.entry(key).or_default().add_action(action);
    }
}

fn parse_feedback_log_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split('\t')
        .find_map(|part| part.strip_prefix(&format!("{key}=")))
}

fn initialize_feedback_state(cwd: &Path) -> FeedbackState {
    let path = kg::first_existing_feedback_log_path(cwd);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return FeedbackState::default(),
    };

    let mut query_stats: HashMap<String, QueryFeedbackStats> = HashMap::new();
    let mut global_stats = GlobalFeedbackStats::default();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let action = match parse_feedback_log_field(&line, "action") {
            Some(action) if matches!(action, "YES" | "NO" | "NIL" | "PICK") => action,
            _ => continue,
        };
        global_stats.add_action(action);
        if let Some(queries) = parse_feedback_log_field(&line, "queries") {
            let query_values: Vec<String> = queries
                .split(" | ")
                .map(|q| q.trim().to_owned())
                .filter(|q| !q.is_empty() && q != "-")
                .collect();
            if let Some(key) = normalize_query_key(&query_values) {
                query_stats.entry(key).or_default().add_action(action);
            }
        }
    }

    FeedbackState {
        counter: 0,
        finds: HashMap::new(),
        query_stats,
        global_stats,
    }
}

fn compute_adaptive_nudge_percent(
    base_percent: u8,
    queries: &[String],
    total_results: usize,
    query_stats: &HashMap<String, QueryFeedbackStats>,
    global_stats: &GlobalFeedbackStats,
) -> u8 {
    let mut effective = i32::from(base_percent);

    if total_results == 0 {
        effective = effective.max(40);
    }

    if let Some(key) = normalize_query_key(queries)
        && let Some(stats) = query_stats.get(&key)
    {
        let events = stats.events();
        if events >= 3 && stats.negative_ratio() >= 0.5 {
            effective += 25;
        } else if events >= 5 && stats.positive_ratio() >= 0.8 {
            effective -= 20;
        }
    }

    if global_stats.events >= 20 && global_stats.negative_ratio() >= 0.4 {
        effective += 10;
    }

    effective.clamp(0, 100) as u8
}

fn should_emit_nudge(percent: u8, salt: &str) -> bool {
    match percent {
        0 => false,
        100 => true,
        percent => {
            let mut hasher = DefaultHasher::new();
            salt.hash(&mut hasher);
            hasher.finish() % 100 < u64::from(percent)
        }
    }
}

fn redact_cli_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut mask_next = false;
    for arg in args {
        if mask_next {
            redacted.push("***REDACTED***".to_owned());
            mask_next = false;
            continue;
        }

        if let Some((key, _value)) = arg.split_once('=') {
            if is_sensitive_key(key) {
                redacted.push(format!("{key}=***REDACTED***"));
                continue;
            }
        }

        if let Some(key) = arg.strip_prefix("--") {
            if is_sensitive_key(key) {
                if arg.contains('=') {
                    redacted.push(format!("--{key}=***REDACTED***"));
                } else {
                    redacted.push(arg.clone());
                    mask_next = true;
                }
                continue;
            }
        }

        redacted.push(arg.clone());
    }
    redacted
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.ends_with("key")
        || lower.contains("api_key")
}

fn to_base36(mut n: u64) -> String {
    const ALPH: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_owned();
    }
    let mut buf = [0u8; 13];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = ALPH[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8_lossy(&buf[i..]).to_string()
}

fn parse_find_total_results(rendered: &str) -> Option<usize> {
    let mut total = 0usize;
    let mut any = false;
    for line in rendered.lines() {
        let line = line.trim_end();
        if !line.starts_with("? ") {
            continue;
        }
        // Expected: "? query (N)" or "? query (shown/total)"
        let open = match line.rfind('(') {
            Some(v) => v,
            None => continue,
        };
        let close = match line.rfind(')') {
            Some(v) => v,
            None => continue,
        };
        if close <= open + 1 {
            continue;
        }
        let inside = match line.get(open + 1..close) {
            Some(v) => v,
            None => continue,
        };
        let count = if let Some((_, total)) = inside.split_once('/') {
            total.trim().parse::<usize>().ok()
        } else {
            inside.trim().parse::<usize>().ok()
        };
        if let Some(n) = count {
            total = total.saturating_add(n);
            any = true;
        }
    }
    any.then_some(total)
}

fn parse_find_candidate_ids(rendered: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in rendered.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some((id, _)) = rest.split_once(" | ") {
                let id = id.trim();
                if !id.is_empty() {
                    ids.push(id.to_owned());
                }
            }
        }
    }
    ids
}

fn feedback_delta(action: &str) -> Option<f64> {
    match action {
        "YES" => Some(1.0),
        "NO" => Some(-1.0),
        "PICK" => Some(2.0),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct ParsedFeedback {
    uid: String,
    action: String,
    pick: Option<usize>,
}

fn parse_feedback_line(line: &str) -> Option<ParsedFeedback> {
    // Accept:
    // - "uid=abc123 YES"
    // - "uid=abc123 NIL"
    // - "uid=abc123 PICK 2"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    if parts.len() < 2 {
        return None;
    }
    let uid = parts[0].strip_prefix("uid=")?.trim().to_owned();
    if uid.is_empty() {
        return None;
    }

    let action = parts[1].to_ascii_uppercase();
    match action.as_str() {
        "YES" | "NO" | "NIL" => Some(ParsedFeedback {
            uid,
            action,
            pick: None,
        }),
        "PICK" => {
            if parts.len() < 3 {
                return None;
            }
            let pick = parts[2].parse::<usize>().ok()?;
            Some(ParsedFeedback {
                uid,
                action,
                pick: Some(pick),
            })
        }
        _ => None,
    }
}

fn cleanup_old_finds(finds: &mut HashMap<String, FindContext>, now_ms: u128, ttl_ms: u128) {
    finds.retain(|_, ctx| now_ms.saturating_sub(ctx.created_at_ms) <= ttl_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_test_graph_workspace(cwd: &Path) {
        fs::create_dir_all(cwd.join(".kg/graphs")).expect("create graph root");
        fs::write(
            cwd.join(".kg.toml"),
            "graphs.fridge = \".kg/graphs/fridge.json\"\n",
        )
        .expect("write config");
        fs::write(
            cwd.join(".kg/graphs/fridge.json"),
            include_str!("../../graph-example-fridge.json"),
        )
        .expect("write fixture");
    }

    fn load_test_graph(cwd: &Path) -> kg::GraphFile {
        let kg_path = cwd.join(".kg/graphs/fridge.kg");
        let json_path = cwd.join(".kg/graphs/fridge.json");
        let path = if kg_path.exists() { kg_path } else { json_path };
        kg::GraphFile::load(&path).expect("load graph")
    }

    #[test]
    fn split_script_handles_semicolons_and_newlines() {
        let script = "a;b\nc";
        let parts = split_script(script);
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_script_respects_quotes() {
        let script = "a; \"b;c\"; 'd;e'";
        let parts = split_script(script);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].trim(), "\"b;c\"");
        assert_eq!(parts[2].trim(), "'d;e'");
    }

    #[test]
    fn split_script_allows_escaped_delimiter() {
        let script = "a\\;b;c";
        let parts = split_script(script);
        assert_eq!(parts, vec!["a\\;b", "c"]);
    }

    #[test]
    fn tokenize_command_parses_quotes_and_escapes() {
        let cmd = "fridge node find \"smart fridge\"";
        let tokens = tokenize_command(cmd).expect("tokenize");
        assert_eq!(tokens, vec!["fridge", "node", "find", "smart fridge"]);
    }

    #[test]
    fn tokenize_command_handles_escaped_semicolon() {
        let cmd = "note\\;extra";
        let tokens = tokenize_command(cmd).expect("tokenize");
        assert_eq!(tokens, vec!["note;extra"]);
    }

    #[test]
    fn tokenize_command_errors_on_unterminated_quote() {
        let cmd = "fridge node find \"smart";
        let err = tokenize_command(cmd).unwrap_err();
        assert_eq!(err, "unterminated quote");
    }

    #[test]
    fn parse_node_find_args_parses_options() {
        let args = vec![
            "fridge".to_owned(),
            "node".to_owned(),
            "find".to_owned(),
            "lodowka".to_owned(),
            "--limit".to_owned(),
            "5".to_owned(),
            "--mode".to_owned(),
            "bm25".to_owned(),
            "--full".to_owned(),
        ];
        let parsed = parse_node_find_args(&args).expect("match").expect("ok");
        assert_eq!(parsed.graph, "fridge");
        assert_eq!(parsed.queries, vec!["lodowka"]);
        assert_eq!(parsed.limit, Some(5));
        assert_eq!(parsed.mode.as_deref(), Some("bm25"));
        assert!(parsed.full);
    }

    #[test]
    fn parse_node_get_args_rejects_unknown_option() {
        let args = vec![
            "fridge".to_owned(),
            "node".to_owned(),
            "get".to_owned(),
            "concept:refrigerator".to_owned(),
            "--nope".to_owned(),
        ];
        let err = parse_node_get_args(&args).expect("match").unwrap_err();
        assert!(err.contains("unknown option"));
    }

    #[test]
    fn parse_node_find_args_parses_output_size() {
        let args = vec![
            "fridge".to_owned(),
            "node".to_owned(),
            "find".to_owned(),
            "lodowka".to_owned(),
            "--output-size".to_owned(),
            "900".to_owned(),
        ];
        let parsed = parse_node_find_args(&args).expect("match").expect("ok");
        assert_eq!(parsed.output_size, Some(900));
    }

    #[test]
    fn parse_node_get_args_parses_output_size() {
        let args = vec![
            "fridge".to_owned(),
            "node".to_owned(),
            "get".to_owned(),
            "concept:refrigerator".to_owned(),
            "--output-size".to_owned(),
            "750".to_owned(),
        ];
        let parsed = parse_node_get_args(&args).expect("match").expect("ok");
        assert_eq!(parsed.output_size, Some(750));
    }

    #[test]
    fn parse_find_total_results_supports_shown_total_headers() {
        let rendered = "? lodowka (2/11)\n# concept:refrigerator | Lodowka [Concept]\n\n? api (1)\n# interface:smart_api | Smart API [Interface]\n";
        assert_eq!(parse_find_total_results(rendered), Some(12));
    }

    #[test]
    fn classify_kg_error_detects_parse_errors() {
        let message = "error: unexpected argument 'x' found\n\nUsage: kg graph";
        let (code, _msg, kind, exit_code) = classify_kg_error(message);
        assert_eq!(code.0, -32011);
        assert_eq!(kind, "parse_error");
        assert_eq!(exit_code, 2);
    }

    #[test]
    fn redact_cli_args_masks_sensitive_values() {
        let args = vec![
            "kg".to_owned(),
            "--token".to_owned(),
            "abc123".to_owned(),
            "api_key=secret123".to_owned(),
            "--mode".to_owned(),
            "bm25".to_owned(),
        ];
        let redacted = redact_cli_args(&args);
        assert_eq!(redacted[2], "***REDACTED***");
        assert_eq!(redacted[3], "api_key=***REDACTED***");
        assert_eq!(redacted[5], "bm25");
    }

    #[test]
    fn should_emit_nudge_respects_zero_and_hundred() {
        assert!(!should_emit_nudge(0, "abc123"));
        assert!(should_emit_nudge(100, "abc123"));
    }

    #[test]
    fn should_emit_nudge_is_deterministic_for_same_salt() {
        let first = should_emit_nudge(20, "abc123");
        let second = should_emit_nudge(20, "abc123");
        assert_eq!(first, second);
    }

    #[test]
    fn parse_feedback_line_allows_passive_suffix() {
        let parsed = parse_feedback_line("uid=abc123 PICK 2 passive=1").expect("parse");
        assert_eq!(parsed.uid, "abc123");
        assert_eq!(parsed.action, "PICK");
        assert_eq!(parsed.pick, Some(2));
    }

    #[test]
    fn adaptive_nudge_guardrail_for_zero_results() {
        let percent = compute_adaptive_nudge_percent(
            5,
            &["missing query".to_owned()],
            0,
            &HashMap::new(),
            &GlobalFeedbackStats::default(),
        );
        assert_eq!(percent, 40);
    }

    #[test]
    fn adaptive_nudge_increases_on_negative_history() {
        let mut query_stats = HashMap::new();
        query_stats.insert(
            "smart fridge".to_owned(),
            QueryFeedbackStats {
                yes: 1,
                no: 2,
                nil: 1,
                pick: 0,
            },
        );
        let percent = compute_adaptive_nudge_percent(
            10,
            &["smart fridge".to_owned()],
            3,
            &query_stats,
            &GlobalFeedbackStats::default(),
        );
        assert_eq!(percent, 35);
    }

    #[test]
    fn adaptive_nudge_decreases_on_positive_history() {
        let mut query_stats = HashMap::new();
        query_stats.insert(
            "smart fridge".to_owned(),
            QueryFeedbackStats {
                yes: 4,
                no: 0,
                nil: 0,
                pick: 1,
            },
        );
        let percent = compute_adaptive_nudge_percent(
            25,
            &["smart fridge".to_owned()],
            3,
            &query_stats,
            &GlobalFeedbackStats::default(),
        );
        assert_eq!(percent, 5);
    }

    #[test]
    fn adaptive_nudge_adds_global_guardrail() {
        let global = GlobalFeedbackStats {
            events: 25,
            negative_events: 12,
        };
        let percent =
            compute_adaptive_nudge_percent(10, &["query".to_owned()], 3, &HashMap::new(), &global);
        assert_eq!(percent, 20);
    }

    #[test]
    fn apply_feedback_updates_appends_f_record_to_kglog() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = dir.path();
        fs::write(
            cwd.join(".kg.toml"),
            "user_short_uid = \"tester01\"\ngraphs.fridge = \"fridge.kg\"\n",
        )
        .expect("write config");
        fs::write(
            cwd.join("fridge.kg"),
            "@ K:concept:refrigerator\nN Lodowka\nD Desc\nE\nP M\n",
        )
        .expect("write graph");

        let server = KgMcpServer::new(cwd.to_path_buf()).expect("server");
        let updates = vec![FeedbackUpdate {
            item_index: 0,
            graph: "fridge".to_owned(),
            node_id: "concept:refrigerator".to_owned(),
            action: "YES".to_owned(),
            delta: 1.0,
            ts_ms: 1,
        }];

        let results = server.apply_feedback_updates(&updates);
        assert!(matches!(results.get(&0), Some(Ok(()))));

        let kglog_raw = fs::read_to_string(cwd.join(".kg").join("cache").join("fridge.kglog"))
            .expect("read kglog");
        assert!(kglog_raw.contains(" tester01 F concept:refrigerator YES"));
    }

    #[test]
    fn kg_script_edge_add_supports_detail_flag() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = dir.path();
        write_test_graph_workspace(cwd);

        let server = KgMcpServer::new(cwd.to_path_buf()).expect("server");
        let result = server
            .kg(Parameters(KgScriptArgs {
                script: "fridge edge add process:defrost AVAILABLE_IN interface:smart_api --detail \"Proces rozmrazania dostepny z API\"".to_owned(),
                mode: None,
                debug: false,
            }))
            .expect("kg script result");

        assert_eq!(result.is_error, Some(false));
        let graph = load_test_graph(cwd);
        let edge = graph
            .edges
            .iter()
            .find(|edge| {
                edge.source_id == "process:defrost"
                    && edge.relation == "AVAILABLE_IN"
                    && edge.target_id == "interface:smart_api"
            })
            .expect("edge added");
        assert_eq!(edge.properties.detail, "Proces rozmrazania dostepny z API");
    }

    #[test]
    fn kg_edge_add_batch_dry_run_reports_failures_without_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = dir.path();
        write_test_graph_workspace(cwd);

        let server = KgMcpServer::new(cwd.to_path_buf()).expect("server");
        let result = server
            .kg_edge_add_batch(Parameters(EdgeAddBatchArgs {
                graph: "fridge".to_owned(),
                edges: vec![
                    EdgeAddBatchItem {
                        source_id: "process:defrost".to_owned(),
                        relation: "AVAILABLE_IN".to_owned(),
                        target_id: "interface:smart_api".to_owned(),
                        detail: Some("Proces rozmrazania dostepny z API".to_owned()),
                    },
                    EdgeAddBatchItem {
                        source_id: "process:defrost".to_owned(),
                        relation: "AVAILABLE_IN".to_owned(),
                        target_id: "datastore:temperature_log".to_owned(),
                        detail: None,
                    },
                ],
                mode: Some("best_effort".to_owned()),
                dry_run: true,
            }))
            .expect("batch dry run");

        assert_eq!(result.is_error, Some(true));
        let structured = result.structured_content.expect("structured content");
        assert_eq!(structured["dry_run"], true);
        assert_eq!(structured["would_add"], 1);
        assert_eq!(structured["added"], 0);
        assert_eq!(structured["failed"], 1);
        assert!(
            structured["results"][1]["error"]
                .as_str()
                .expect("error text")
                .contains("DataStore cannot be target of AVAILABLE_IN")
        );

        let graph = load_test_graph(cwd);
        assert!(!graph.has_edge("process:defrost", "AVAILABLE_IN", "interface:smart_api"));
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let service = KgMcpServer::new(cwd)?;
    let server = service
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    server.waiting().await?;
    Ok(())
}
