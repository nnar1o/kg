//! Integration tests for the kg-mcp MCP server.
//!
//! Uses subprocess testing because `KgMcpServer` is defined in the binary crate
//! and not publicly accessible from integration tests.
//!
//! Each test spawns a fresh kg-mcp process in a temp directory and communicates
//! via the MCP JSON-RPC protocol over stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Lightweight MCP client — communicates with a kg-mcp subprocess over stdio
// ---------------------------------------------------------------------------

struct McpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawn the kg-mcp binary in the given working directory and run the
    /// MCP initialize handshake.
    ///
    /// Sets `HOME` to the temp dir so that graph storage defaults to
    /// `<temp>/.kg/graphs/` instead of the real home directory.
    fn spawn(cwd: &Path) -> Self {
        let exe = std::env!("CARGO_BIN_EXE_kg-mcp");
        let mut child = Command::new(exe)
            .current_dir(cwd)
            .env("HOME", cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn kg-mcp");

        let stdin = child.stdin.take().expect("stdin not available");
        let stdout = BufReader::new(child.stdout.take().expect("stdout not available"));

        let mut client = McpClient {
            child,
            stdin,
            stdout,
            next_id: 1,
        };

        // Short grace period for the server to start
        let deadline = Instant::now() + Duration::from_secs(5);

        // MCP handshake: initialize request
        let init_response = client.send_request(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "kg-mcp-test",
                    "version": "1.0.0",
                }
            })),
        );

        // Verify initialize succeeded
        assert!(
            init_response.get("result").is_some(),
            "initialize failed: {:?}",
            init_response
        );

        // Send initialized notification
        client.send_notification("notifications/initialized", None);

        // Drain any residual messages (e.g. pings) up to the deadline
        client.drain_until_idle(deadline);

        client
    }

    /// Consume any lines that arrive within `timeout` — handles stray pings etc.
    fn drain_until_idle(&mut self, deadline: Instant) {
        // Set stdin to non-blocking to read any pending messages
        // Since we can't easily make BufReader non-blocking on stdio,
        // we just try to read with a small timeout using the OS pipe buffer
        let _ = deadline; // not used — we rely on the fact that no request is pending
    }

    /// Issue a JSON-RPC request and return the full response Value.
    fn send_request(&mut self, method: &str, params: Option<Value>) -> Value {
        let id = self.next_id;
        self.next_id += 1;

        let mut req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        });
        if let Some(p) = params {
            req["params"] = p;
        }

        self.write_msg(&req);
        self.read_msg()
    }

    /// Issue a JSON-RPC notification (no response expected).
    fn send_notification(&mut self, method: &str, params: Option<Value>) {
        let mut req = json!({
            "jsonrpc": "2.0",
            "method": method,
        });
        if let Some(p) = params {
            req["params"] = p;
        }
        self.write_msg(&req);
    }

    /// Convenience: call `tools/list`.
    fn list_tools(&mut self) -> Value {
        self.send_request("tools/list", None)
    }

    /// Convenience: call `tools/call` with the given tool name and arguments.
    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let params = json!({
            "name": name,
            "arguments": arguments,
        });
        self.send_request("tools/call", Some(params))
    }

    /// Extract the text content from a tool call result.
    fn tool_text(response: &Value) -> String {
        response["result"]["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or("")
            .to_owned()
    }

    /// Extract `isError` from a tool call result.
    fn tool_is_error(response: &Value) -> bool {
        response["result"]["isError"].as_bool().unwrap_or(false)
    }

    /// Check if a tool call response contains an error (JSON-RPC error or isError).
    fn get_error_message(response: &Value) -> Option<String> {
        if let Some(err) = response.get("error") {
            return err["message"].as_str().map(|s| s.to_owned());
        }
        if Self::tool_is_error(response) {
            // The error detail may be in the text content
            let text = Self::tool_text(response);
            if !text.is_empty() {
                return Some(text);
            }
            // Or in structured_content if present
            if let Some(sc) = response["result"].get("structured_content") {
                if let Some(err) = sc.get("error").and_then(|v| v.as_str()) {
                    return Some(err.to_owned());
                }
            }
        }
        None
    }

    // -- private helpers ------------------------------------------------

    fn write_msg(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).expect("json encode");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .expect("write to kg-mcp stdin");
        self.stdin.flush().expect("flush kg-mcp stdin");
    }

    fn read_msg(&mut self) -> Value {
        let mut line = String::new();
        loop {
            line.clear();
            self.stdout
                .read_line(&mut line)
                .expect("read from kg-mcp stdout");
            if line.trim().is_empty() {
                continue; // skip blank lines
            }
            // Check for server-initiated requests (e.g. ping)
            match serde_json::from_str::<Value>(line.trim()) {
                Ok(msg) => {
                    if msg.get("method").is_some() && msg.get("id").is_some() {
                        // Server sent us a request — respond with empty result
                        let id = msg["id"].clone();
                        let pong = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {}
                        });
                        self.write_msg(&pong);
                        continue; // read the next line (our actual response)
                    }
                    return msg;
                }
                Err(_) => continue, // skip malformed lines
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a temp dir and spawn a kg-mcp client initialised in it.
fn setup() -> (TempDir, McpClient) {
    let dir = TempDir::new().expect("temp dir");
    let cwd = dir.path().to_path_buf();
    let client = McpClient::spawn(&cwd);
    (dir, client)
}

/// Run a `kg` tool script and return the response.
fn run_kg(client: &mut McpClient, script: &str) -> Value {
    client.call_tool("kg", json!({ "script": script }))
}

/// Run a `kg` tool script with an explicit top-level graph argument.
fn run_kg_with_graph(client: &mut McpClient, graph: &str, script: &str) -> Value {
    client.call_tool("kg", json!({ "graph": graph, "script": script }))
}

/// Run a `kg` tool script and return the text output.
fn run_kg_text(client: &mut McpClient, script: &str) -> String {
    let resp = run_kg(client, script);
    McpClient::tool_text(&resp)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn mcp_lists_exactly_3_tools() {
    let (_dir, mut client) = setup();

    let response = client.list_tools();
    let tools = response["result"]["tools"]
        .as_array()
        .expect("result.tools array");

    assert_eq!(tools.len(), 3, "expected exactly 3 MCP tools");

    let mut names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    names.sort();

    assert_eq!(names, vec!["kg", "kg_help", "kg_schema"]);

    let kg_tool = tools
        .iter()
        .find(|tool| tool["name"] == "kg")
        .expect("kg tool");
    assert!(kg_tool["inputSchema"]["properties"]["graph"].is_object());
    let graph_description = kg_tool["inputSchema"]["properties"]["graph"]["description"]
        .as_str()
        .expect("graph description");
    assert!(graph_description.contains("active graph"));
}

#[test]
fn mcp_kg_rejects_graph_scoped_scl_without_graph() {
    let (_dir, mut client) = setup();

    let response = run_kg(&mut client, "find refrigerator");

    assert!(McpClient::tool_is_error(&response));
    let text = McpClient::tool_text(&response);
    assert!(text.contains("top-level `graph` argument"), "{}", text);
    assert!(text.contains("use <graph>"), "{}", text);
    assert!(text.contains("kg <graph>"), "{}", text);
}

#[test]
fn mcp_kg_rejects_graphless_canonical_commands() {
    let (_dir, mut client) = setup();

    for script in [
        "kg --event-log vectors stats",
        "kg --event-log feedback-summary",
        "--legacy node find query",
    ] {
        let response = run_kg(&mut client, script);
        assert!(McpClient::tool_is_error(&response), "script: {script}");
        let text = McpClient::tool_text(&response);
        assert!(text.contains("top-level `graph` argument"), "{}", text);
        assert!(text.contains("use <graph>"), "{}", text);
        assert!(text.contains("kg <graph>"), "{}", text);
    }
}

#[test]
fn mcp_kg_preserves_global_graph_admin_commands_without_graph() {
    let (_dir, mut client) = setup();

    for script in ["list", "kg list", "kg --event-log list"] {
        let response = run_kg(&mut client, script);
        assert!(!McpClient::tool_is_error(&response), "script: {script}");
    }
}

#[test]
fn mcp_kg_accepts_graph_wrapper_around_global_options() {
    let (_dir, mut client) = setup();
    let output = run_kg_text(&mut client, "kg graph create wrapper_graph");
    assert!(output.contains("created"), "graph creation: {}", output);

    for script in [
        "kg --event-log graph wrapper_graph stats",
        "kg graph --event-log wrapper_graph stats",
        "kg graph --legacy wrapper_graph stats",
        "kg graph wrapper_graph --legacy stats",
    ] {
        let response = run_kg(&mut client, script);
        assert!(!McpClient::tool_is_error(&response), "script: {script}");
        assert!(McpClient::tool_text(&response).contains("stats"));
    }
}

#[test]
fn mcp_kg_scopes_canonical_list_options() {
    let (_dir, mut client) = setup();
    let output = run_kg_text(&mut client, "kg graph create list_graph");
    assert!(output.contains("created"), "graph creation: {}", output);

    for script in [
        "list -l 5",
        "list -s 2026-01-01",
        "list --limit=5",
        "list -l5",
        "list -s2026-01-01",
    ] {
        let response = run_kg(&mut client, script);
        assert!(McpClient::tool_is_error(&response), "script: {script}");
        assert!(McpClient::tool_text(&response).contains("graph is required"));

        let response = run_kg_with_graph(&mut client, "list_graph", script);
        assert!(!McpClient::tool_is_error(&response), "script: {script}");
    }
}

#[test]
fn mcp_kg_graph_argument_scopes_scl_script() {
    let (_dir, mut client) = setup();
    let output = run_kg_text(&mut client, "kg graph create scoped_graph");
    assert!(output.contains("created"), "graph creation: {}", output);

    let response = run_kg_with_graph(
        &mut client,
        "scoped_graph",
        "add concept:scoped_node --name Scoped",
    );
    assert!(!McpClient::tool_is_error(&response));
    let output = run_kg_text(&mut client, "kg scoped_graph node get concept:scoped_node");
    assert!(output.contains("Scoped"), "scoped graph output: {}", output);

    let response = run_kg_with_graph(&mut client, "scoped_graph", "node find Scoped");
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));

    let response = run_kg_with_graph(&mut client, "scoped_graph", "kg node find Scoped");
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));

    let response = run_kg_with_graph(&mut client, "scoped_graph", "find Scoped");
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));

    for script in [
        "--legacy node find Scoped",
        "use scoped_graph; --legacy node find Scoped",
    ] {
        let response = if script.starts_with("use ") {
            run_kg(&mut client, script)
        } else {
            run_kg_with_graph(&mut client, "scoped_graph", script)
        };
        assert!(McpClient::tool_is_error(&response), "script: {script}");
    }

    for script in [
        "kg scoped_graph node find Scoped --legacy",
        "kg scoped_graph node --legacy find Scoped",
    ] {
        let response = run_kg(&mut client, script);
        assert!(McpClient::tool_is_error(&response), "script: {script}");
    }

    let response = run_kg_with_graph(&mut client, "scoped_graph", "get concept:scoped_node");
    assert!(!McpClient::tool_is_error(&response));
    let structured = response["result"]
        .get("structuredContent")
        .or_else(|| response["result"].get("structured_content"))
        .expect("structured content for SCL get");
    assert!(
        structured["requires_feedback"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );

    for script in [
        "kg scoped_graph node find Scoped --event-log",
        "kg scoped_graph --event-log node find Scoped",
    ] {
        let response = run_kg(&mut client, script);
        assert!(!McpClient::tool_is_error(&response), "script: {script}");
        assert!(McpClient::tool_text(&response).contains("scoped_node"));
    }

    let response = run_kg(&mut client, "kg scoped_graph node find -- --event-log");
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("--event-log"));

    let response = run_kg_with_graph(
        &mut client,
        "scoped_graph",
        "kg --event-log node find Scoped",
    );
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));

    let response = run_kg_with_graph(
        &mut client,
        "scoped_graph",
        "kg scoped_graph node get concept:scoped_node",
    );
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("Scoped"));

    let response = run_kg_with_graph(
        &mut client,
        "other_graph",
        "kg scoped_graph node find Scoped",
    );
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));

    let response = run_kg_with_graph(
        &mut client,
        "other_graph",
        "kg --event-log scoped_graph node find Scoped",
    );
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("scoped_node"));
}

