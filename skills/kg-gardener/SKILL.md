# kg-gardener - graph quality skill

Use the `kg` CLI to inspect, validate, and improve the quality of a knowledge graph.

Preferred command pattern: `kg graph <graph> ...`.

## Validation

```bash
# Quick structural check (schema, IDs, references, duplicates)
kg graph <graph> check

# Full audit with deep source file checks
kg graph <graph> audit --deep
kg graph <graph> audit --errors-only
kg graph <graph> audit --warnings-only
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
kg graph <graph> stats
kg graph <graph> stats --by-type
kg graph <graph> stats --by-relation
kg graph <graph> stats --show-sources
```

## Quality commands

```bash
# Nodes with no description
kg graph <graph> quality missing-descriptions
kg graph <graph> quality missing-descriptions --limit 20 --node-types Concept,Process

# Nodes with no key_facts
kg graph <graph> quality missing-facts
kg graph <graph> quality missing-facts --sort edges

# Suspiciously similar node names
kg graph <graph> quality duplicates
kg graph <graph> quality duplicates --threshold 0.75

# Structural gaps (DataStores missing STORED_IN, Processes with no inputs)
kg graph <graph> quality edge-gaps
kg graph <graph> quality edge-gaps --relation STORED_IN
```

Short aliases also work:

```bash
kg graph <graph> missing-descriptions
kg graph <graph> missing-facts
kg graph <graph> duplicates
kg graph <graph> edge-gaps
```

## Typical gardening workflow

1. Run `kg graph <graph> check` to find hard errors first.
2. Run `kg graph <graph> stats --by-type --by-relation` to get an overview.
3. Run `kg graph <graph> quality missing-descriptions` and fix empty descriptions.
4. Run `kg graph <graph> quality missing-facts` sorted by edge count to prioritize important nodes.
5. Run `kg graph <graph> quality duplicates` to catch naming inconsistencies.
6. Run `kg graph <graph> quality edge-gaps` to find structural holes.
7. Run `kg graph <graph> audit --deep` as a final check including source file existence.
