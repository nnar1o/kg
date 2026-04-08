# kg - local knowledge graph for your AI assistant

<img width="434" height="369" alt="image" src="https://github.com/user-attachments/assets/f53bf36f-ac6e-4f83-afaf-00ea9ef12b7e" />

![CI](https://img.shields.io/github/actions/workflow/status/nnar1o/kg/ci.yml?branch=master)
![Release](https://img.shields.io/github/v/release/nnar1o/kg?display_name=tag&sort=semver)
![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)

> **Beta** - APIs may still change and some bugs are still expected.

# kg — local project memory for AI assistants

`kg` gives your AI assistant persistent, structured, editable project memory stored locally as a knowledge graph.

Instead of relying only on document chunk retrieval, you can keep architecture, decisions, incidents, rules, dependencies, and workflows in a graph that is readable, reviewable, and Git-friendly.

Use it when you want your assistant to understand an existing project across sessions — not start from zero every time.

## Why use it

- **Persistent memory** — keep project knowledge between conversations
- **Structured, not fuzzy** — inspect nodes, edges, facts, and gaps directly
- **Editable and reviewable** — store graphs as `*.kg` files with readable diffs
- **Local-first** — your project memory stays on your machine
- **Works with MCP clients** — connect it as a local stdio MCP server

## Why not just RAG

Classic RAG is good for retrieving text chunks from documents.

`kg-mcp` is better when you want:
- stable project memory instead of repeated retrieval
- explicit facts, relations, and dependencies
- graph updates during real work with the assistant
- something you can inspect, version, diff, and improve over time

## Installation

Recommended install:

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

You can also download a ready binary from GitHub Releases.

## Connect `kg-mcp` to Your AI Client

Add `kg-mcp` as a local stdio MCP server.

Example config:

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

After that:

1. restart your AI client,
2. confirm the `kg` MCP server is available,
3. start using the prompts below.

Full MCP setup and reference: [`docs/mcp.md`](docs/mcp.md)

## Generate a Graph

This is the first workflow for a new project: ask the assistant to create or extend a graph from your documentation.

By default, graphs are stored in `~/.kg/graphs` as `*.kg` files.

Minimal prompt:

```text
You are connected to kg-mcp.

Project graph name: payments

Build or extend this graph from the project documentation I provide.
Use `payments` as the graph name for all graph operations.

Only add facts grounded in source material.
If an important fact is missing and can be inferred safely from the provided docs, update the graph.
If something is ambiguous, ask or record it as a note instead of inventing facts.
```

Example prompt with documents:

```text
Use kg-mcp to build or extend the `payments` graph from these documents:
- docs/payments/overview.md
- docs/payments/retries.md
- docs/payments/providers.md

Only add facts grounded in the documents.
If something is ambiguous, keep it out of the graph or record it as a note.
When you finish, summarize what was added, what remains unclear, and what document should be ingested next.
```

Longer prompt for this workflow: [`docs/ai-prompt-graph-from-docs.md`](docs/ai-prompt-graph-from-docs.md)

## Ask the Assistant About Facts in the Graph

Once the graph exists, the normal workflow is to ask the assistant to inspect it and answer questions from it.

Example prompt:

```text
Use kg-mcp to inspect my existing `payments` graph.

I want to understand:
- how payment authorization works,
- what triggers retries,
- which external providers are involved,
- which datastore reads and writes are part of the flow.

If the graph is missing critical information, say exactly what is missing.
```

Other useful questions:

- "What rules control retries in the `payments` graph?"
- "Which systems write to the orders datastore?"
- "What is missing or weak in this graph?"
- "Which nodes and edges explain the authorization flow?"

## Add or Update Facts Through the Assistant

You can also ask the assistant to improve the graph while you work.

Example prompt:

```text
Use kg-mcp to review my existing `payments` graph.

Find:
- missing important nodes,
- weak descriptions,
- missing facts,
- suspicious or low-value edges.

Apply safe improvements where possible.
Only add facts grounded in the graph, the provided docs, or the current discussion.
If something is ambiguous, leave it out or add a note.

When you finish, summarize:
- what was wrong,
- what you changed,
- what still needs manual review.
```

This works best when your main system prompt or project prompt already tells the assistant which graph belongs to the project.

Minimal project-level prompt:

```text
You are connected to kg-mcp.
Project graph name: payments.
Use this graph for relevant reads and updates in this project.
If you notice important missing information that is grounded in the available docs or conversation context, update the graph as part of your work.
If uncertain, ask or add a note instead of inventing facts.
```

## Tips

### Keep Graphs in Git

The default graph directory is `~/.kg/graphs`.

You can put that directory under git.

Recommended approach:

- keep the main `*.kg` graph files in git,
- ignore generated sidecars and local operational files,
- treat backup snapshots and event logs as local machine history unless you explicitly want to version them.

Suggested `.gitignore`:

```gitignore
*.kgindex
*.event.log
*.migration.log
*.bak
*.bck.*.gz
```

In practice:

- `*.kg` is the main graph file you usually want to review and commit,
- `*.kgindex` is a generated local index,
- `*.event.log` is a local append-only change timeline,
- `*.bak` is the previous on-disk version from the last write,
- `*.bck.*.gz` are periodic compressed backup snapshots,
- `*.migration.log` is a migration report when older graphs are converted.

`*.kg` is git-friendly and intentionally structured to make diffs readable and merges easier when several people work on the same graph.

### Export a Graph to HTML

To generate an interactive HTML view of a graph:

```sh
kg graph payments export-html --output payments.html
```

You can keep the generated HTML as a shareable visual snapshot of the current graph.

## Documentation

- [`docs/mcp.md`](docs/mcp.md) - MCP setup and tool reference
- [`docs/ai-prompt-graph-from-docs.md`](docs/ai-prompt-graph-from-docs.md) - longer prompt for document ingestion
- [`docs/build-graph-from-docs.md`](docs/build-graph-from-docs.md) - graph-building workflow from docs
- [`docs/troubleshooting.md`](docs/troubleshooting.md) - common issues

## Contact

For questions or feedback: `nnar10@proton.me`
