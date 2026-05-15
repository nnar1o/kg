goal: Refactor annotate feature into cleaner, more modular Rust code without changing behavior.
context: The annotate command already works and is covered by tests. The main clean-code issue is that annotate logic lives inside src/output.rs alongside unrelated rendering code.
constraints: Keep implementation Rust-only and local; preserve current CLI behavior and output format; avoid changing non-annotate flows.
done_criteria: Annotate logic is moved into a dedicated module or clearly isolated helpers; output.rs is slimmer; tests still pass; clippy remains green.
expected_output: Small refactor diff, no functional regression, and concise validation summary.
