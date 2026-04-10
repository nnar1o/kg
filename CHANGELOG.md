# Changelog

## [0.2.10] - 2026-04-11

### Added
- shared text normalization module used by both `score-all` and `node find`, including mandatory pure-number token removal, EN/PL stopword filtering, stemming, and synonym canonicalization
- `node find --mode hybrid` with weighted BM25 + fuzzy fusion and query rewrite/expansion
- optional `--tune bm25=...,fuzzy=...,vector=...` for controlling hybrid ranking weights

### Changed
- default `node find` mode is now `hybrid`
- score calculators use IDF-weighted overlap for description and attribute bundle components
- `score-all` cache metadata now marks normalization version (`normalization=v2`)

## [0.2.9] - 2026-04-11

### Added
- `score-all` now creates deterministic similarity clusters in the score cache graph, with cluster nodes (`@`) and `HAS` membership edges carrying strength in `d`
- new `clusters` command to inspect top clusters sorted by relevance from the latest score cache snapshot
- `clusters --skill gardener` mode for AI-assisted triage with action-oriented cluster output

### Changed
- `clusters` resolves the latest `<graph>.score.<ts>.kg` automatically so assistants do not need direct cache traversal
- edge type rule enforcement now applies only to built-in core types, allowing custom cluster type `@` membership edges

## [0.2.4] - 2026-04-10

### Added
- always return `score` for `kg <graph> node find` in CLI and JSON output
- add `--debug-score` to expose score breakdown (`raw_relevance`, normalization, lexical/authority components)
- add `ndcg@k` to baseline golden-set metrics alongside hit-rate/top1/MRR

### Changed
- recalibrate find scoring with normalized relevance and capped authority boost (`feedback` + `importance`)
- improve fuzzy ranking coverage for `key_facts`, attached notes, and neighbor context while preserving entity-first ranking
- tighten BM25 lexical boost behavior for phrase/token matching and improve Unicode token handling
- optimize find performance by caching notes/neighbors per query and reusing tokenized BM25 documents with field weighting

## [0.2.3] - 2026-04-08

### Fixed
- restore `graph-example-fridge.json` so the full test suite and packaged crate verification build cleanly again

### Changed
- refine README wording around local-first git-friendly project memory

## [0.2.2] - 2026-04-08

### Changed
- keep `kg --help` focused on command help by disabling the ASCII logo in default output while retaining the banner in code
- refine the README into an MCP-first end-user guide for `kg-mcp`
- document graph storage and git maintenance for `~/.kg/graphs`, including recommended ignores for `*.kgindex`, `*.event.log`, `*.migration.log`, `*.bak`, and `*.bck.*.gz`
- update release docs to match the current `kg-mcp` workflow and Apache 2.0 licensing

## [0.1.18] - 2026-04-07

### Changed
- add ASCII banner to top-level `kg --help`
- refresh `kg --help` command descriptions and examples to match the current preferred `kg graph <graph> ...` workflow
- update MCP tool descriptions and docs to reflect the current tool set, including batch options and deprecated compatibility tools
- rewrite README as an MCP-first end-user guide instead of CLI-first onboarding
- document end-user workflows for generating graphs from docs, querying existing graphs, and updating graph facts through the assistant
- document local graph storage in `~/.kg/graphs`, git usage for `*.kg` graphs, ignoring `*.kgindex` and `*.kglog`, and HTML export tips
- change project license from MIT to Apache 2.0

## [0.1.17] - 2026-04-04

### Fixed
- publish crate under the correct crates.io package name: `kg-cli`

## [0.1.16] - 2026-04-04

### Fixed
- preserve full node IDs (including type prefix) in `.kgindex` for both native and legacy header shapes
- create new graphs as `.kg` by default (instead of `.json`) in the JSON-compatible runtime
- skip empty `E` and `P` lines when serializing `.kg` nodes

### Changed
- make installer script (`curl ... | sh`) the first install option in README for end-user onboarding

## [0.1.15] - 2026-04-04

### Added
- native text `.kg` parser/serializer with deterministic ordering, native note blocks, and edge validity fields (`i`/`x`)
- `.kg` sidecars: `<graph>.kgindex` (lazy rebuild + invalidation) and `<graph>.kglog` (`H`/`F` events)
- config-backed persistent `user_short_uid` in `.kg.toml` used by sidecar logging and MCP feedback writes
- smart JSON->KG migration report at `<graph>.migration.log` with mapping and rewrite statistics
- optional strict KG format mode via `KG_STRICT_FORMAT=1` (field order + length enforcement)
- onboarding docs: getting started guide, troubleshooting guide, doc-to-graph playbook, and ready AI prompt for `kg-mcp`

### Changed
- enforce relation semantic source/target compatibility as validation errors
- default runtime migration now performs semantic conversion from JSON to native text `.kg` instead of raw file copy
- node get path uses kgindex lookup hint with sorted-ID fast path fallback
- README and MCP docs now emphasize `kg graph <graph> ...` command shape and link project AI skills
- repository skills updated to current runtime and supported relation set

### Fixed
- access-log path compatibility between `.json` and `.kg` after migration
- graceful behavior when sidecar paths are missing or unwritable (best-effort sidecar updates)

## [0.1.14] - 2026-03-31

### Fixed
- switch the macOS Intel release runner to the supported `macos-15-intel` GitHub Actions label so multi-arch releases complete successfully

## [0.1.13] - 2026-03-31

### Changed
- build release artifacts for Linux, macOS Intel, and macOS Apple Silicon in GitHub Actions
- publish per-target binaries and update `install.sh` to download the correct asset for the current OS/architecture

## [0.1.12] - 2026-03-31

### Added
- configurable MCP feedback `nudge` probability in `.kg.toml` with default `20` and validation for values from `0` to `100`

### Changed
- show MCP nudges probabilistically based on config while keeping structured feedback metadata available

## [0.1.11] - 2026-03-31

### Changed
- run CI and release workflows on GitHub-hosted `ubuntu-latest` runners instead of self-hosted runners

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
