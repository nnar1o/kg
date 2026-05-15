goal: Extend automatic indexing so code parsing covers multiple languages and parsed documents create chapter sub-nodes with chapter content.
context: User уточnił that auto parsing should work for languages like Java, JS, and C++, and that document parsing should add chapters as sub-nodes containing their content.
constraints: Use subagents; keep changes minimal and focused; preserve existing behavior; verify whether current architecture already supports or partially supports this before coding.
done_criteria: Docs and/or code reflect multi-language symbol extraction plus chapter sub-nodes for parsed documents; tests validate at least 5-10 regressions, 1 edge, 1 negative deterministic and fast.
expected_output: Implemented change with concise validation summary.
status: in_progress
summary: Orchestrating research on current parser coverage and document hierarchy handling.
artifacts: tasks/multi-language-doc-chapters-20260508/orchestrator.md
risks: Overstating parser support or creating noisy graph bloat with chapter nodes.
next_action: Launch researcher to inspect current indexing/parsing implementation.
