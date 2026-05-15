goal: Implement a new `kg graph <graph> annotate` command that annotates input text with graph-backed keyword/entity markers.
context:>
  User asked for a local, Rust-only, lightweight implementation. The feature should find relevant words/phrases in free text and append inline annotations like `[kg odkurzacze @K:odkurzacz]` when a graph node/fact matches.
constraints:
  - Rust only; no external services.
  - Keep dependencies lightweight.
  - Prefer reuse of existing text normalization/search utilities.
  - Do not regress existing CLI behavior.
done_criteria:
  - New `annotate` CLI command exists and is wired into dispatch.
  - Annotation algorithm uses graph data and existing normalization/synonym machinery.
  - Includes tests for regression, edge, and negative cases.
  - `cargo test` passes for relevant suite.
expected_output: Short design/implementation summary, files changed, and validation results.
status: done
summary: "Implemented the annotation command, cleaned the clippy baseline, and passed both clippy and cargo test."
artifacts:
  - tasks/annotate-text/researcher.md
  - tasks/annotate-text/coder.md
  - tasks/annotate-text/tester.md
risks:
  - Polish stemming/lemmatization quality may remain heuristic.
  - Annotation collisions may need conservative thresholds.
next_action: None.
cached_tokens: 0
max_coder_tester_loops: 2
max_runtime_minutes: 45
max_budget: low
