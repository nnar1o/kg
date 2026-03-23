# MCP Server Reference

`kg-mcp` is a native MCP stdio server built with [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk). It exposes all kg operations as MCP tools.

## Running

```sh
./target/release/kg-mcp    # binary
cargo run --bin kg-mcp      # from source
```

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

### Nodes

- `kg_node_find` — search nodes by query
- `kg_node_get` — get node by ID
- `kg_node_add` — create single node
- `kg_node_add_batch` — create multiple nodes
- `kg_node_modify` — update node fields
- `kg_node_remove` — delete node

### Edges

- `kg_edge_add` — create edge
- `kg_edge_remove` — delete edge

### Graph

- `kg_stats` — graph statistics
- `kg_check` — integrity validation
- `kg_audit` — deep audit (errors/warnings)
- `kg_quality` — quality gaps (missing descriptions, facts, duplicates, edge gaps)
- `kg_export_html` — interactive HTML visualization

### Feedback

- `kg_feedback` — record YES/NO/PICK feedback
- `kg_feedback_batch` — batch feedback

### Access

- `kg_access_log` — search access history
- `kg_access_stats` — access statistics

### Passthrough

- `kg_command` — run arbitrary kg CLI commands

## Resources

- `kg://cwd` — current working directory
- `kg://graphs` — discovered graph files
- `kg://graph/{name}` — graph stats summary

## Prompts

- `kg_workflow_prompt` — template for planning safe multi-step edits
