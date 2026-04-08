# kg v0.2.3

`kg-mcp` is a lightweight local alternative to classic RAG systems when you want structured, editable, git-friendly project memory instead of document chunk retrieval.

This release is a small follow-up to `0.2.2` that restores the bundled fridge fixture used by tests and crate packaging verification, and includes a small README wording refinement.

## Highlights

- restore `graph-example-fridge.json` used by the full test suite and packaged crate verification
- keep `cargo test` and `cargo package` green again after the earlier cleanup commit
- small README wording refinement around local-first git-friendly project memory

## Why this release matters

- the crate published to crates.io now contains the missing fixture again
- CI and local full-suite verification match the published source state
- the README copy stays aligned with the git-friendly local-memory positioning

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
