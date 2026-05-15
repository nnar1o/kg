goal: Review the new multi-language automatic indexing feature, then fix the most critical issues one by one.
context: User asked to "rozpisz i popraw po kolei" after a critical review checklist. Feature already exists and needs sequential QA-driven improvements.
constraints: Use subagents; keep each change minimal; follow researcher -> coder -> tester loop; do not widen scope without evidence.
done_criteria: Critical issues are identified, the first fix is implemented and verified, and the user receives a clear order of remaining work.
expected_output: A ranked fix plan plus one implemented and tested improvement if feasible in this pass.
status: done
summary: Completed the collision-proof ID fix, section-ID stability fix, Python parser-coverage improvement, and the first reindex performance optimization.
artifacts: Cargo.toml; src/code_symbols.rs; src/auto_update.rs; src/document_sections.rs; tests/graph_mutation.rs; tasks/multi-language-index-qa-20260508/{orchestrator,researcher,coder,tester}.md
risks: Remaining gap is the linear `ensure_generated_edge()` hotspot and still-limited e2e reindex coverage.
next_action: Report completion and note the next likely optimization.
