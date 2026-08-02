# kg-mcp Simple Command Language (SCL) — Reference

> Goal: let LLM agents operate the kg graph via short English commands
> (`find fridge`, `add edge x HAS y`, `create concept:foo ...`) instead of the
> verbose, error-prone `graph node find ... --limit N --mode hybrid` CLI.
> SCL is a thin translation layer on top of the existing CLI; it does **not**
> replace it. SCL is used through the single `kg` MCP tool — there are no
> separate typed tools for each operation.

---

## 1. Why LLMs fail today

Root causes observed in `src/bin/kg-mcp.rs` and `src/cli.rs`:

| # | Problem | Symptom |
|---|---------|---------|
| 1 | Verbosity: `fridge node find "compressor" --limit 5 --mode hybrid --full` | LLM forgets flags, drops quotes, swaps `--limit`/`--output-size` |
| 2 | Graph name must be the first token (`fridge node ...`) | LLM omits it or guesses wrong graph |
| 3 | ID format `<type_code>:snake_case` is strict | LLM emits `Fridge`, `fridge_1`, `"concept:foo bar"` |
| 4 | `node add` requires `--source "<TYPE> <LINK>"` with restricted TYPE set | LLM writes free-form source strings → validation error |
| 5 | `provenance` must be single letter `U|D|A|G` | LLM sends `"user"`, `"derived"` |
| 6 | Edge relations are constrained per source/target type | LLM picks any relation → `validate_edge` rejects |
| 7 | Two parallel surfaces: typed MCP tools (`kg_node_find`, `kg_edge_add_batch`, ...) and free-text `kg` script | **Resolved:** typed tools removed; only `kg`, `kg_help`, `kg_schema` exist |
| 8 | Error messages point to internal validators, not to what the LLM should type | No actionable hint |

SCL fixes #1, #2, #3, #5, #8 directly and softens #4/#6 via smart defaults
and helpful errors. #7 was addressed by removing the typed tools — only `kg`,
`kg_help`, and `kg_schema` exist.

---

## 2. Design principles

1. **One entry tool.** `kg` accepts a script of one or more SCL lines
   separated by `;` or newline. There are no separate typed tools — `kg`
   is the single entry point for all operations.
2. **Graph is implicit.** SCL never requires the graph name as the first
   token. The MCP server resolves it from the active graph context
   (server config / session). A `use <graph>` line overrides for the script.
3. **Verb-first grammar.** Every line starts with a verb: `find`, `search`,
   `get`, `add`, `create`, `modify`, `remove`, `connect`, `disconnect`,
   `list`, `stats`, `help`, `feedback`, `use`. No nested subcommands.
4. **Synonyms collapse to one verb.** `search` = `find`. `create` = `add`
   (for nodes). `connect` = `add edge`. `disconnect` = `remove edge`.
5. **Positional first, flags second.** The essential argument is positional;
   optional modifiers use `--flag value` (unchanged from CLI, so LLMs that
   already emit flags still work).
6. **Defaults fill required fields.** `add` auto-fills `provenance=A`,
   `confidence=0.7`, `importance=0.5`, `created_at=now`, and a synthetic
   `source` (`OTHER <link-or-name>`) when the LLM omits them. Strict mode
   (`strict` keyword at script start) disables defaults and re-enables
   full validation.
7. **Errors are actionable.** Every rejection returns: the offending token,
   the expected grammar, a one-line fix example, and the canonical CLI
   equivalent.
8. **Backward compatible.** Any line that already parses as canonical CLI
   (`<graph> node find ...`) is passed through untouched. SCL only kicks in
   when the first token is a known verb, not a graph name.

---

## 3. Grammar (EBNF-ish)

