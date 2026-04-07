# kg — local knowledge graph CLI
<img width="434" height="369" alt="image" src="https://github.com/user-attachments/assets/f53bf36f-ac6e-4f83-afaf-00ea9ef12b7e" />

![CI](https://img.shields.io/github/actions/workflow/status/nnar1o/kg/ci.yml?branch=master)
![Release](https://img.shields.io/github/v/release/nnar1o/kg?display_name=tag&sort=semver)
![License](https://img.shields.io/badge/License-MIT-green.svg)

> **Beta** - The tool is usable, but command details may still change.

`kg` helps you build a local knowledge graph from project docs, architecture notes, incidents, and system knowledge.

Use it when you want to:

- keep one searchable graph per domain, project, or incident,
- inspect concepts, processes, rules, bugs, decisions, and dependencies,
- validate graph quality instead of storing loose notes,
- let an AI assistant read and update the graph safely through MCP.

The default UX is for humans: readable terminal output, local files, no external service required.

## Install

### Option 1: installer script

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

### Option 2: crates.io

```sh
cargo install kg-cli
```

### Option 3: from source

```sh
cargo install --path .
```

Check installation:

```sh
kg --version
kg --help
```

## Quick start

Create a graph:

```sh
kg create fridge
```

Add a few nodes:

```sh
kg graph fridge node add concept:refrigerator --type Concept --name "Refrigerator" --description "Cooling appliance" --importance 5
kg graph fridge node add process:defrost --type Process --name "Defrost cycle" --description "Periodic ice removal cycle" --importance 4
```

Connect them:

```sh
kg graph fridge edge add concept:refrigerator DEPENDS_ON process:defrost --detail "requires periodic defrost"
```

Search and inspect:

```sh
kg graph fridge node find refrigerator
kg graph fridge node get concept:refrigerator --full
kg graph fridge stats --by-type --by-relation
```

Check graph quality:

```sh
kg graph fridge check
kg graph fridge quality missing-descriptions
kg graph fridge quality missing-facts
```

Main command pattern:

```sh
kg graph <graph-name> <command> [args...]
```

Examples:

```sh
kg graph fridge node find refrigerator --output-size 1200
kg graph fridge kql "node type=Concept sort=name limit=10"
kg graph fridge history
```

## Build a graph from documentation

There are two practical ways to do this.

### Option 1: import prepared Markdown files

If your docs are already normalized into Markdown files with YAML frontmatter, you can import them directly.

Example file:

```md
---
id: concept:refrigerator
type: Concept
name: Refrigerator
description: Cooling appliance
provenance: docs
key_facts: ["Keeps food cold", "Has freezer compartment in some models"]
source_files: [manual.md]
---
```

Import:

```sh
kg create docs-demo
kg graph docs-demo import-md --path ./docs
kg graph docs-demo check --errors-only
```

See full format: [`docs/import-markdown.md`](docs/import-markdown.md)

### Option 2: use an LLM with `kg-mcp` on raw docs

This is the better option when you have raw architecture docs, specs, ADRs, runbooks, or mixed notes and want the AI to convert them into a clean graph.

1. Start the MCP server:

```sh
kg-mcp
```

2. Connect your AI client to `kg-mcp`.

Minimal config example:

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

3. Create an empty graph:

```sh
kg create payments
```

4. Give your AI assistant the source documents and an explicit ingestion instruction.

Example prompt for an LLM connected to `kg-mcp`:

```text
Build a knowledge graph from my project documentation using kg-mcp.

Graph name: payments
Scope: payment flow, retry rules, integrations, and datastore dependencies.
Sources:
- docs/payments/overview.md
- docs/payments/retries.md
- docs/payments/integrations.md

Rules:
1. Only add facts grounded in the provided documents.
2. Use stable IDs in the form <type>:<snake_case_name>.
3. Prefer a smaller correct graph over a larger speculative graph.
4. Work in batches of at most 10 nodes.
5. After each batch run:
   - kg graph payments check --errors-only
   - kg graph payments quality missing-descriptions
   - kg graph payments quality missing-facts
   Fix issues before continuing.
6. For each node include type, name, description, importance, and source reference when available.
7. Use notes for assumptions, not hard facts.
8. Do not delete existing nodes or edges unless I ask.

Workflow:
- First show me the extraction plan: candidate nodes, candidate edges, and ambiguous items.
- Wait for approval.
- Then create or update the graph batch by batch.
- At the end run:
  - kg graph payments stats --by-type --by-relation
  - kg graph payments node find "retry"
  - kg graph payments node get <2-3 critical node ids> --full
- Report what was added, remaining quality gaps, and which docs should be ingested next.
```

If you want a longer ready-to-copy version, use [`docs/ai-prompt-graph-from-docs.md`](docs/ai-prompt-graph-from-docs.md).

If you want the full manual playbook, use [`docs/build-graph-from-docs.md`](docs/build-graph-from-docs.md).

## Everyday commands

```sh
# list graphs
kg list --full

# inspect graph health
kg graph payments stats --by-type --by-relation
kg graph payments check --errors-only

# inspect nodes
kg graph payments node find "retry policy"
kg graph payments node get process:authorize_payment --full

# query with KQL
kg graph payments kql "node type=Process sort=name limit=20"

# export/import helpers
kg graph payments export-json --help
kg graph payments import-csv --help
kg graph payments import-md --help
```

## AI / MCP usage

`kg-mcp` exposes the graph to an AI assistant over stdio, so the assistant can search, read, add, and validate graph data without direct shell access.

See full MCP reference: [`docs/mcp.md`](docs/mcp.md)

## FAQ

### Should I use `kg graph <name> ...` or `kg <name> ...`?

Use `kg graph <name> ...` as the default pattern. The shorter form is only for backward compatibility.

### What file format does `kg` use?

Default runtime prefers `.kg`. Older `.json` graphs can still be read, and the runtime may migrate them side by side.

### What are `.kgindex` and `.kglog`?

They are helper sidecars for `.kg` graphs. `kgindex` speeds up node lookup and `kglog` stores lightweight search/feedback events.

### Which output should I use in scripts?

Use `--json` in scripts and CI. Use the default text output in interactive terminal work.

### A command fails with validation errors. What should I do?

Run:

```sh
kg graph <graph-name> check --errors-only
```

Fix the reported node or edge issues, then rerun your original command.

## Documentation

- [`docs/getting-started.md`](docs/getting-started.md) - first-use guide
- [`docs/build-graph-from-docs.md`](docs/build-graph-from-docs.md) - docs to graph playbook
- [`docs/ai-prompt-graph-from-docs.md`](docs/ai-prompt-graph-from-docs.md) - ready prompt for LLM ingestion
- [`docs/import-markdown.md`](docs/import-markdown.md) - Markdown import format
- [`docs/import-csv.md`](docs/import-csv.md) - CSV import format
- [`docs/kql.md`](docs/kql.md) - query language
- [`docs/mcp.md`](docs/mcp.md) - MCP server reference
- [`docs/troubleshooting.md`](docs/troubleshooting.md) - common issues

## Contact

For questions or feedback: `nnar10@proton.me`
