# kg v0.2.4

`kg-mcp` is a lightweight local alternative to classic RAG systems when you want structured, editable, git-friendly project memory instead of document chunk retrieval.

This release focuses on find quality and observability: score is now always visible, scoring internals are debuggable, and ranking behavior is calibrated to be more stable across real-world query patterns.

## Highlights

- always return `score` in `node find` output (CLI and JSON)
- add `--debug-score` with detailed score breakdown (raw relevance, normalization, lexical boost, authority cap/application)
- recalibrate ranking with normalized relevance + capped authority (`feedback` and `importance`)
- improve fuzzy and BM25 matching behavior (facts/notes coverage, phrase/token handling, Unicode tokenization)
- optimize find runtime with per-query cached neighbor/note context and token-based BM25 document reuse
- extend baseline quality report with `ndcg@k`

## Why this release matters

- users can now inspect and trust ranking decisions directly from `find`
- rankings are less sensitive to outliers and metadata over-dominance
- better relevance for multi-token and cross-field queries while preserving typo tolerance
- improved search throughput on larger graphs via less repeated work

## Installation

```sh
curl -sSL https://raw.githubusercontent.com/nnar1o/kg/master/install.sh | sh
```

You can also download a release binary from GitHub Releases.

## Quick start

1. Add `kg-mcp` to your MCP client.
2. Restart the client.
3. Tell the assistant which graph belongs to the project.
4. Ask it to build or extend the graph from your docs.
5. Keep using the same graph in later conversations.

Minimal project prompt:

```text
You are connected to kg-mcp.
Project graph name: payments.
Use this graph for relevant reads and updates in this project.
If you notice important missing information that is grounded in the available docs or conversation context, update the graph as part of your work.
If uncertain, ask or add a note instead of inventing facts.
```