```
script      ::= line ( ";" | newline )*
line        ::= use | find | get | add | modify | remove |
                connect | disconnect | list | stats | help | feedback |
                strict | cli_passthrough

use         ::= "use" graphname
strict      ::= "strict"                       # applies to rest of script
find        ::= ("find"|"search") query (flag)*
get         ::= "get" nodeid (flag)*
add         ::= ("add"|"create") nodeid ["as" nodetype] (flag)*
modify      ::= "modify" nodeid (flag)*
remove      ::= "remove" ("node"|"edge")? target
connect     ::= ("connect"|"link"|"add edge") srcid relation tgtid (flag)*
disconnect  ::= ("disconnect"|"remove edge") srcid relation tgtid
list        ::= "list" ("nodes"|"edges"|"types"|"relations"|"graphs")
stats        ::= "stats" ["graph" graphname]
help        ::= "help" [topic]
feedback    ::= "feedback" uid verdict
verdict     ::= "yes" | "no" | "nil" | "pick" number

target      ::= nodeid | (srcid relation tgtid)
nodeid      ::= typecode ":" name
query       ::= bareword | quoted
flag        ::= "--" name [value]
relation    ::= uppercase_word                 # HAS, USES, ...
graphname   ::= bareword
topic       ::= bareword
```

Notes:
- `bareword` = `[A-Za-z0-9_:-]+`
- `quoted` = `"..."` with `\"` escapes (reuses `tokenize_command`)
- Unknown first token → treat whole line as canonical CLI passthrough

---

## 4. Verb reference

### 4.1 `find` / `search`
```
find <query> [--limit N] [--mode hybrid|bm25|fuzzy] [--full]
```
- `find fridge` → `node find "fridge"`
- `find "smart fridge" --limit 5 --full`
- `search compressor --mode bm25`

### 4.2 `get`
```
get <nodeid> [--full]
```
- `get concept:refrigerator`
- `get process:defrost_cycle --full`

### 4.3 `add` / `create`
```
add <nodeid> [as <type>] [--name "..."] [--desc "..."] [--domain "..."]
            [--importance 0.8] [--confidence 0.9] [--provenance U|D|A|G]
            [--source "URL https://..."] [--fact "..."] [--alias "..."]
```
- `add concept:fridge --name "Refrigerator" --desc "kitchen appliance"`
- `create bug:door_seal_leak --name "Door seal leak"`
- Type inferred from ID prefix if `as` omitted (`concept:foo` → `Concept`).
- Defaults: `provenance=A`, `confidence=0.7`, `importance=0.5`,
  `source="OTHER scl:<nodeid>"`, `created_at=now`.

### 4.4 `modify`
```
modify <nodeid> [--name "..."] [--desc "..."] [--importance 0.9]
                [--confidence 0.9] [--fact "..."] [--alias "..."]
```
- `modify concept:fridge --importance 0.9 --fact "uses R134a"`
- Append facts with repeated `--fact`. Description replaces.

### 4.5 `remove`
```
remove <nodeid>
remove edge <srcid> <relation> <tgtid>
```
- `remove concept:old_thing`
- `remove edge concept:fridge HAS feature:door`

### 4.6 `connect` / `add edge`
```
connect <srcid> <relation> <tgtid> [--detail "..."]
```
- `connect concept:fridge USES process:compressor_cycle`
- `add edge concept:fridge HAS feature:door --detail "primary door"`

### 4.7 `disconnect` / `remove edge`
```
disconnect <srcid> <relation> <tgtid>
```

### 4.8 `list`
```
list nodes | edges | types | relations | graphs
```
- `list types` → returns valid node types + prefixes
- `list relations` → returns valid relations + edge rules
- `list graphs` → returns available graph names

### 4.9 `stats`
```
stats [graph <graphname>]
```

### 4.10 `help`
```
help [find|get|add|modify|remove|connect|list|feedback|...]
```
Returns grammar + examples for the topic. `help` alone returns the full
SCL cheat-sheet (this is what the MCP server should expose as tool
description so LLMs see it up front).

### 4.11 `feedback`
```
feedback <uid> yes | no | nil | pick <N>
```
- `feedback abc123 yes`
- `feedback abc123 pick 2`

### 4.12 `use`
```
use <graphname>
```
Sets active graph for the rest of the script. If omitted, server uses
its configured default graph.

### 4.13 `strict`
```
strict
```
Switches subsequent lines to full validation (no auto-defaults).
Place at script start for strict mode throughout.

---

