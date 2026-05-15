goal: Determine the exact current automatic indexing behavior and the documented intended format coverage.
context: Need to answer whether auto graph generation parses multiple document formats and creates nodes from them, without guessing.
constraints: Research only; verify in README, docs, and src; capture exact filenames/lines; do not edit code.
done_criteria: List current implemented formats/outputs and planned/specified formats with citations.
expected_output: A compact findings note with implementation vs spec.
status: done
summary: Current auto-update scans directories, generates GDIR/GFIL nodes, and extracts Rust symbols only; broader document parsing is spec-only.
artifacts: README.md:116-120; docs/auto-nodes-requirements.md:228-240, 268-315; src/auto_update.rs:253-425, 586-634
risks: README was ambiguous about how much parsing is actually implemented.
next_action: Apply the README clarification and validate wording.
