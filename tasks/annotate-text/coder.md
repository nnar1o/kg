goal: Implement `kg graph <graph> annotate` in Rust using the approved lightweight design.
context:>
  Wait for researcher handoff before coding. Reuse existing CLI and graph utilities; keep changes minimal and covered by tests.
constraints:
  - Only modify code after design is confirmed.
  - Add tests that are deterministic and fast.
  - Keep output formatting stable and human-readable.
done_criteria:
  - Code compiles.
  - New command works on representative sample text.
  - Tests cover regression, edge, and negative scenarios.
expected_output: Implementation summary, touched files, and any caveats.
status: done
summary: >
  Added `kg graph <graph> annotate` with local Rust-only annotation rendering, span-aware
  tokenization, filler filtering, greedy non-overlap selection, and conservative scoring.
artifacts:
  - src/cli.rs
  - src/lib.rs
  - src/app/mod.rs
  - src/app/graph_annotate.rs
  - src/output.rs
  - src/text_norm.rs
  - tests/cli_smoke.rs
risks:
  - Repo-wide clippy baseline may still block a full warning-free gate.
next_action: Await tester verdict / baseline decision.
