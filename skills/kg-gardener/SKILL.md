# kg-gardener - graph quality skill

Use the `kg` CLI to inspect, validate, and improve the quality of a knowledge graph.

## Validation

```bash
# Quick structural check (schema, IDs, references, duplicates)
kg <graph> check

# Full audit with deep source file checks
kg <graph> audit --deep
kg <graph> audit --errors-only
kg <graph> audit --warnings-only
```

Output format:
```
= check
status: VALID | INVALID
errors: N
warnings: N
error-list:
- duplicate node id: concept:foo
warning-list:
- node concept:bar missing description
```

## Stats

```bash
kg <graph> stats
kg <graph> stats --by-type
kg <graph> stats --by-relation
kg <graph> stats --show-sources
```

## Quality commands

```bash
# Nodes with no description
kg <graph> quality missing-descriptions
kg <graph> quality missing-descriptions --limit 20 --node-types Concept,Process

# Nodes with no key_facts
kg <graph> quality missing-facts
kg <graph> quality missing-facts --sort edges

# Suspiciously similar node names
kg <graph> quality duplicates
kg <graph> quality duplicates --threshold 0.75

# Structural gaps (DataStores missing STORED_IN, Processes with no inputs)
kg <graph> quality edge-gaps
kg <graph> quality edge-gaps --relation STORED_IN
```

Short aliases also work:

```bash
kg <graph> missing-descriptions
kg <graph> missing-facts
kg <graph> duplicates
kg <graph> edge-gaps
```

## Typical gardening workflow

1. Run `kg <graph> check` to find hard errors first.
2. Run `kg <graph> stats --by-type --by-relation` to get an overview.
3. Run `kg <graph> quality missing-descriptions` and fix empty descriptions.
4. Run `kg <graph> quality missing-facts` sorted by edge count to prioritize important nodes.
5. Run `kg <graph> quality duplicates` to catch naming inconsistencies.
6. Run `kg <graph> quality edge-gaps` to find structural holes.
7. Run `kg <graph> audit --deep` as a final check including source file existence.
