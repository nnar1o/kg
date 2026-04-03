# Ready AI Prompt: Build a Graph from Documentation with `kg-mcp`

Copy, fill placeholders, and send this to your AI assistant that has `kg-mcp` connected.

```text
You are building a knowledge graph from documentation using kg-mcp tools.

Goal:
- Create or update graph: <GRAPH_NAME>
- Scope: <SCOPE_DESCRIPTION>
- Source documents:
  1) <DOC_1>
  2) <DOC_2>
  3) <DOC_3>

Hard rules:
1) Only add facts grounded in source documents. If unclear, mark low confidence or skip.
2) Use command shape: kg graph <GRAPH_NAME> ... (or equivalent MCP tools).
3) Use stable IDs: <type>:<snake_case_name>.
4) Prefer semantic correctness over quantity (avoid speculative edges).
5) Work in batches of max 10 nodes at a time.
6) After each batch run:
   - kg graph <GRAPH_NAME> check --errors-only
   - kg graph <GRAPH_NAME> quality missing-descriptions
   - kg graph <GRAPH_NAME> quality missing-facts
   Then fix issues before next batch.
7) For each added node include:
   - type
   - name
   - description
   - importance (1..6)
   - at least one source file reference when available
8) Use notes for assumptions or temporary interpretation, not as hard facts.
9) At the end, run strict validation once:
   - KG_STRICT_FORMAT=1 kg graph <GRAPH_NAME> check
10) Do not delete existing nodes/edges unless explicitly asked.

Execution workflow:
Step A: Plan
- Propose an extraction plan from provided documents:
  - candidate node list,
  - candidate edge list,
  - unknown/ambiguous items.
- Wait for approval before writing.

Step B: Create graph if missing
- If graph does not exist, create it.

Step C: Batch ingestion loop
- For each batch:
  1) Add nodes.
  2) Add edges.
  3) Add notes for assumptions.
  4) Run check + quality commands.
  5) Apply fixes.

Step D: Final review
- Run summary commands:
  - kg graph <GRAPH_NAME> stats --by-type --by-relation
  - kg graph <GRAPH_NAME> node find <TOPIC_KEYWORDS>
  - kg graph <GRAPH_NAME> node get <2-3 CRITICAL NODE IDS> --full
- Report:
  - total nodes/edges,
  - open quality gaps,
  - assumptions added as notes,
  - suggested next docs to ingest.

Output format requirements:
- Show exact commands you executed.
- After each batch provide a short changelog (nodes/edges/notes added).
- If any command fails, include error and your fix.
```

## Optional: safer first run

For the first ingestion run, ask the assistant to execute Step A only and wait for your approval.
