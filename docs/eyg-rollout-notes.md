# EYG Rollout Notes

## Scope
- Default runtime works on `*.kg` with JSON -> KG auto-migration (side-by-side, source JSON is kept).
- Legacy runtime remains available via `kg graph <name> --legacy`.
- Sidecars are active for KG graphs:
  - `<graph>.kglog` for hit/feedback events,
  - `<graph>.kgindex` for node header line index.

## Migration behavior
- When `<name>.kg` exists, runtime uses it.
- When `<name>.kg` is missing and `<name>.json` exists, runtime migrates JSON to KG.
- Migration writes `<name>.migration.log` with mapping counters and warnings.
- Migration normalizes historical aliases for node types/relations and rewrites incoming relation forms (`<-`) to outgoing edges.

## Strict parser mode
- Default parser mode is compatibility-first (permissive length checks/order acceptance).
- Optional strict mode can be enabled with environment variable:

```bash
KG_STRICT_FORMAT=1 kg graph fridge node get concept:refrigerator
```

- In strict mode, KG parser enforces:
  - field length limits,
  - non-decreasing canonical field order in node/note/edge blocks.

## Rollback plan
1. For immediate rollback on a graph command, use `--legacy`.
2. To return to JSON-only operation for automation, append `--legacy` in scripts/aliases.
3. Keep generated `*.kg` files for later replay; source `*.json` remains unchanged.
4. If KG sidecar files are corrupted, remove `*.kgindex`/`*.kglog`; they are rebuilt or recreated lazily.

## Operational checks
- Smoke check:

```bash
kg graph fridge node get concept:refrigerator
kg graph fridge node find lodowka
kg graph fridge check
```

- Verify migration artifacts after first KG read:
  - `fridge.kg`
  - `fridge.kgindex`
  - `fridge.kglog`
  - `fridge.migration.log`
