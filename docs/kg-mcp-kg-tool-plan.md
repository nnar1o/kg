# kg MCP: shell-like `kg` tool plan

## Goal
Expose a single MCP tool named `kg` that accepts a shell-like script and runs multiple kg commands separated by `;` or newlines. This gives an LLM-friendly one-line interface for mixed feedback + queries.

## API surface
- Tool name: `kg`
- Args:
  - `script: string`
  - `mode?: "best_effort" | "strict"` (default `best_effort`)

## Script semantics
- Delimiters: `;` and `\n` split commands, but only at top level (not inside quotes).
- `\;` escapes a literal semicolon.
- Leading `#` in a command means comment, skip it.
- Empty commands are ignored.

## Feedback lines
Recognized without full kg parsing:
- `uid=XXXXXX YES|NO|NIL`
- `uid=XXXXXX PICK N`
- Optional alias: `feedback uid=XXXXXX YES`

## kg commands
Commands look like the CLI without the `kg` binary:
- `fridge node find "smart fridge"`
- `fridge node get concept:refrigerator`
- `fridge stats --by-type`
Aliases accepted:
- `kg <graph> ...`
- `graph <graph> ...`

## Execution model
- Commands are executed sequentially in order.
- Feedback lines are buffered and flushed as a batch before the next non-feedback command, using existing batch log logic.
- `node find` and `node get` run through the uid-aware handlers so `NUDGE` + `structured_content.requires_feedback` is preserved.
- All other commands run via `execute_kg`.

## Shell-words rules (tokenization)
Minimal, deterministic shell-words behavior:

- Whitespace separates tokens when not inside quotes.
- Single quotes (`'...'`): everything inside is literal; no escaping.
- Double quotes (`"..."`): contents are literal except `\` escapes the next character (including `"`).
- Backslash outside quotes escapes the next character (including whitespace and `;`).
- Unterminated quotes are errors.

Examples:
- `fridge node find "smart fridge"` → tokens `fridge`, `node`, `find`, `smart fridge`
- `note\;extra` → token `note;extra`
- `"a;b"; c` → command 1: `"a;b"`, command 2: `c`

## Structured output
Return `structured_content` with:
- `steps[]`: `{ cmd, kind: "kg"|"feedback", ok, stdout?, error?, requires_feedback? }`
- `requires_feedback[]`: aggregated `requires_feedback` objects from node find/get

## Tests
Unit tests:
- split/quote handling (`;` + newline + escaped `;`)
- tokenization (quotes, backslash, errors)
- node find/get argument parsing

## Rollout strategy
- Keep existing single-action tools but deprecate in descriptions: "Prefer `kg` for multi-command scripts".
- Update README and (optionally) skills to show `kg` one-liner examples.

## Examples
```text
uid=ab12cd YES; fridge node find "smart fridge"; fridge node get concept:refrigerator
```

```text
# Collect feedback and inspect a single node
uid=ab12cd PICK 2
fridge node get concept:refrigerator
```

```text
# Escaped semicolons stay within a token
fridge node find note\;extra
```
