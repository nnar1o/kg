# Build a Graph from Documentation (Step by Step)

Use this guide when you have raw project docs and want to create a clean, useful graph from scratch.

This flow works both manually in CLI and through an AI client connected to `kg-mcp`.

## 1) Define scope before adding anything

Decide what this graph is for:

- one project area (`payments`, `auth`, `search`), or
- one incident (`incident-2026-04-01`), or
- one architecture slice (`billing-runtime`).

Do not mix unrelated domains in one graph.

## 2) Create graph and choose naming rules

Create a graph:

```sh
kg create <graph-name>
```

Use stable IDs from day one:

- format: `<type>:<snake_case_name>`
- examples: `process:order_checkout`, `datastore:orders_db`, `rule:retry_policy`

Use one canonical ID per concept (avoid synonyms as separate nodes).

## 3) Extract source facts from documentation

When reading docs, extract only verifiable facts:

- what it is (name + type)
- what it does (description)
- key facts (constraints, SLAs, assumptions)
- dependencies and data flow (edges)
- source file links (where the fact came from)

If something is uncertain, keep confidence lower and avoid over-linking.

## 4) Add high-value nodes first

Start from nodes that anchor the domain:

- core concepts,
- main processes,
- critical datastores/interfaces,
- key rules/decisions.

Example:

```sh
kg graph <graph-name> node add concept:checkout --type Concept --name "Checkout" --description "Order finalization flow" --importance 5
kg graph <graph-name> node add process:authorize_payment --type Process --name "Authorize payment" --importance 6
kg graph <graph-name> node add datastore:orders_db --type DataStore --name "Orders DB" --importance 5
```

## 5) Connect nodes with semantic relations

Add edges only when relation meaning is clear from docs:

```sh
kg graph <graph-name> edge add process:authorize_payment READS_FROM datastore:orders_db --detail "reads order state"
kg graph <graph-name> edge add concept:checkout DEPENDS_ON process:authorize_payment --detail "checkout requires successful authorization"
```

Prefer fewer, correct edges over many vague edges.

## 6) Run quality checks in short loops

After each batch (for example every 10-20 nodes):

```sh
kg graph <graph-name> check --errors-only
kg graph <graph-name> quality missing-descriptions
kg graph <graph-name> quality missing-facts
```

Fix issues immediately, then continue extraction.

## 7) Add notes for context that should not become hard facts

Use notes for caveats, temporary context, and interpretation boundaries:

```sh
kg graph <graph-name> note add concept:checkout --body "Assumes synchronous gateway response in v1" --tag assumption
```

## 8) Validate search quality

Run several realistic queries and confirm useful hits:

```sh
kg graph <graph-name> node find "payment timeout"
kg graph <graph-name> node find "retry policy"
kg graph <graph-name> node get process:authorize_payment --full
```

If results are weak, improve names/descriptions/facts rather than adding random edges.

## 9) Production hardening pass

Before sharing the graph:

- run strict checks once:

```sh
KG_STRICT_FORMAT=1 kg graph <graph-name> check
```

- verify no critical check errors,
- verify core nodes have descriptions + sources,
- verify key flows are represented by explicit edges.

## 10) If you build graph with AI + `kg-mcp`

Use this process:

1. Give AI clear scope and source docs list.
2. Force AI to work in batches (draft -> add -> check -> fix).
3. Require AI to run `check`/`quality` after each batch.
4. Approve only grounded nodes/edges (must point to source docs).

Ready prompt template: [`docs/ai-prompt-graph-from-docs.md`](ai-prompt-graph-from-docs.md).