#[test]
fn mcp_kg_use_persists_across_script_commands() {
    let (_dir, mut client) = setup();
    for graph in ["first_graph", "second_graph"] {
        let output = run_kg_text(&mut client, &format!("kg graph create {graph}"));
        assert!(output.contains("created"), "graph creation: {}", output);
    }

    let response = run_kg(
        &mut client,
        "use first_graph; add concept:first_node --name First\nuse second_graph; add concept:second_node --name Second",
    );
    assert!(!McpClient::tool_is_error(&response));

    let response = run_kg(
        &mut client,
        "use first_graph; kg --event-log node find First",
    );
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("first_node"));

    let response = run_kg(&mut client, "use first_graph; find First");
    assert!(!McpClient::tool_is_error(&response));
    assert!(McpClient::tool_text(&response).contains("first_node"));

    let first = run_kg_text(&mut client, "kg first_graph node get concept:first_node");
    assert!(first.contains("First"), "first graph output: {}", first);
    let second = run_kg_text(&mut client, "kg second_graph node get concept:second_node");
    assert!(second.contains("Second"), "second graph output: {}", second);
}

#[test]
fn mcp_kg_schema_returns_valid_types_and_relations() {
    let (_dir, mut client) = setup();

    let response = client.call_tool("kg_schema", json!({}));

    assert!(
        !McpClient::tool_is_error(&response),
        "kg_schema should not error"
    );

    let text = McpClient::tool_text(&response);

    // Verify key node types are documented
    assert!(
        text.contains("Concept"),
        "text should contain 'Concept' type"
    );
    assert!(
        text.contains("Process"),
        "text should contain 'Process' type"
    );
    assert!(
        text.contains("DataStore"),
        "text should contain 'DataStore' type"
    );

    // Verify key relations are documented
    assert!(text.contains("HAS"), "text should contain HAS relation");
    assert!(
        text.contains("STORED_IN"),
        "text should contain STORED_IN relation"
    );
    assert!(
        text.contains("TRIGGERS"),
        "text should contain TRIGGERS relation"
    );
    assert!(
        text.contains("DEPENDS_ON"),
        "text should contain DEPENDS_ON relation"
    );
}

