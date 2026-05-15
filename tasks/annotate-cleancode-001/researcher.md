status: done
summary: Initial code review shows annotate code is concentrated in src/output.rs and relies on text_norm + Bm25Index. Best cleanup is modular extraction and a small config object.
artifacts: src/output.rs annotate section, src/text_norm.rs, src/cli.rs, src/lib.rs
risks: Over-refactoring could alter span selection or formatting; keep behavior stable.
next_action: Implement extraction and minimal simplification.
