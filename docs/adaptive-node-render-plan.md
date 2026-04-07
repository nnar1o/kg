# Adaptive Node Rendering Plan

## Goal

Make `node find` and `node get` render text adaptively toward a target response size instead of using a fixed output shape.

The feature should work the same way in both CLI and MCP text responses.

## Primary behavior

- Default target size: `1400` characters.
- New option for CLI and MCP text rendering: `target_chars` / `--target-chars`.
- `--full` keeps the current non-adaptive behavior.
- `--json` keeps the current non-adaptive behavior.
- Adaptive rendering is enabled by default for normal text output in `node find` and `node get`.

## Rendering strategy

Use an iterative candidate-based approach instead of one fixed renderer.

1. Generate several candidate renderings.
2. Measure the final character count of each candidate.
3. Compare candidates against the target window.
4. Pick the best candidate.

Target window:

- Acceptable fit: `70%..100%` of `target_chars`.
- Prefer staying under target.
- If every useful candidate exceeds target, pick the smallest reasonable overshoot.

## Depth behavior

### Single result

If `node find` returns exactly one node, or for `node get`:

- Start with `depth=0`.
- If the output is too small, generate deeper candidates (`depth=1`, then `depth=2`, optionally `depth=3`).
- Choose the deepest candidate that still fits best within the target window.

`depth=0` means:

- main node details
- direct links summary and selected links
- no expanded neighbor summaries

`depth>0` means:

- include neighbor summaries layer by layer
- continue only while the candidate remains a good fit for the target size

### Multiple results

For multiple matches from `node find`:

- Prefer breadth over depth.
- Start with `depth=0` for all returned nodes.
- First shrink detail level and visible links.
- Then shrink the number of visible nodes.
- Only consider deeper expansion for top results if the result set is very small and the output is still significantly below target.

## Handling many links on a node

Nodes with many incident edges must not consume the entire response.

### Hub mode

Enable a compact "hub mode" when a node has more than a threshold number of links.

Suggested initial threshold:

- `12` incident edges

In hub mode:

- render aggregated link counts by direction and relation
- render only top-ranked edge lines
- append an omission marker such as `... N more links omitted`

Example summary shape:

```text
links: 84 total
out: USES x22, DEPENDS_ON x15, READS_FROM x9
in: AFFECTED_BY x18, CREATED_BY x7
-> USES | concept:cache | Cache
<- AFFECTED_BY | bug:timeout | Timeout bug
... 76 more links omitted
```

## Candidate dimensions

Each candidate should vary along a small set of knobs.

- `depth`
- `detail_level`
- `edge_cap`
- `result_cap`

Suggested detail levels:

- `Rich`
- `Compact`
- `Minimal`

Suggested behavior by detail level:

- `Rich`: full header, several facts, visible links, richer neighbor summaries
- `Compact`: header, limited facts, limited links
- `Minimal`: `# id | name [type]` plus minimal supporting context

## Candidate selection

Each candidate should be scored after rendering.

Inputs to scoring:

- rendered character count
- overshoot above `target_chars`
- undershoot below `target_chars`
- depth used
- number of visible result nodes
- number of visible links

Selection priorities:

1. Strongly prefer candidates at or below target.
2. Prefer candidates within the `70%..100%` window.
3. For single-result flows, prefer greater useful depth when size fit is similar.
4. For multi-result flows, prefer showing more result nodes before expanding depth.

## Rendering rules

### Single-result flow

Suggested initial candidates:

- `depth=0`, `Rich`
- `depth=1`, `Compact`
- `depth=2`, `Compact`
- `depth=2`, `Minimal`

Optional later extension:

- `depth=3`, `Minimal`

### Multi-result flow

Suggested initial candidates:

- `depth=0`, all results, `Compact`
- `depth=0`, all results, fewer links, `Compact`
- `depth=0`, top `K` results, `Minimal`
- optional `depth=1` for top `1..2` results only when total results are small and output is far below target

## CLI and MCP behavior

The adaptive rendering rules should be shared between CLI and MCP.

### CLI

- Add `--target-chars <n>` to `node find` and `node get`.
- Default text output uses adaptive rendering.
- `--full` and `--json` bypass adaptive rendering.

### MCP

- Add `target_chars` to `kg_node_find` and `kg_node_get`.
- Extend the shell-style `kg` parser to support `--target-chars`.
- MCP should reuse the same adaptive renderer as CLI.

## Code changes

### `src/cli.rs`

- Add `target_chars: Option<usize>` to `NodeCommand::Find` and `NodeCommand::Get`.

### `src/app/graph_node_edge.rs`

- Route text rendering through adaptive rendering when `full == false` and `json == false`.
- Keep existing code path for `--full` and `--json`.

### `src/output.rs`

- Add `render_find_adaptive(...)`.
- Add `render_node_adaptive(...)`.
- Add candidate generation and selection helpers.
- Add link aggregation and ranking helpers.
- Keep current `render_node_block(...)` as a lower-level building block for full rendering.
- Stop hard-limiting links inside fixed helpers when adaptive logic needs full edge access.

Suggested helper areas:

- full edge collection
- neighbor traversal up to depth
- hub summarization
- candidate rendering
- candidate scoring and selection

### `src/bin/kg-mcp.rs`

- Add `target_chars` to `NodeFindArgs` and `NodeGetArgs`.
- Parse `--target-chars` in shell-style command parsing.
- Pass the value through to the existing CLI-backed calls.

## Testing

Add tests for at least the following cases:

- `node get` respects `--target-chars` and stays near budget
- `node find` with one result prefers deeper output than `depth=0` when budget allows
- `node find` with many results shrinks detail before dropping all context
- hub node output shows link aggregation and omission markers
- `--full` preserves current behavior
- MCP shell parser accepts `--target-chars`
- CLI and MCP text semantics remain aligned

## Recommended rollout

Phase 1:

- implement adaptive renderer for text output only
- keep `--full` and `--json` untouched
- support depth up to `2`
- support hub mode and omission markers

Phase 2:

- tune scoring weights based on real graphs
- decide whether shallow depth expansion for small multi-result sets improves output quality
- optionally raise max smart depth to `3`

## Open points

- Exact scoring weights for overshoot vs undershoot.
- Whether multi-result rendering should ever use `depth=1` by default.
- Whether the default target should remain a constant or later become configurable in `.kg.toml`.
