# kg v0.1.2

First release.

## What's included

- **CLI** — Full-featured CLI for managing JSON knowledge graphs
- **MCP Server** — Native MCP stdio server for AI tool integration
- **TUI Browser** — Interactive terminal graph browser

## Features

- Graph CRUD operations (create, read, update, delete)
- Full-text and BM25 search
- Graph merge, diff, backup, timeline
- CSV, JSON, Markdown import
- Quality analysis (missing descriptions, facts, edge gaps)
- Structured feedback system

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/main/install.sh | sh
```

Or build from source: `cargo install --path .`
