# MCP Server Reference

`kg-mcp` is a native MCP stdio server built with [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk). It exposes all kg operations as MCP tools.

If you are new to MCP, start with `kg-mcp` as a local stdio process (no network service needed).

## Quick start

1. Ensure `kg-mcp` is installed and runnable.
2. Add it to your MCP client config.
3. Restart the client and verify tools appear.
4. Run a simple tool call (`kg_node_find` or `kg`).

For edge work, start with `kg_schema` so you can see valid relations, allowed source/target types, and ID prefixes before mutating the graph.

Example first command through MCP shell tool:

```text
graph fridge node find refrigerator
```

## Running

```sh
./target/release/kg-mcp    # binary
cargo run --bin kg-mcp      # from source
```

Tip: during local development, prefer `cargo run --quiet --bin kg-mcp` in client config.

## Client Config

### OpenCode / Claude Desktop

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

### Development mode

```json
{
  "mcpServers": {
    "kg": {
      "command": "cargo",
      "args": ["run", "--quiet", "--bin", "kg-mcp"]
    }
  }
}
```

## Tools

### Shell-like

- `kg` — run multiple commands separated by `;` or newlines

Good for compact workflows, for example:

```text
create fridge; graph fridge node add concept:refrigerator --type Concept --name Refrigerator
```

The shell tool passes normal CLI flags through unchanged, including edge details such as `--detail "requires periodic defrost"`.

### Core tools

- `kg_command` — run one CLI command by passing argv-style arguments
- `kg_create_graph` — create a new graph
- `kg_schema` — inspect valid node types, relations, ID prefixes, and edge rules
- `kg_stats` — graph statistics
- `kg_gap_summary` — run a bundled quality sweep for collaborative cleanup

### Nodes

- `kg_node_find` — search nodes by query
- `kg_node_get` — get node by ID
- `kg_node_add` — create single node
- `kg_node_add_batch` — create multiple nodes, with `mode=atomic|best_effort` and optional `on_conflict=skip`
- `kg_node_modify` — update node fields
- `kg_node_remove` — delete node

### Edges

- `kg_edge_add` — create edge
- `kg_edge_add_batch` — create multiple edges, with `mode=atomic|best_effort` and optional `dry_run=true` preflight
- `kg_edge_remove` — delete edge

Example batch preflight:

```json
{
  "graph": "fridge",
  "mode": "best_effort",
  "dry_run": true,
  "edges": [
    {
      "source_id": "process:defrost",
      "relation": "AVAILABLE_IN",
      "target_id": "interface:smart_api",
      "detail": "Proces rozmrazania dostepny z API"
    }
  ]
}
```

### Graph

- `kg_check` — deprecated compatibility tool for integrity validation
- `kg_audit` — deprecated compatibility tool for deep audit (errors/warnings)
- `kg_quality` — deprecated compatibility tool for quality subcommands
- `kg_export_html` — deprecated compatibility tool for HTML export

### Feedback

- `kg_feedback` — record YES/NO/PICK feedback
- `kg_feedback_batch` — batch feedback

### Access

- `kg_access_log` — deprecated compatibility tool for search access history
- `kg_access_stats` — deprecated compatibility tool for access statistics

### Passthrough

- `kg_command` — run arbitrary kg CLI commands

## Resources

- `kg://cwd` — current working directory
- `kg://graphs` — discovered graph files
- `kg://graph/{name}` — graph stats summary

## Prompts

- `kg_workflow_prompt` — template for planning safe multi-step edits
- Ready-to-copy ingestion prompt: [`docs/ai-prompt-graph-from-docs.md`](ai-prompt-graph-from-docs.md)

## Common issues

- Tools not visible in client: verify command path and restart the client process.
- Feedback-required flow in some operations: follow tool response metadata and submit required feedback before continuing.
- Unexpected graph format behavior: check whether runtime is using default `.kg` mode or `--legacy` JSON mode.

See also: [`docs/troubleshooting.md`](troubleshooting.md)
