# kg-mcp MCP Tools – Optimized Proposal

## Problem

Current: **22 tools** + 3 prompts + 3 resources. Most individual node/edge tools bloat AI context (~15k+ tokens in system prompt). The `kg` script runner already handles all CRUD via CLI commands — the granular tools are redundant wrappers.

## Proposed Tools (3)

| # | Tool | Description | Parameters |
|---|------|-------------|------------|
| 1 | **`kg`** | Execute one or more kg commands: find/get nodes, CRUD nodes/edges, graph create/stats, audit, feedback. | `script: string` (commands separated by `;` or newline), `mode?: "best_effort"\|"strict"`, `debug?: bool` |
| 2 | **`kg_help`** | Return detailed manual with examples for a kg domain. | `domain: string` — one of: `node`, `edge`, `graph`, `schema`, `kql`, `feedback`, `batch`, `script`, `all` |
| 3 | **`kg_schema`** | Return valid node types, relations, ID prefixes, and edge rules. | (none) |

## How `kg_help` Works

When the AI needs to understand how to perform an operation, it calls `kg_help` with the appropriate domain:

| Domain | Returns |
|--------|---------|
| `node` | How to find, get, add (single + batch), modify, remove nodes. Field reference, required vs optional, `source` format rules, provenance/confidence/importance dictionary. Full JSON examples. |
| `edge` | How to add (single + batch, dry-run), remove edges. Relation dictionary, edge validation rules. Full JSON examples. |
| `graph` | How to create graphs, get stats, run audits, export. Full examples. |
| `schema` | Complete schema reference: all node types (GDIR, GFIL, GSYM, Concept, Process, DataStore, Interface, Rule, Feature, Decision, Convention, Note, Bug), relation types with G-prefix rules, ID format `<type_code>:snake_case`, edge combination rules, provenance/confidence/importance ranges. |
| `kql` | KQL query language syntax, operators, examples. |
| `feedback` | How feedback works: `YES`, `NO`, `NIL`, `PICK n`. Passive vs active feedback. Feedback line format. Batch feedback via `kg` script. |
| `batch` | How to batch node/edge operations. Atomic vs best_effort mode. `on_conflict=skip`. Dry-run preflight. |
| `script` | Multi-command scripting syntax: `;` vs newline separation, quote/escape handling, mixing find→get→feedback in one call. Debug mode. |
| `all` | Complete kg-mcp reference (everything above combined). Use when exploring. |

### Usage Flow

```
AI: calls kg_schema() → discovers valid types/relations
AI: calls kg_help(domain="node") → learns node CRUD with examples
AI: calls kg(script="fridge node find 'compressor'") → executes
```

## Migration Map

| Old Tool | Replaced By |
|----------|-------------|
| `kg_node_find` | `kg` script: `graph node find "query" [--full] [--output-size N] [--limit N]` |
| `kg_node_get` | `kg` script: `graph node get id [--full] [--output-size N]` |
| `kg_node_add` | `kg` script: `graph node add <id> --type <T> --name <N> [flags...]` |
| `kg_node_add_batch` | `kg` script: `graph node add-batch [...]` |
| `kg_node_modify` | `kg` script: `graph node modify id [fields...]` |
| `kg_node_remove` | `kg` script: `graph node remove id` |
| `kg_edge_add` | `kg` script: `graph edge add <source> <relation> <target> [--detail ...]` |
| `kg_edge_add_batch` | `kg` script: `graph edge add-batch [...] [--dry-run]` |
| `kg_edge_remove` | `kg` script: `graph edge remove <source> <relation> <target>` |
| `kg_create_graph` | `kg` script: `graph create <name>` |
| `kg_stats` | `kg` script: `graph stats [--by-type] [--by-relation]` |
| `kg_gap_summary` | `kg` script: `graph gap-summary [--limit N]` |
| `kg_feedback_batch` | `kg` script: inline `uid=abc123 YES` lines |
| `kg_command` | `kg` (single command = single-line script) |
| `kg_check` (deprecated) | `kg` script: `graph check [--deep]` |
| `kg_audit` (deprecated) | `kg` script: `graph audit [--deep]` |
| `kg_quality` (deprecated) | `kg` script: `graph quality <cmd>` |
| `kg_export_html` (deprecated) | `kg` script: `graph export-html [--output ...]` |
| `kg_access_log` (deprecated) | `kg` script: `graph access-log` |
| `kg_access_stats` (deprecated) | `kg` script: `graph access-stats` |

## Prompts & Resources

Prompts and resources stay unchanged — they don't bloat the JSON tool list.

| Type | Name | Purpose |
|------|------|---------|
| Prompt | `kg_workflow_prompt` | Template for planning kg operations |
| Prompt | `kg_collaborative_prompt` | Collaborative graph improvement |
| Prompt | `kg_feedback_retrospective_prompt` | Feedback + gaps retrospective |
| Resource | `kg://cwd` | Current working directory |
| Resource | `kg://graphs` | Discovered graph files |
| Resource | `kg://graph/{name}` | Graph stats summary |

## Token Savings Estimate

| | Before | After |
|---|--------|-------|
| Tools in system prompt | 22 tools × ~600 tokens each ≈ 13,200 tokens | 3 tools × ~200 tokens ≈ 600 tokens |
| Tool descriptions | Verbose, all params listed | Short, domain-specific help offloaded |
| Context saved | — | **~12,600 tokens** (95% reduction) |

## Implementation Notes

1. **`kg` tool** — Already exists (`kg-mcp_kg: KgScriptArgs`). Rename to `kg`. No logic changes needed.
2. **`kg_help` tool** — New tool. Returns pre-compiled help text for each domain. Content derived from `skills/kg/SKILL.md` and `docs/mcp.md`. No runtime parsing of help content — static strings, cheap.
3. **`kg_schema` tool** — Already exists (`kg-mcp_kg_schema`). Rename to `kg_schema`. No logic changes.
4. **Deprecated tools** — Remove all from the `#[tool_router]` registration. They still work via `kg` script internally.
5. **Node/edge tools** — Remove `kg_node_add`, `kg_node_get`, `kg_node_find`, etc. All handled by `kg` script runner which already dispatches `handle_node_find`, `handle_node_get`, `execute_kg_for` → `run_kg()`.

## Risk

- **AI must learn to use `kg_help` proactively.** The first call to any kg operation should be `kg_schema` → `kg_help <domain>` → `kg <script>`. This is a one-step indirection compared to directly calling typed tools, but saves massive context.
- **`kg` script syntax must be well-documented in `kg_help`.** If the help content is unclear, the AI will struggle. The help content is the critical dependency.
- **Feedback tracking.** Currently `handle_node_find`/`handle_node_get` auto-track feedback state. The `kg` script runner must preserve this behavior — it already does.
