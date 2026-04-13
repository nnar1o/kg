# kg v0.2.12

`kg-mcp` is a lightweight local alternative to classic RAG systems when you want structured, editable, git-friendly project memory instead of document chunk retrieval.

This release focuses on CLI correctness and smoother graph authoring defaults: stats no longer include internal metadata nodes, and the documented minimal `node add` command now works out of the box.

## Highlights

- exclude internal `^:graph_info` metadata nodes from `kg graph <graph> stats` counts and type breakdowns
- make minimal `kg graph <graph> node add <id> --type <Type> --name <Name>` apply safe metadata defaults
- keep validation guarantees while removing the UX mismatch between optional-looking flags and runtime-required metadata
- add integration coverage for the minimal `node add` path and stats behavior

## Why this release matters

- `stats` now reflects user-authored graph content instead of internal bookkeeping nodes
- onboarding and docs-aligned `node add` commands now work without trial-and-error metadata flags
- regressions are covered with smoke/integration tests to keep these paths stable

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
