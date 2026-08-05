# kg v0.3.0

Feature release: makes MCP graph scope explicit, tightens canonical/SCL script handling, and preserves feedback workflows.

## Highlights

- add the top-level MCP `graph` argument as the preferred graph selector
- persist `use <graph>` over later lines in the same script
- reject graph-scoped operations without `graph`, `use <graph>`, or an explicit canonical graph prefix instead of silently choosing a default
- improve canonical CLI/SCL grammar handling while preserving feedback-required and passive feedback behavior

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

## Previous: v0.2.24

# kg v0.2.24

Patch release: aligns MCP/SCL prompts and documentation with the current three-tool surface, correcting retrospective, importance, and provenance guidance.

## Highlights

- document the three-tool MCP/SCL surface: `kg`, `kg_help`, and `kg_schema`
- correct feedback-retrospective prompt commands and normalize importance guidance to `0.0..1.0`
- document generated-node provenance with `G`

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

## Previous: v0.2.23

# kg v0.2.23

Patch release: fixes `cargo fmt` formatting violations that caused CI failures in v0.2.22. No behavioral changes.

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

## Previous: v0.2.22

# kg v0.2.22

`kg_help` and the SCL cheat-sheet are now SCL-first: short verb-first commands are the primary syntax shown to LLMs, with canonical CLI as fallback.

## Highlights

- rewrite `scl_cheat_sheet()` with quick examples, core verbs, IDs, relations, and tips
- rewrite `get_help()` domain sections (node, edge, graph, feedback, batch, script) to lead with SCL syntax
- expand README SCL quickstart with a full verb table and practical examples

## Why this release matters

- LLMs using `kg-mcp` now see natural short-English commands first, reducing friction and canonical-CLI memorization
- help text is consistent with the SCL language the parser already accepts
- README onboarding matches the actual MCP tool surface

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
