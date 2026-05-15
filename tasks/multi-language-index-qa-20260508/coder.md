goal: Implement the first high-priority fix from research with the smallest safe diff.
context: Apply only the first fix that reduces the most risk without changing scope.
constraints: Modify only the files needed; keep existing behavior stable; add or update tests.
done_criteria: Target bug/regression is fixed and covered by tests.
expected_output: Minimal patch plus test updates.
status: done
summary: Optimized `ensure_generated_edge()` by adding a local edge index per root update and added a focused regression test.
artifacts: src/auto_update.rs; tests in auto_update.rs
risks: Some stale index entries can remain after renames, but behavior stays correct because lookups fall back to scan.
next_action: Move to the next QA item.
