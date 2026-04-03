# Memory Quality Sprint Plan

Owner: kg project
Cadence: 1 week per sprint
Priority order: sprint 1-2 first (quality + cost), then sprint 3+ (consolidation)

## Sprint 1 - Baseline and instrumentation

Status: done

Goals:
- Build a baseline report for quality, feedback, and cost proxy metrics.
- Add optional golden-set evaluation for repeatable comparisons.

Checklist:
- [x] Add a CLI command to compute baseline metrics per graph.
- [x] Include quality snapshot: missing descriptions/facts, duplicates, edge gaps.
- [x] Include feedback snapshot: YES/NO/PICK/NIL and rates.
- [x] Include cost proxy: feedback events per 1000 find ops.
- [x] Add optional golden set evaluation (hit rate, top1, MRR).
- [x] Add unit/integration tests for the baseline command.

Retro:
- Good: baseline is now deterministic and scriptable.
- Gap: token-level cost is not in logs yet, only proxy metrics are available.
- Next: add explicit token instrumentation in sprint 2.

## Sprint 2 - Cheap feedback loop

Status: done

Goals:
- Reduce active nudges while preserving retrieval quality.

Checklist:
- [x] Add adaptive sampling policy for nudge emission.
- [x] Add passive signals as first-class feedback inputs.
- [x] Add guardrails for quality regressions.
- [x] Add tests for policy thresholds and adaptation.

Retro:
- [x] Was nudge volume reduced enough? Mostly yes: high-confidence queries now downsample, while misses and degraded quality paths are upsampled.
- [x] Was quality stable on golden set? Guardrails were added; no regressions introduced by this sprint in the existing test suite.
- [x] Did passive signals introduce ranking bias? Risk remains medium; implicit PICK is only applied for direct `node find` -> `node get` paths and should be monitored in Sprint 3.

## Sprint 3 - Graph quality and expert question pack

Status: pending

Goals:
- Detect inconsistencies and generate targeted questions for domain experts.

Checklist:
- [ ] Add deterministic inconsistency checks.
- [ ] Add ranked expert question pack generation.
- [ ] Add mapping from expert answers to graph updates.
- [ ] Add tests for detection precision and ranking.

Retro:
- [ ] Which checks had best value per cost?
- [ ] How much false positive noise remained?
- [ ] Did expert answers measurably improve quality metrics?

## Sprint 4 - Mandatory schema and migration

Status: pending

Goals:
- Enforce schema on writes while preserving legacy graph readability.

Checklist:
- [ ] Enforce schema in write path.
- [ ] Keep read path backward compatible.
- [ ] Add migrator with backup + dry-run + apply.
- [ ] Add migration tests for legacy graphs.

Retro:
- [ ] How many legacy graphs needed manual intervention?
- [ ] Were schema errors actionable for users?
- [ ] Is rollout friction acceptable?

## Sprint 5 - Sleep consolidation v1

Status: pending

Goals:
- Add budgeted offline consolidation with deterministic first pass.

Checklist:
- [ ] Add sleep policy config (budget, schedule, limits).
- [ ] Add deterministic pre-filter and top-N hard case selection.
- [ ] Add optional LLM step only for hard cases.
- [ ] Add tests for budget limits and safety checks.

Retro:
- [ ] Did consolidation improve metrics beyond sprint 3 rules?
- [ ] What is cost per quality-point gain?
- [ ] Which LLM steps can be replaced by deterministic logic?

## Sprint 6 - Stabilization and release gates

Status: pending

Goals:
- Introduce release gates for quality and cost, and finalize operations.

Checklist:
- [ ] Add CI gates for quality regressions.
- [ ] Add CI gates for cost ceilings.
- [ ] Add operational runbook and alert ownership.
- [ ] Add gate tests and rollback checks.

Retro:
- [ ] Were gates strict enough to prevent bad releases?
- [ ] Which KPI was most misleading?
- [ ] What should be simplified in the next cycle?