#[test]
fn mcp_kg_help_returns_content_for_all_domains() {
    let (_dir, mut client) = setup();

    let domains = [
        "node", "edge", "graph", "schema", "kql", "feedback", "batch", "script", "all",
    ];

    for domain in &domains {
        let response = client.call_tool("kg_help", json!({ "domain": domain }));
        assert!(
            !McpClient::tool_is_error(&response),
            "kg_help('{}') should not error",
            domain
        );

        let text = McpClient::tool_text(&response);
        assert!(
            !text.is_empty(),
            "kg_help('{}') should return non-empty text",
            domain
        );
    }
}

#[test]
fn mcp_kg_script_node_find_on_empty_graph() {
    let (_dir, mut client) = setup();

    // Create a fresh graph
    let output = run_kg_text(&mut client, "kg graph create test_graph");
    assert!(
        output.contains("created"),
        "graph creation should succeed: {}",
        output
    );

    // Find on empty graph — should return 0 results
    let output = run_kg_text(&mut client, "kg test_graph node find \"nonexistent\"");
    assert!(
        !output.contains("ERROR"),
        "find on empty graph should not error: {}",
        output
    );
}

#[test]
fn mcp_kg_script_full_crud_cycle() {
    let (_dir, mut client) = setup();

    // 1. Create graph
    let output = run_kg_text(&mut client, "kg graph create crud_graph");
    assert!(
        output.contains("created"),
        "step 1 — create graph: {}",
        output
    );

    // 2. Add node
    let script = r#"kg crud_graph node add concept:crud_test --type Concept --name "CRUD Test Node" --description "Integration test node" --domain-area testing --provenance U --confidence 0.9 --importance 0.8 --created-at "2026-06-26T12:00:00Z" --source "test fixture""#;
    let output = run_kg_text(&mut client, script);
    assert!(!output.contains("ERROR"), "step 2 — add node: {}", output);

    // 3. Find node — should find our node
    let output = run_kg_text(&mut client, r#"kg crud_graph node find "CRUD Test Node""#);
    assert!(
        output.contains("crud_test"),
        "step 3 — find node should return our node: {}",
        output
    );

    // 4. Modify node
    let output = run_kg_text(
        &mut client,
        r#"kg crud_graph node modify concept:crud_test --name "Modified CRUD Node" --description "Updated description" --domain-area testing --provenance U --confidence 0.95 --importance 0.9 --created-at "2026-06-26T12:00:00Z" --source "test update""#,
    );
    assert!(
        !output.contains("ERROR"),
        "step 4 — modify node: {}",
        output
    );

    // Verify modification by getting the node
    let output = run_kg_text(&mut client, "kg crud_graph node get concept:crud_test");
    assert!(
        output.contains("Modified CRUD Node"),
        "step 4b — verify rename: {}",
        output
    );

    // 5. Remove node
    let output = run_kg_text(&mut client, "kg crud_graph node remove concept:crud_test");
    assert!(
        !output.contains("ERROR"),
        "step 5 — remove node: {}",
        output
    );

    // Verify removal by finding (should be empty)
    let output = run_kg_text(&mut client, r#"kg crud_graph node find "CRUD""#);
    assert!(
        !output.contains("crud_test"),
        "step 5b — verify node removed: {}",
        output
    );
}

#[test]
fn mcp_kg_error_includes_detail_in_message() {
    let (_dir, mut client) = setup();

    // Create a graph first
    let output = run_kg_text(&mut client, "kg graph create error_test");
    assert!(output.contains("created"), "graph creation: {}", output);

    // Try to add a node with importance=8 (invalid — must be 0..1)
    let script = r#"kg error_test node add concept:bad_importance --type Concept --name "Bad" --description "test" --domain-area testing --provenance U --confidence 0.9 --importance 8 --created-at "2026-06-26T12:00:00Z" --source "test""#;
    let response = run_kg(&mut client, script);

    // The tool itself should report isError
    assert!(
        McpClient::tool_is_error(&response),
        "expected error for invalid importance"
    );

    let text = McpClient::tool_text(&response);
    let error_msg = McpClient::get_error_message(&response).unwrap_or(text);

    // The error message should contain the specific validation detail
    // The server formats errors as: "kg command validation error — importance must be in range 0..1, got 8"
    assert!(
        error_msg.contains("importance must be in range"),
        "error message should mention importance range constraint, got: {}",
        error_msg
    );
}

#[test]
fn mcp_feedback_auto_skip_on_high_score() {
    let (_dir, mut client) = setup();

    // Create graph
    let output = run_kg_text(&mut client, "kg graph create feedback_graph");
    assert!(output.contains("created"), "graph creation: {}", output);

    // Add a node with a distinctive name (will be exact-match-found)
    let script = r#"kg feedback_graph node add concept:very_specific --type Concept --name "ultra_specific_thing_xyz_123" --description "Unique test node" --domain-area testing --provenance U --confidence 0.95 --importance 0.9 --created-at "2026-06-26T12:00:00Z" --source "test""#;
    let output = run_kg_text(&mut client, script);
    assert!(!output.contains("ERROR"), "add node: {}", output);

    // Add a second node so total > 1 (avoids total<=1 auto-skip)
    let script = r#"kg feedback_graph node add concept:other_node --type Concept --name "Some Other Thing" --description "Another node" --domain-area testing --provenance U --confidence 0.5 --importance 0.3 --created-at "2026-06-26T12:00:00Z" --source "test""#;
    let output = run_kg_text(&mut client, script);
    assert!(!output.contains("ERROR"), "add second node: {}", output);

    // Find with exact match on the distinctive name
    // The exact match should produce a high BM25 score (>= 800 threshold)
    let output = run_kg_text(
        &mut client,
        r#"kg --event-log feedback_graph node find "ultra_specific_thing_xyz_123""#,
    );

    // When feedback is auto-skipped (top_score >= 800), no NUDGE line should appear
    assert!(
        !output.contains("NUDGE"),
        "auto-skipped feedback should not include a NUDGE line, got:\n{}",
        output
    );

    // The node should appear in results
    assert!(
        output.contains("very_specific"),
        "find should return our specific node, got:\n{}",
        output
    );
}

#[test]
fn mcp_event_log_preserves_feedback_and_passive_resolution() {
    let (_dir, mut client) = setup();

    let output = run_kg_text(&mut client, "kg graph create event_feedback_graph");
    assert!(output.contains("created"), "graph creation: {}", output);
    for (id, name) in [
        ("concept:event_first", "event_first_match"),
        ("concept:event_second", "event_second_match"),
    ] {
        let script = format!(
            "kg event_feedback_graph node add {id} --type Concept --name \"{name}\" --description \"event feedback test\" --domain-area testing --provenance U --confidence 0.9 --importance 0.8 --created-at \"2026-06-26T12:00:00Z\" --source \"test\""
        );
        let output = run_kg_text(&mut client, &script);
        assert!(!output.contains("ERROR"), "node add: {}", output);
    }

    let response = run_kg(
        &mut client,
        "kg --event-log event_feedback_graph node find event --with-feedback; kg --event-log event_feedback_graph node get concept:event_first",
    );
    assert!(!McpClient::tool_is_error(&response));
    let structured = response["result"]
        .get("structuredContent")
        .or_else(|| response["result"].get("structured_content"))
        .expect("structured content");
    assert!(
        structured["steps"]
            .as_array()
            .is_some_and(|steps| { steps.iter().any(|step| step["kind"] == "passive_feedback") })
    );
}
