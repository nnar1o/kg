# Getting Started with kg

This guide is for people who want to use `kg` quickly without reading the full reference first.

## 1) Install

From source (recommended in this repository):

```sh
cargo install --path .
```

Alternative:

```sh
cargo install kg-cli
```

Check installation:

```sh
kg --version
kg --help
```

## 2) Mental model

- A **graph** is your knowledge base (for example: `fridge`, `payments`, `incident-2026-04`).
- A graph contains:
  - **nodes** (Concept, Process, Rule, Feature, Decision, Convention, Note, Bug, DataStore, Interface)
  - **edges** (relations between nodes)
  - **notes** (free-form context attached to nodes)

Primary command shape:

```sh
kg graph <graph-name> <command> ...
```

## 3) First working graph

Create a graph:

```sh
kg create fridge
```

Add nodes:

```sh
kg graph fridge node add concept:refrigerator --type Concept --name "Refrigerator"
kg graph fridge node add process:defrost --type Process --name "Defrost cycle" --importance 5
```

Connect nodes:

```sh
kg graph fridge edge add concept:refrigerator DEPENDS_ON process:defrost --detail "requires periodic defrost"
```

Find and inspect:

```sh
kg graph fridge node find refrigerator
kg graph fridge node get concept:refrigerator --full
kg graph fridge node find refrigerator --output-size 1200
```

Validate quality:

```sh
kg graph fridge check
kg graph fridge quality missing-facts
kg graph fridge quality missing-descriptions
```

## 4) Useful day-1 commands

```sh
# Graph overview
kg list --full
kg graph fridge stats --by-type --by-relation

# Query language
kg graph fridge kql "node type=Concept sort=name limit=20"

# Imports
kg graph fridge import-csv --help
kg graph fridge import-md --help

# Export/backup style operations
kg graph fridge export-json --help
kg graph fridge history
kg graph fridge timeline
```

## 5) New runtime behavior (`.kg`, sidecars, legacy)

- Default runtime prefers `.kg` text graphs.
- If only `.json` exists, runtime can migrate side-by-side to `.kg`.
- Optional sidecars may appear for `.kg` graphs:
  - `<graph>.kgindex`
  - `<graph>.kglog`
- Use `--legacy` to force JSON-first behavior for older integrations:

```sh
kg graph fridge --legacy stats
```

Strict format checks are optional and disabled by default:

```sh
KG_STRICT_FORMAT=1 kg graph fridge check
```

## 6) MCP setup (for AI assistants)

Run server:

```sh
kg-mcp
```

Minimal client config example:

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

See full details: [`docs/mcp.md`](mcp.md)

## 7) If something is unclear

Start with help pages in this order:

```sh
kg --help
kg graph --help
kg graph <graph-name> node --help
kg graph <graph-name> edge --help
```

Then check [`docs/troubleshooting.md`](troubleshooting.md).
