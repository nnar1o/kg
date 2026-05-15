status: in_progress
summary: Extract annotate pipeline from output.rs into a dedicated module, keep rendering wrapper thin, and preserve tests/behavior.
artifacts: src/annotate.rs (new), src/output.rs, src/lib.rs
risks: Borrowing and string slicing details; ensure offsets remain byte-correct.
next_action: Patch code and run targeted validation.
