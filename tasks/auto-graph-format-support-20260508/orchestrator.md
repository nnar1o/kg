goal: Add the missing documentation detail about what automatic graph generation currently parses and what it creates.
context: User asked to "add missing information - use subagents" after asking whether automatic graph generation parses different document formats and creates nodes from them.
constraints: Use subagents; keep changes minimal; only document accurate, verified behavior; do not overstate unimplemented parsing support.
done_criteria: README/docs clearly state current auto-indexing behavior and supported input formats; response confirms what is implemented vs planned; no unrelated changes.
expected_output: Short docs patch plus concise summary of current support and gap.
status: done
summary: Research, docs update, and tester validation completed successfully.
artifacts: README.md; tasks/auto-graph-format-support-20260508/{orchestrator,researcher,coder,tester}.md
risks: None remaining for this task.
next_action: Report completion to the user.
