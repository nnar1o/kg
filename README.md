# kg — local knowledge graph CLI

![CI](https://img.shields.io/github/actions/workflow/status/nnar1o/kg/ci.yml?branch=master)
![Release](https://img.shields.io/github/v/release/nnar1o/kg?display_name=tag&sort=semver)
![License](https://img.shields.io/badge/License-MIT-green.svg)

> **Beta** — This software is in active development. APIs may change.

A fast CLI for managing JSON knowledge graphs with native MCP server support. Built for LLM chat workflows: plain text by default, `--json` for machines.

## Features

- **Graph operations** — create, query, merge, diff, backup
- **Search** — full-text and BM25 ranking
- **MCP server** — first-class tool integration for AI assistants
- **TUI browser** — interactive graph exploration
- **Quality analysis** — find missing descriptions, facts, edge gaps
- **Import** — CSV, JSON, Markdown/YAML frontmatter

## Install

### Quick (curl)

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/main/install.sh | sh
```

### From source

```sh
cargo install --path . --locked --root ~/.local
export PATH="$HOME/.local/bin:$PATH"
```

Or build manually: `cargo build --release`

## Quick start

```sh
kg init                           # print init prompts
kg create mygraph                 # create graph
kg mygraph node find query        # search
kg mygraph node get id             # view node
kg mygraph node add id --type Concept --name "Label"
kg mygraph edge add source REL target
kg mygraph quality missing-facts  # quality check
```

See `kg --help` for all commands.

## MCP Server

The primary way to integrate with LLMs:

```sh
./target/release/kg-mcp
```

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

Tools: node find/get/add/modify/remove, edge add/remove, stats, check, audit, quality, export-html, feedback, and a shell-like `kg` tool for multi-command scripts.

See [`docs/mcp.md`](docs/mcp.md) for full docs.

## Documentation

Detailed guides in [`docs/`](docs/):

- [`docs/sprint-plan.md`](docs/sprint-plan.md) — roadmap
- [`docs/kql.md`](docs/kql.md) — KQL query language
- [`docs/import-csv.md`](docs/import-csv.md) — CSV import
- [`docs/import-markdown.md`](docs/import-markdown.md) — Markdown import
- [`docs/mcp.md`](docs/mcp.md) — MCP server reference
- [`docs/decision-backend.md`](docs/decision-backend.md) — backend selection
