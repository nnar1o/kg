# kg — local knowledge graph CLI

> **Beta** — This software is in active development. APIs may change.

A fast, opinionated CLI for managing JSON knowledge graphs with **native MCP server support**. Built for LLM chat workflows: every command outputs deterministic symbolic plain text by default, with `--json` for machine output.

## Installation

Requires a Rust toolchain with Cargo.

```sh
cargo install --path . --locked --root ~/.local
export PATH="$HOME/.local/bin:$PATH"
```

Or build release binary manually:

```sh
cargo build --release
./target/release/kg
```

This project also builds an MCP server binary:

```sh
./target/release/kg-mcp
```

And a TUI browser:

```sh
./target/release/kg-tui <graph>
```

## Quick start

```sh
# Print init prompts/snippets
kg init
kg init --target mcp
kg init --target doc

# Create a new graph
kg create fridge

# List available graphs
kg list
kg list --full
kg list --json

# Search across multiple queries at once
kg fridge node find lodowka rozmrazanie smart api
kg fridge node find lodowka --mode bm25
kg fridge node find lodowka rozmrazanie --json

# TUI browser (async search)
kg-tui fridge --mode bm25

# View a specific node
kg fridge node get concept:refrigerator
kg fridge node get concept:refrigerator --full
kg fridge node get concept:refrigerator --json

# Add a node
kg fridge node add concept:ice_maker \
  --type Concept \
  --name "Kostkarka" \
  --description "Automatyczna kostkarka do lodu" \
  --source instrukcja.md

# Update a node (partial, non-destructive)
kg fridge node modify concept:ice_maker \
  --fact "Wytwarza kostki lodu co 2 godziny" \
  --alias "Ice Maker"

# Rename a node (updates edges + notes)
kg fridge node rename concept:ice_maker concept:ice_maker_v2

# Remove a node (also removes incident edges)
kg fridge node remove concept:ice_maker

# Add an edge
kg fridge edge add concept:refrigerator STORED_IN datastore:settings_storage \
  --detail "Ustawienia zapisane w pamieci"

# Add a note
kg fridge note add concept:refrigerator \
  --text "Sprawdzic czy to dotyczy nowych modeli" \
  --tag backlog \
  --source instrukcja.md

# List notes (optionally filter by node)
kg fridge note list --node concept:refrigerator

# Diff two graphs
kg diff fridge fridge_backup
kg diff fridge fridge_backup --json

# Merge graphs (prefer incoming nodes/edges/notes)
kg merge fridge fridge_backup --strategy prefer-new

# Export graph as JSON (useful for DB backends)
kg fridge export-json --output fridge.json

# Import graph from JSON (overwrites target graph)
kg fridge import-json --input fridge.json

# Import CSV (nodes/edges/notes)
kg fridge import-csv --nodes nodes.csv --edges edges.csv --notes notes.csv
# See docs/import-csv.md for column definitions

# Import Markdown/YAML frontmatter
kg fridge import-md --path docs/notes
# See docs/import-markdown.md for details

# KQL query (simple filters)
kg fridge kql "node type=Concept name~lodowka"
kg fridge kql "node type=Concept" --json

# Export snapshot (ts in ms, auto prefers event log if present)
kg fridge as-of --ts-ms 1711200000000 --output fridge.asof.json

# Export snapshot explicitly from event log
kg fridge as-of --ts-ms 1711200000000 --source event-log --output fridge.asof.json

# List backup snapshots
kg fridge history --limit 10

# List event log snapshots
kg fridge timeline --limit 10

# List event log snapshots within range
kg fridge timeline --since-ts-ms 1711200000000 --until-ts-ms 1711203600000

# Diff two snapshots (ts in ms)
kg fridge diff-as-of --from-ts-ms 1711200000000 --to-ts-ms 1711203600000
kg fridge diff-as-of --from-ts-ms 1711200000000 --to-ts-ms 1711203600000 --json

# Diff two event log snapshots (ts in ms)
kg fridge diff-as-of --from-ts-ms 1711200000000 --to-ts-ms 1711203600000 --source event-log

# Export graph to DOT/Mermaid
kg fridge export-dot --output fridge.dot
kg fridge export-mermaid --output fridge.mmd

# Remove an edge
kg fridge edge remove concept:refrigerator STORED_IN datastore:settings_storage

# Check graph integrity
kg fridge check
kg fridge audit --deep

# Configure backend (local-only)
# .kg.toml
# backend = "redb"

# Analyze graph structure
kg fridge stats
kg fridge list
kg fridge list --type Process --limit 20
kg fridge list --type Process --limit 20 --json
kg fridge quality missing-descriptions
kg fridge quality missing-facts
kg fridge quality duplicates
kg fridge quality edge-gaps
kg fridge quality missing-facts --json
```

