# Changelog

## [0.1.10] - 2026-03-31

### Added
- per-request debug output for MCP `kg` commands plus richer trace and error metadata

### Changed
- refactor CLI dispatch into app modules and move integration tests out of the main binary
- update GitHub Actions runner labels to match the self-hosted runner setup

## [0.1.8] - 2026-03-25

### Added
- kg_gap_summary tool and kg-assistant skill for collaborative graph improvement
- feedback-summary command with human-readable insights
- shell-like kg MCP tool for multi-command workflows
- kg init prompts and doc skill guidance
- JSON graph storage abstraction
- configurable graph store selection
- first-class notes to graphs
- BM25 search mode
- node rename and graph diff
- graph merge command
- async TUI graph browser
- as-of export from graph backups
- graph backup history command
- diff-as-of for graph backups
- include notes in graph diffs
- show field-level changes in graph diffs
- JSON output for graph history
- event log timeline for temporal queries
- filter timeline snapshots by time range
- redb backend and JSON import/export
- benchmarking suite for large graphs
- UX improvements: optional feedback, edge batch, schema tool, write-time validation

### Changed
- Move feedback-summary under graph subcommand
- Persist feedback signals and add graph backups
- Refactor: remove dead code, fix errors, improve code quality
- Update README: add beta notice, emphasize MCP integration
- Simplify README, add docs/mcp.md
- Add release build, install.sh, and GitHub Actions workflow
- Add release notes
- Add explicit contents:write permission
- Bump version for new release
- Fix README badges
- Fix CI branch and install URL
- Prepare 0.1.4 and 0.1.5 releases for crates.io
- Rename crate to kg-cli for publishing
- Fix clippy warnings and add ops unit tests
- Remove SSH keys from git and add to .gitignore
- Plan future sprints
- Sprints 12, 13, 15, 16, 17: Schema validation, KQL v1, vectors, interop, git-friendly
- Sprint 14: Persistent BM25 index infrastructure
- Sprint 14: integrate persistent BM25 index into search queries

### Fixed
- Fix clippy warnings
- Fix README badges
- Fix CI branch and install URL
