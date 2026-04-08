# kg v0.2.2

`kg-mcp` is a lightweight local alternative to classic RAG systems when you want structured, editable, git-friendly project memory instead of document chunk retrieval.

This release tightens the end-user workflow around the current `kg-mcp` model: connect your AI client, build or extend a graph from docs, ask questions against the existing graph, and keep the graph itself under git while operational sidecars stay local.

## Highlights

- MCP-first end-user documentation for `kg-mcp`
- Apache 2.0 license
- local graph storage in `~/.kg/graphs`
- git-friendly `*.kg` format designed for readable diffs and easier merges
- recommended git ignores for `*.kgindex`, `*.event.log`, `*.migration.log`, `*.bak`, and `*.bck.*.gz`
- prompts and documentation for generating graphs from documentation, asking the assistant about facts already in the graph, and updating graph facts safely through the assistant
- default `kg --help` output without the ASCII logo
- interactive HTML export for graph visualization

## Documentation updates in this release

- README rewritten as an end-user guide for `kg-mcp`
- clearer MCP setup guidance
- examples for project-level prompts that include the graph name
- guidance to let the assistant update the graph when grounded missing information is discovered
- git workflow tips for teams collaborating on the same graph
- practical guidance on which graph files to commit and which local backup/log files to ignore

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
