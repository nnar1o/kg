goal: Inspect the current indexing implementation and identify the highest-risk issues in order.
context: Focus on the multi-language indexer, document sections, and generated-node stability.
constraints: Research only; cite files/functions; prioritize bugs, then semantics, then performance; do not edit code.
done_criteria: Provide a ranked list of concrete issues with evidence and recommend the first fix.
expected_output: Short research report with the top issue and a safe fix path.
status: done
summary: Recommended the next optimization as a local edge index for `ensure_generated_edge()` because it was the safest high-impact remaining hotspot.
artifacts: src/auto_update.rs
risks: End-to-end reindex coverage is still limited.
next_action: Continue with the next QA target.
