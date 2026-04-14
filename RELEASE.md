# kg v0.2.14

`kg-mcp` now handles batch writes much closer to single-command behavior and reports failures in a way that is actionable for MCP clients.

This release focuses on reliability and diagnostics for batch node/feedback operations.

## Highlights

- apply schema validation in `kg_node_add_batch`, matching single `node add` expectations
- normalize explicit batch `sources` using the same rules as single add
- fix feedback batch accounting so graph update failures are counted and reported as real failures
- standardize batch summaries to `OK`/`ERROR` with per-item failure lines (no more confusing `Error: OK (...)`)

## Why this release matters

- batch and single-node add behavior is now consistent for schema-enforced projects
- MCP clients can show precise failure diagnostics without losing successful items
- operators can trust batch feedback status counters when partial graph updates fail

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

You can also download a release binary from GitHub Releases.

## Quick start

1. Add `kg-mcp` to your MCP client.
2. Restart the client.
3. Tell the assistant which graph belongs to the project.
4. Ask it to build or extend the graph from your docs.
5. Keep using the same graph in later conversations.

Minimal project prompt:

```text
You are connected to kg-mcp.
Project graph name: payments.
Use this graph for relevant reads and updates in this project.
If you notice important missing information that is grounded in the available docs or conversation context, update the graph as part of your work.
If uncertain, ask or add a note instead of inventing facts.
```
