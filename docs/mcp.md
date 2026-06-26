# MCP Server Reference

`kg-mcp` is a native MCP stdio server built with [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk). It exposes all kg operations as MCP tools.

If you are new to MCP, start with `kg-mcp` as a local stdio process (no network service needed).

## Quick start

1. Ensure `kg-mcp` is installed and runnable.
2. Add it to your MCP client config.
3. Restart the client and verify tools appear.
4. Run a simple tool call: `kg_schema` to inspect valid types/relations, `kg_help` with a domain for examples, then `kg` to execute.

Start with `kg_schema` so you can see valid relations, allowed source/target types, and ID prefixes before mutating the graph. Call `kg_help <domain>` (domains: `node`, `edge`, `graph`, `schema`, `kql`, `feedback`, `batch`, `script`, `all`) for detailed syntax and examples.

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

## MCP Registry Publishing

This repository includes a top-level `server.json` for `mcp-publisher` and publishes MCP Registry metadata from version tags via `.github/workflows/publish-mcp.yml`.

Because the official registry does not currently support Cargo crates as a package type, the published entry currently uses repository metadata plus `websiteUrl` instead of pretending that `kg-mcp` is an `npm` or `pypi` package.

Installation stays the same:

- GitHub release binaries
- `install.sh`
- local `cargo run --bin kg-mcp` during development

## Tools (3)

All kg operations are exposed through **3 tools**. The `kg` script runner handles node/edge CRUD, graph management, stats, audit, and feedback. `kg_help` provides on-demand documentation with examples. `kg_schema` returns the live schema.

### `kg` — Script runner

Execute one or more kg commands separated by `;` or newlines. Lines starting with `#` are comments. Feedback lines (`uid=...`) are buffered and flushed before the next non-feedback command.

Parameters: `script` (string), `mode` (`best_effort`|`strict`, default `best_effort`), `debug` (bool, optional).

```text
graph fridge node find refrigerator --output-size 1200; uid=abc123 YES; graph fridge node get concept:refrigerator --full
```

The shell tool passes normal CLI flags through unchanged. For read/search flows it mirrors the CLI surface for `node find`, `node get`, and `kql`, including `--output-size`, `--limit`, `--mode`, and `--full`.

### `kg_help` — Dynamic documentation

Returns detailed manual with examples for a given domain. Use this before unfamiliar operations.

Parameters: `domain` (string).

| Domain | Covers |
|--------|--------|
| `node` | find, get, add, modify, remove, batch add — field reference, source formats, provenance |
| `edge` | add, batch add (dry-run), remove — relation dictionary |
| `graph` | create, stats, check, audit, quality, gap-summary |
| `schema` | node types, ID format, edge rules, G-prefix conventions |
| `kql` | KQL query language syntax and examples |
| `feedback` | YES/NO/NIL/PICK lines, passive feedback, feedback-required flow |
| `batch` | node/edge batch modes (atomic vs best_effort), on_conflict=skip, dry-run |
| `script` | multi-command syntax, separators, comments, mode selection |
| `all` | complete reference (all domains combined) |

### `kg_schema` — Live schema

Returns valid node types, relations, ID prefixes, and edge rules. No parameters.

### Usage flow

```
kg_schema()                              → discover valid types/relations
kg_help(domain="node")                   → learn node CRUD with examples
kg(script="fridge node find 'compressor'") → execute
```

## Resources

- `kg://cwd` — current working directory
- `kg://graphs` — discovered graph files
- `kg://graph/{name}` — graph stats summary

## Prompts

- `kg_workflow_prompt` — template for planning safe multi-step edits
- Ready-to-copy ingestion prompt: [`docs/ai-prompt-graph-from-docs.md`](ai-prompt-graph-from-docs.md)

## Common issues

- Tools not visible in client: verify command path and restart the client process.
- Feedback-required flow in some operations: follow `structured_content.requires_feedback` in `kg` responses and submit feedback via `uid=...` lines in the next `kg` script call (see `kg_help domain="feedback"`).
- Unexpected graph format behavior: check whether runtime is using default `.kg` mode or `--legacy` JSON mode.

See also: [`docs/troubleshooting.md`](troubleshooting.md)