## 5. Translation table (SCL → canonical CLI)

| SCL | Canonical |
|-----|-----------|
| `find fridge` | `<g> node find "fridge"` |
| `search fridge --limit 5` | `<g> node find "fridge" --limit 5` |
| `get concept:fridge` | `<g> node get concept:fridge` |
| `add concept:fridge --name "Fridge"` | `<g> node add concept:fridge --type Concept --name "Fridge" --provenance A --confidence 0.7 --importance 0.5 --source "OTHER scl:concept:fridge"` |
| `create bug:leak --name "Leak"` | as above with `--type Bug` |
| `modify concept:fridge --importance 0.9` | `<g> node modify concept:fridge --importance 0.9` |
| `remove concept:fridge` | `<g> node remove concept:fridge` |
| `connect concept:fridge USES process:comp` | `<g> edge add concept:fridge USES process:comp` |
| `add edge x HAS y --detail "d"` | `<g> edge add x HAS y --detail "d"` |
| `disconnect x HAS y` | `<g> edge remove x HAS y` |
| `list types` | (new) returns schema node types |
| `stats` | `<g> stats` |
| `feedback abc yes` | `uid=abc YES` |
| `use fridge` | sets graph context |
| `strict` | toggles strict mode |

`<g>` = active graph name resolved from `use` or server default.

---

## 6. Smart defaults & inference

| Field | Default | Source |
|-------|---------|--------|
| `node_type` | from ID prefix (`concept:foo`→`Concept`) | `validate.rs:69-80` |
| `provenance` | `A` (assumed) | `validate.rs` |
| `confidence` | `0.7` | `validate.rs` |
| `importance` | `0.5` | `validate.rs` |
| `created_at` | `now()` UTC | `validate.rs` |
| `source` | `OTHER scl:<nodeid>` | `validate.rs` source enum |
| `name` | humanized ID (`concept:smart_fridge`→`Smart Fridge`) | new helper |

Strict mode disables all defaults; missing required fields produce an
actionable error listing exactly which fields are missing and the SCL
line to add them.

---

## 7. Error contract

Every SCL rejection returns JSON:

```json
{
  "ok": false,
  "error": "invalid_relation",
  "message": "Relation 'OWNS' is not valid. Valid relations: HAS, USES, ...",
  "input": "connect concept:fridge OWNS process:comp",
  "expected_grammar": "connect <srcid> <relation> <tgtid> [--detail \"...\"]",
  "fix_example": "connect concept:fridge USES process:comp",
  "canonical_equivalent": "<g> edge add concept:fridge USES process:comp"
}
```

Error categories: `unknown_verb`, `bad_id_format`, `unknown_type`,
`unknown_relation`, `edge_type_mismatch`, `missing_required_field`,
`bad_value_range`, `graph_not_found`, `node_not_found`, `ambiguous`.

---

## 8. Implementation plan

### 8.1 New module: `src/scl.rs`
- `pub fn parse_line(line: &str, ctx: &Ctx) -> Result<CanonicalLine, SclError>`
- `pub fn parse_script(script: &str, ctx: &Ctx) -> Result<Vec<CanonicalLine>, SclError>`
- `Ctx { graph: String, strict: bool }`
- `CanonicalLine` = enum mirroring `cli::Command`/`GraphCommand` so we can
  hand off to existing `execute()` in `lib.rs:372` without re-implementing ops.

### 8.2 Hook in `src/bin/kg-mcp.rs`
- In the `kg` tool handler (~line 1205), before `tokenize_command`:
  1. Try `scl::parse_script(input, &ctx)`.
  2. If first token is a known verb → use SCL path.
  3. Else → fall through to existing `tokenize_command` + CLI path
     (backward compatible).
- Update `kg` tool description to embed the SCL cheat-sheet (§4) so LLMs
  see the grammar in the tool schema itself.

### 8.3 Schema helpers
- Reuse `validate::VALID_NODE_TYPES`, `VALID_RELATIONS`,
  `type_to_prefix` for `list types` / `list relations` and for inference.
- Add `validate::prefix_to_type` reverse lookup for ID→type inference.

