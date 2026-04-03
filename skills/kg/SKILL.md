# kg - knowledge graph skill

Use the `kg` CLI to read and edit a local knowledge graph (`.kg` by default, with `.json` compatibility).

## Command pattern

Preferred form:

```bash
kg graph <graph> <command> ...
```

Backward-compatible shorthand (`kg <graph> ...`) may still work in many places, but use `kg graph` in new instructions.

## Read

```bash
# Find nodes by keyword (fuzzy, multi-query)
kg graph <graph> node find <query>
kg graph <graph> node find <q1> <q2>

# Get a node by ID
kg graph <graph> node get <id>

# Get full details
kg graph <graph> node get <id> --full
```

## Write

```bash
# Add node
kg graph <graph> node add <id> --type <Type> --name <Name> --source <file>
  [--description <text>] [--fact <text>...] [--alias <text>...]
  [--domain-area <text>] [--provenance <text>] [--confidence <0.0-1.0>]
  [--created-at <ISO8601>] [--importance <1..6>]

# Modify node (all flags optional, appended values are deduplicated)
kg graph <graph> node modify <id> [--name <text>] [--description <text>]
  [--fact <text>...] [--alias <text>...] [--source <file>...]
  [--importance <1..6>]

# Remove node (also removes incident edges)
kg graph <graph> node remove <id>

# Add edge
kg graph <graph> edge add <source_id> <RELATION> <target_id> [--detail <text>]

# Remove edge
kg graph <graph> edge remove <source_id> <RELATION> <target_id>
```

## Node ID convention

Format: `prefix:snake_case`

| Type      | Prefix       |
|-----------|--------------|
| Concept   | `concept:`   |
| Process   | `process:`   |
| DataStore | `datastore:` |
| Interface | `interface:` |
| Rule      | `rule:`      |
| Feature   | `feature:`   |
| Decision  | `decision:`  |
| Convention| `convention:`|
| Note      | `note:`      |
| Bug       | `bug:`       |

## Create a new graph

```bash
kg create <graph_name>
```

Default file is `~/.kg/graphs/<graph_name>.kg` (legacy JSON mode available via `--legacy` on graph commands).
