goal: Update documentation to include the missing clarification about automatic graph generation and supported formats.
context: After research, patch the most relevant docs so users understand current behavior and limitations.
constraints: Minimal diff; only documentation; keep wording precise; do not claim full DOCX/PDF parsing if not implemented.
done_criteria: Docs mention scanning directories, current recognized file types, generated nodes, and the implementation/spec gap.
expected_output: A small docs patch.
status: done
summary: Smallest safe fix is README-only wording that separates current Rust-focused extraction from planned multi-format document parsing.
artifacts: README.md replacement text for the Automatic graph section
risks: None beyond preserving exact accuracy.
next_action: Validate the final wording against code and requirements.