### 8.4 Tests (`src/scl.rs` unit tests + integration)
Cover every row in §5 plus edge cases:
- quoted multiword args
- `--source` override disables synthetic source
- strict mode rejects missing fields
- unknown verb falls through to CLI
- `use` persists across `;`-separated lines
- `feedback` line inside a script
- ID with unknown prefix → actionable error
- relation invalid for given source/target type → actionable error
- empty script → returns help, not error

### 8.5 Docs
- This file (`docs/scl.md`) is the spec.
- Update `kg_help` output to print SCL cheat-sheet first.
- Add a short "SCL quickstart" section to README.

### 8.6 Migration / rollout
1. Ship SCL as opt-in (first-token-is-verb detection).
2. SCL is the primary interface — the three tools (`kg`, `kg_help`, `kg_schema`) are the only surface.
3. No breaking change to existing CLI scripts.

---

## 9. Critical analysis

**Strengths**
- Thin layer, no new ops, reuses `execute()` → low risk.
- Backward compatible (passthrough).
- Defaults remove the top LLM failure modes (#3,#4,#5) without weakening
  strict validation for human callers.
- Single entry tool reduces the "which tool do I call?" confusion.

**Weaknesses / open risks**
- **Implicit graph** assumes a server-configured default. If the MCP
  server serves multiple graphs with no default, `use` becomes mandatory
  and LLMs may still forget it. Mitigation: server config must set a
  default; `kg` tool description must state it.
- **Synthetic `source`** (`OTHER scl:<id>`) lowers data quality. Acceptable
  for agent-authored nodes, but strict mode should be the default for
  human/curated workflows. Document clearly.
- **Type inference from ID prefix** depends on the prefix→type map staying
  in sync with `validate.rs`. Must be generated from the same source, not
  duplicated, or it will drift.
- **Verb `add` vs `create` vs `connect` vs `add edge`**: four aliases for
  two operations. Convenient for LLMs but may confuse humans reading logs.
  Keep aliases, but canonicalize in output so logs always show `node add` /
  `edge add`.
- **No transaction semantics**: a multi-line SCL script runs line-by-line;
  a failure mid-script leaves partial changes. For batch adds, prefer the
  existing `edge add-batch` atomic path. Consider an optional
  `begin ... end` block in v2.
- **Feedback inside script** is convenient but mixes read/write with
  training-signal capture. Keep it, but log separately.
- **`remove` ambiguity**: `remove x HAS y` (edge) vs `remove x` (node).
  Grammar resolves via the `edge` keyword or by detecting
  `<id> <RELATION> <id>` shape, but LLMs may still write
  `remove x y z`. Mitigation: if 3 barewords and middle is uppercase
  relation → edge; else if 1 bareword → node; else actionable error.
- **Localization**: SCL is English-only. Fine for LLMs; humans using other
  languages must use canonical CLI. Document.

**Recommendation**: implement §8.1–8.4 as the first increment; ship behind
the verb-detection gate; measure LLM error rate before/after; iterate on
defaults and error messages based on real failures. Defer `begin/end`
transactions and typed-tool deprecation to v2.

---

## 10. SCL cheat-sheet (for tool description / `help`)

```
find <q>              search nodes
get <id>              fetch one node
add <id> [--name ..]  create node (type from id prefix)
modify <id> [--..]    update node fields
remove <id>           delete node
remove edge <s> <R> <t>           delete edge
connect <s> <R> <t>  create edge  (alias: add edge)
disconnect <s> <R> <t>           delete edge (alias: remove edge)
list nodes|edges|types|relations|graphs
stats
help [verb]
feedback <uid> yes|no|nil|pick <n>
use <graph>            set active graph
strict                 disable defaults for following lines

IDs: <type>:snake_case   e.g. concept:fridge, bug:door_seal
Relations: HAS USES STORED_IN TRIGGERS CREATED_BY AFFECTED_BY
           AVAILABLE_IN DOCUMENTED_IN DEPENDS_ON TRANSITIONS
           DECIDED_BY GOVERNED_BY READS_FROM
Flags after positional args. Quote multiword values.
```