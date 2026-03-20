# kg - knowledge graph skill

Use the `kg` CLI to read and edit a local JSON knowledge graph.

## Graph selection

All commands start with the graph name:

```
kg <graph> node ...
kg <graph> edge ...
```

Example graph name: `fridge`, `myproject`, etc.

## Read

```bash
# Find nodes by keyword (fuzzy, multi-query)
kg <graph> node find <query>
kg <graph> node find <q1> <q2>

# Get a node by ID
kg <graph> node get <id>

# Get full details (domain_area, provenance, confidence, created_at)
kg <graph> node get <id> --full
```

Output uses symbolic format:
```
# concept:refrigerator | Lodowka
aka: Chlodziarka, Fridge
- Fact 1
- Fact 2
(2 facts total)
-> HAS | concept:cooling_chamber | Komora Chlodzenia
<- STORED_IN | datastore:temp_log | Log Temperatur
```

## Write

```bash
# Add node
kg <graph> node add <id> --type <Type> --name <Name> --source <file>
  [--description <text>] [--fact <text>...] [--alias <text>...]
  [--domain-area <text>] [--provenance <text>] [--confidence <0.0-1.0>]
  [--created-at <ISO8601>]

# Modify node (all flags optional, appended values are deduplicated)
kg <graph> node modify <id> [--name <text>] [--description <text>]
  [--fact <text>...] [--alias <text>...] [--source <file>...]

# Remove node (also removes incident edges)
kg <graph> node remove <id>

# Add edge
kg <graph> edge add <source_id> <RELATION> <target_id> [--detail <text>]

# Remove edge
kg <graph> edge remove <source_id> <RELATION> <target_id>
```

## Node ID convention

Format: `prefix:snake_case`

| Type      | Prefix      |
|-----------|-------------|
| Concept   | `concept:`  |
| Process   | `process:`  |
| DataStore | `datastore:`|
| Interface | `interface:`|
| Rule      | `rule:`     |
| Feature   | `feature:`  |
| Decision  | `decision:` |

## Create a new graph

```bash
kg create <graph_name>
```

Stored at `~/.kg/graphs/<graph_name>.json`.
