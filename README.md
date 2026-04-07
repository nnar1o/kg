# kg — local knowledge graph CLI
<img width="434" height="369" alt="image" src="https://github.com/user-attachments/assets/f53bf36f-ac6e-4f83-afaf-00ea9ef12b7e" />

![CI](https://img.shields.io/github/actions/workflow/status/nnar1o/kg/ci.yml?branch=master)
![Release](https://img.shields.io/github/v/release/nnar1o/kg?display_name=tag&sort=semver)
![License](https://img.shields.io/badge/License-MIT-green.svg)

> **Beta** - This software is in active development. APIs may change.

A fast CLI for managing local knowledge graphs (`.kg` and `.json`) with native MCP server support. Built for LLM chat workflows: readable text by default, `--json` for automation.

## Why kg

- **One local graph per domain** - store concepts, processes, rules, bugs, decisions
- **Fast lookup and search** - fuzzy, BM25, vector modes
- **Quality controls** - checks, audits, missing facts/descriptions, duplicate detection
- **LLM integration** - native MCP server (`kg-mcp`) with graph-safe tools
- **Flexible I/O** - import/export CSV, JSON, Markdown, KQL queries
- **Backward compatible runtime** - legacy JSON mode via `--legacy`

## Install

### Option 1: quick installer script (recommended for end users)

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

The installer auto-detects Linux x86_64, macOS x86_64, and macOS Apple Silicon releases.

### Option 2: install from crates.io

```sh
cargo install kg-cli
```

### Option 3: install from source (recommended for contributors)

```sh
cargo install --path .
```

## 60-second quick start

```sh
# 1) Create graph
kg create fridge

# 2) Add two nodes
kg graph fridge node add concept:refrigerator --type Concept --name "Refrigerator"
kg graph fridge node add process:defrost --type Process --name "Defrost cycle"

# 3) Connect them
kg graph fridge edge add concept:refrigerator DEPENDS_ON process:defrost --detail "requires periodic defrost"

# 4) Search and inspect
kg graph fridge node find refrigerator
kg graph fridge node get concept:refrigerator --full

# 5) Validate quality
kg graph fridge check
kg graph fridge quality missing-facts
```

See `kg --help` and `kg graph --help` for the full command tree.

For a more complete onboarding flow, go to [`docs/getting-started.md`](docs/getting-started.md).

## Command patterns (important)

Most commands use this structure:

```sh
kg graph <graph-name> <command> [args...]
```

Examples:

```sh
kg graph fridge stats
kg graph fridge node list --type Concept --limit 20
kg graph fridge kql "node type=Concept sort=name limit=10"
```

There is also a backward-compatible shorthand still accepted in many places:

```sh
kg fridge node find cooling
```

## MCP Server

The primary way to integrate with LLMs:

```sh
./target/release/kg-mcp
```

`kg-mcp` uses stdio transport (no HTTP server to expose).

Config for OpenCode/Claude Desktop:

```json
{
  "mcpServers": {
    "kg": {
      "command": "/path/to/kg-mcp"
    }
  }
}
```

Primary tools: `kg`, `kg_command`, `kg_create_graph`, `kg_schema`, `kg_node_find`, `kg_node_get`, `kg_node_add`, `kg_node_add_batch`, `kg_node_modify`, `kg_node_remove`, `kg_edge_add`, `kg_edge_add_batch`, `kg_edge_remove`, `kg_stats`, `kg_feedback`, `kg_feedback_batch`, and `kg_gap_summary`. Compatibility tools such as `kg_check`, `kg_audit`, `kg_quality`, `kg_export_html`, `kg_access_log`, and `kg_access_stats` remain available but are deprecated in favor of the `kg` script tool.

When creating edges through MCP, call `kg_schema` first to inspect valid relations, allowed source/target types, and ID prefixes. `kg_edge_add_batch` also supports `dry_run=true` for preflight validation before writing changes.

See [`docs/mcp.md`](docs/mcp.md) for full docs.

## Common workflows

```sh
# List available graphs
kg list --full

# Show graph health quickly
kg graph fridge stats --by-type --by-relation
kg graph fridge check --errors-only

# Use strict parser checks for .kg files (optional)
KG_STRICT_FORMAT=1 kg graph fridge check

# Keep JSON-first behavior for older pipelines
kg graph fridge --legacy stats
```

## Documentation

Detailed guides in [`docs/`](docs/):

- [`docs/getting-started.md`](docs/getting-started.md) - beginner guide with practical examples
- [`docs/build-graph-from-docs.md`](docs/build-graph-from-docs.md) - step-by-step playbook to build a graph from raw documentation
- [`docs/ai-prompt-graph-from-docs.md`](docs/ai-prompt-graph-from-docs.md) - ready-to-use AI prompt for `kg-mcp` graph construction
- [`docs/troubleshooting.md`](docs/troubleshooting.md) - common problems and fixes
- [`docs/sprint-plan.md`](docs/sprint-plan.md) — roadmap
- [`docs/kql.md`](docs/kql.md) — KQL query language
- [`docs/import-csv.md`](docs/import-csv.md) — CSV import
- [`docs/import-markdown.md`](docs/import-markdown.md) — Markdown import
- [`docs/mcp.md`](docs/mcp.md) — MCP server reference
- [`docs/decision-backend.md`](docs/decision-backend.md) — backend selection
- [`docs/eyg-rollout-notes.md`](docs/eyg-rollout-notes.md) - migration, strict mode, rollback notes

## Project skills (for AI workflows)

If your AI client supports repo skills, these templates are available:

- [`skills/kg/SKILL.md`](skills/kg/SKILL.md) - core read/write graph operations
- [`skills/kg-builder/SKILL.md`](skills/kg-builder/SKILL.md) - build graph from docs/code/specs
- [`skills/kg-assistant/SKILL.md`](skills/kg-assistant/SKILL.md) - collaborative graph improvement with user
- [`skills/kg-gardener/SKILL.md`](skills/kg-gardener/SKILL.md) - graph quality and maintenance workflow

## FAQ

### Should I use `kg graph <name> ...` or `kg <name> ...`?

Use `kg graph <name> ...` as the default pattern. The shorthand `kg <name> ...` is kept for backward compatibility.

### Why did my graph file change from `.json` to `.kg`?

Default runtime prefers `.kg` and can auto-migrate from `.json` side-by-side. Your original `.json` file is kept.

### What are `.kgindex` and `.kglog` files?

They are sidecars for `.kg` graphs: `.kgindex` helps fast node lookup, `.kglog` stores lightweight hit/feedback events.

### How do I keep old JSON-first behavior?

Run graph commands with `--legacy`, for example: `kg graph fridge --legacy stats`.

### A command fails with validation errors. What now?

Run `kg graph <graph> check --errors-only` first, fix the reported node/edge issues, then retry your command.

### Which output format should I use in scripts?

Use `--json` in automation and CI. Use default text output for interactive local usage.

## Need help fast?

If users are unsure how to start, share these three commands first:

```sh
kg --help
kg graph --help
kg graph <graph> node --help
```

Then follow [`docs/getting-started.md`](docs/getting-started.md).

## Benchmarks (large graphs)

Generate a big synthetic graph JSON:

```sh
cargo run --release --example generate_large_graph -- --out ./big.json --nodes 100000 --edges-per-node 5
```

Run criterion benchmarks (sizes configurable via env vars):

```sh
KG_BENCH_NODES=20000 KG_BENCH_EDGES_PER_NODE=5 cargo bench --bench large_graph
```

Other benches:

```sh
# End-to-end CLI benchmarks (spawns `kg`, covers JSON + persisted BM25 index + redb backend)
KG_BENCH_NODES=20000 KG_BENCH_EDGES_PER_NODE=5 cargo bench --bench cli_e2e

# Persisted BM25 index benchmarks (build/save/load)
KG_BENCH_NODES=20000 KG_BENCH_EDGES_PER_NODE=5 cargo bench --bench persistence
```