## Graph storage

Graphs are stored as JSON files. The default graph root is:

- `$HOME/.kg/graphs/` if `$HOME` (or `USERPROFILE`) is set
- `./.kg/graphs/` relative to the current working directory otherwise

Graph writes are atomic and create a `.bak` file when overwriting an existing graph.
Periodic compressed backups (`.bck.<ts>.gz`) are created at most once per hour.

## Configuration (.kg.toml)

`kg` searches for `.kg.toml` by walking up from the current working directory. Paths are resolved relative to the config file location.

Use a shared graph directory for a project:

```toml
backend = "json"
graph_dir = "graphs/"
```

Or reference graphs explicitly by name:

```toml
[graphs]
fridge = "data/fridge.json"
```

Graph resolution order for `kg <graph> ...` is:

1. Explicit `graphs.<name>` entry in `.kg.toml` (must exist)
2. `graph_dir` from `.kg.toml` with either `<name>` or `<name>.json`
3. Raw path candidates, checked in order: `<graph>`, `./<graph>`, `./<graph>.json`, `<graph_root>/<graph>`, `<graph_root>/<graph>.json`, `./graph-example-<graph>.json`

## Node structure

Each node has:

- `id` — prefixed identifier, e.g. `concept:refrigerator`
- `type` — one of: `Concept`, `Process`, `DataStore`, `Interface`, `Rule`, `Feature`, `Decision`, `Convention`, `Note`, `Bug`
- `name` — human-readable label
- `properties.description` — prose description
- `properties.key_facts` — array of structured facts
- `properties.alias` — array of alternative names
- `properties.domain_area` — optional domain tag
- `properties.provenance` — optional provenance tag
- `properties.confidence` — optional confidence score (0.0–1.0)
- `properties.created_at` — optional ISO timestamp
- `properties.feedback_score` — aggregate feedback score (system-managed)
- `properties.feedback_count` — feedback count (system-managed)
- `properties.feedback_last_ts_ms` — last feedback timestamp in ms (system-managed)
- `source_files` — array of source file paths

## Edge structure

Each edge has:

- `source_id` — node identifier
- `relation` — one of: `HAS`, `STORED_IN`, `TRIGGERS`, `CREATED_BY`, `AFFECTED_BY`, `AVAILABLE_IN`, `DOCUMENTED_IN`, `DEPENDS_ON`, `TRANSITIONS`, `DECIDED_BY`, `GOVERNED_BY`, `USES`, `READS_FROM`
- `target_id` — node identifier
- `properties.detail` — optional edge description
- `properties.feedback_score` — aggregate feedback score (system-managed)
- `properties.feedback_count` — feedback count (system-managed)
- `properties.feedback_last_ts_ms` — last feedback timestamp in ms (system-managed)

## Output format

All output uses symbolic plain text optimized for low token count:

