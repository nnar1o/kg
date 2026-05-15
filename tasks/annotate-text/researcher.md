goal: Propose a lightweight Rust-only design for `kg annotate` that filters colloquial filler and produces high-precision inline annotations.
context:>
  Existing code already has `text_norm` normalization, synonym expansion, BM25 indexing, and CLI subcommand patterns. Need a practical approach for Polish and English-ish free text.
constraints:
  - Local only; no network or hosted NLP.
  - Favor existing crate ecosystem already present if possible.
  - Avoid adding heavy ML/NLP dependencies unless strongly justified.
done_criteria:
  - Recommend algorithm stages for candidate extraction, filtering, span selection, and output formatting.
  - Identify any small crates worth adding and why.
  - Provide edge cases and failure modes to guard against.
expected_output: A concise design note with implementation guidance suitable for handoff to coder.
status: done
summary: >
  Recommend a pure Rust, high-precision lexical annotator with no new crates for v1.
  Reuse text_norm + Bm25Index, filter stopwords/fillers, generate short candidate spans,
  score exact/alias/fact/synonym overlaps, and render non-overlapping annotations greedily.
artifacts:
  - design recommendation: reusable output renderer in src/output.rs
  - filtering recommendation: extend src/text_norm.rs with filler list
  - integration recommendation: new CLI command and non-mutating dispatch
risks:
  - Polish inflection remains heuristic.
  - Generic terms may over-annotate without conservative thresholds.
  - Overlap resolution needs stable tie-breaking.
next_action: Hand off to coder to implement CLI, renderer, and tests.
