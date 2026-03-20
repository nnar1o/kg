# kg — local knowledge graph CLI

A fast, opinionated CLI for managing JSON knowledge graphs. Built for LLM chat workflows: every command outputs deterministic symbolic plain text, not JSON.

## Installation

```sh
cargo install --path . --root ~/.local
export PATH="$HOME/.local/bin:$PATH"
```

Or build release binary manually:

```sh
cargo build --release
./target/release/kg
```

## Quick start

```sh
# Create a new graph
kg create fridge

# Search across multiple queries at once
kg fridge node find lodowka rozmrazanie smart api

# View a specific node
kg fridge node get concept:refrigerator
kg fridge node get concept:refrigerator --full

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

# Remove a node (also removes incident edges)
kg fridge node remove concept:ice_maker

# Add an edge
kg fridge edge add concept:refrigerator STORED_IN datastore:settings_storage \
  --detail "Ustawienia zapisane w pamieci"

# Remove an edge
kg fridge edge remove concept:refrigerator STORED_IN datastore:settings_storage

# Check graph integrity
kg fridge check
kg fridge audit --deep

# Analyze graph structure
kg fridge stats
kg fridge quality missing-descriptions
kg fridge quality missing-facts
kg fridge quality duplicates
kg fridge quality edge-gaps
```

## Graph storage

Graphs are stored as JSON files. The default location is `~/.kg/graphs/<name>.json`.

Graphs can also be placed alongside the project via a `.kg.toml` config:

```toml
graph_dir = "graphs/"
```

Or referenced explicitly by name in config:

```toml
[graphs]
fridge = "data/fridge.json"
```

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
- `source_files` — array of source file paths

## Edge structure

Each edge has:

- `source_id` — node identifier
- `relation` — one of: `HAS`, `STORED_IN`, `TRIGGERS`, `CREATED_BY`, `AFFECTED_BY`, `AVAILABLE_IN`, `DOCUMENTED_IN`, `DEPENDS_ON`, `TRANSITIONS`, `DECIDED_BY`, `GOVERNED_BY`, `USES`, `READS_FROM`
- `target_id` — node identifier
- `properties.detail` — optional edge description

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