```text
# concept:refrigerator | Lodowka
  aka: Chlodziarka, Fridge
  - Standardowy zakres temperatur: 2-8 st C (chlodziarka), -18 do -24 st C (zamrazarka).
  - Zasilanie: 230V AC, pobor mocy 100-400W.
  - Klasa energetyczna: A++ lub wyzsza dla nowych modeli.
  (3 facts total)
  -> HAS concept:cooling_chamber (Komora Chlodzenia) "Glowna przestrzen..."
  -> HAS concept:temperature (Temperatura) "Parametr fizyczny..."
  -> DOCUMENTED_IN interface:smart_api (Smart Home API (REST)) "Specyfikacja API..."
  <- CREATED_BY process:cooling (Proces Chlodzenia) "Glowny cykl pracy..."
```

Search results are grouped per query:

```text
? lodowka (2)

  # concept:refrigerator | Lodowka
    aka: Chlodziarka, Fridge
    ...

  # interface:smart_api | Smart Home API (REST)
    ...

? rozmrazanie (2)

  # process:defrost | Rozmrazanie Automatyczne
    ...
```

Mutation output is terse:

```
+ node concept:ice_maker
~ node concept:ice_maker
- node concept:ice_maker (5 edges removed)
+ edge concept:refrigerator STORED_IN datastore:settings_storage
- edge concept:refrigerator STORED_IN datastore:settings_storage
```

## Supported node types

`Concept`, `Process`, `DataStore`, `Interface`, `Rule`, `Feature`, `Decision`, `Convention`, `Note`, `Bug`

## ID convention

Node IDs must follow `prefix:snake_case`. The prefix should match the node type:

| Type | Prefix |
|------|--------|
| Concept | `concept` |
| Process | `process` |
| DataStore | `datastore` |
| Interface | `interface` |
| Rule | `rule` |
| Feature | `feature` |
| Decision | `decision` |
| Convention | `convention` |
| Note | `note` |
| Bug | `bug` |

## Skills

OpenCode-compatible skill files for AI-assisted usage:

- [`skills/kg/`](skills/kg/) — basic read/write commands
- [`skills/kg-builder/`](skills/kg-builder/) — building a graph from docs or code
- [`skills/kg-gardener/`](skills/kg-gardener/) — validation and quality analysis

To use: copy the `skills/` directory to `~/.config/opencode/skills/`.

## MCP Server — First-class integration

`kg` ships a native MCP stdio server built with [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk). This is the **primary way** to integrate with LLMs and AI assistants.

### Why MCP?

- **Tool integration** — expose all kg operations as MCP tools
- **Structured feedback** — send feedback signals back to the graph
- **Quality gaps** — query missing descriptions, facts, edge gaps
- **Graph browsing** — interactive HTML export for visualization

### Run locally

```sh
cargo run --bin kg-mcp
```

### MCP client config (OpenCode example)

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

If you prefer running from source during development:

```json
{
  "mcpServers": {
    "kg": {
      "command": "cargo",
      "args": ["run", "--quiet", "--bin", "kg-mcp"]
    }
  }
}
```

### What the MCP server exposes

- A shell-like script tool (`kg`) that runs one or more commands separated by `;` or newlines
- Tools for all core graph operations (`kg_node_find`, `kg_node_get`, `kg_node_add`, `kg_node_add_batch`, `kg_node_modify`, `kg_node_remove`, `kg_edge_add`, `kg_edge_remove`, `kg_stats`, `kg_check`, `kg_audit`, `kg_quality`, `kg_export_html`, `kg_access_log`, `kg_access_stats`, `kg_feedback`, `kg_feedback_batch`)
- A generic passthrough tool (`kg_command`) that supports full CLI coverage by accepting raw `kg` args
- Resources:
  - `kg://cwd` — current working directory for the MCP process
  - `kg://graphs` — discovered graph JSON files
  - `kg://graph/{graph}` — rendered graph stats summary
- Prompt template `kg_workflow_prompt` for planning safe multi-step graph edits

All tool responses return the same deterministic symbolic text as the CLI.

## Architecture

```
src/
  main.rs       — entry point, error handling
  cli.rs        — clap command definitions
  lib.rs        — execution logic, validation, quality analysis
  graph.rs      — data model and persistence
  config.rs     — .kg.toml discovery
  output.rs     — symbolic text rendering
```

## Testing

```sh
cargo test
```
