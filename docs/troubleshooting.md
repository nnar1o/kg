# Troubleshooting

This page covers the most common issues reported by new users.

## I do not know which command shape to use

Use the explicit pattern first:

```sh
kg graph <graph-name> <command> ...
```

Example:

```sh
kg graph fridge node find cooling
```

Some shorthand commands still work (`kg fridge ...`), but the `kg graph ...` form is clearer and easier to teach.

## `cargo run` says it cannot determine which binary to run

This project exposes multiple binaries. Choose one explicitly:

```sh
cargo run --bin kg -- --help
cargo run --bin kg-mcp -- --help
```

## I cannot find my graph file after migration

Default runtime prefers `.kg`. If you started with `.json`, a side-by-side `.kg` may have been created.

Check both files:

```sh
ls *.json *.kg
```

If you need old behavior temporarily:

```sh
kg graph <graph-name> --legacy stats
```

## Why are `.kgindex` and `.kglog` files appearing?

They are sidecar files for `.kg` graphs:

- `.kgindex` - line index to speed up lookups
- `.kglog` - hit/feedback log used by tooling and analysis

They are expected and managed automatically.

## `node get` / `node find` returns nothing

Check these in order:

```sh
kg list --full
kg graph <graph-name> node find <query> --limit 20
kg graph <graph-name> node find <query> --full
```

Common causes:

- wrong graph name
- wrong node ID prefix (for example `concept:` vs `process:`)
- searching by term not present in `name/description/facts/aliases`

## Validation says my graph is invalid

Run:

```sh
kg graph <graph-name> check --errors-only
```

Typical fixes:

- missing required node fields (`type`, `name`)
- unsupported relation for a source/target type pair
- `importance` outside `1..=6`

For stricter format checks:

```sh
KG_STRICT_FORMAT=1 kg graph <graph-name> check
```

## MCP server starts but client does not show tools

Verify the configured command path is absolute and executable.

Minimal config pattern:

```json
{
  "mcpServers": {
    "kg": {
      "command": "/absolute/path/to/kg-mcp"
    }
  }
}
```

Also confirm `kg-mcp` runs from terminal without crashing.

## Import commands fail

First check command-specific help:

```sh
kg graph <graph-name> import-csv --help
kg graph <graph-name> import-md --help
```

Then validate input format against:

- [`docs/import-csv.md`](import-csv.md)
- [`docs/import-markdown.md`](import-markdown.md)

## Still stuck?

Collect these outputs before reporting an issue:

```sh
kg --version
kg list --full
kg graph <graph-name> check --limit 100
```

This usually makes support much faster.
