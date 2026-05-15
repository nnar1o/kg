goal: Implement the smallest safe change to support multi-language code parsing and document chapter sub-nodes.
context: After research, add/adjust code and tests so indexing reflects the requested behavior.
constraints: Minimal diff; prefer reusing existing parsers/libraries; preserve generated-node ownership rules; ensure tests are fast and deterministic.
done_criteria: Code parses more than Rust for symbols where feasible and documents create chapter sub-nodes with content; tests cover regressions/edge/negative cases.
expected_output: Code patch plus test results.
status: pending
summary: 
artifacts: 
risks: Need to avoid adding unsupported formats without parser coverage.
next_action: Apply implementation after research.
