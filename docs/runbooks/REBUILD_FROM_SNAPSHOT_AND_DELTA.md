# Rebuild from Snapshot and Delta (v2.2)

## Goal
Recover chain state by replaying retained blocks on top of the latest validated snapshot, with explicit validation and fallback behavior.

## Preconditions
- `/health` and `/readiness` are reachable.
- Operator has current snapshot/replay-plan visibility.
- Incident artifacts were captured before mutation.

## Operator steps
1. Inspect replay prerequisites:
   - `curl -s http://127.0.0.1:8080/snapshot | jq`
   - `curl -s http://127.0.0.1:8080/sync/replay-plan | jq`
   - `curl -s http://127.0.0.1:8080/sync/rebuild-preview | jq`
2. If no snapshot is present, create one first:
   - `curl -s -X POST http://127.0.0.1:8080/snapshot/create | jq`
3. Optional prune (after snapshot validation):
   - `curl -s -X POST http://127.0.0.1:8080/prune -H 'content-type: application/json' -d '{"keep_recent_blocks":64}' | jq`
4. Execute rebuild using snapshot + delta path:
   - `curl -s -X POST http://127.0.0.1:8080/sync/rebuild -H 'content-type: application/json' -d '{"force":true,"allow_partial_replay":false,"persist_after_rebuild":true,"reconcile_mempool":true}' | jq`
5. Validate post-rebuild coherence:
   - `curl -s http://127.0.0.1:8080/sync/verify | jq`
   - `curl -s http://127.0.0.1:8080/maintenance/report | jq '.data.state_audit'`
   - `curl -s 'http://127.0.0.1:8080/maintenance/report?deep=true' | jq '.data.state_audit'`
   - `curl -s http://127.0.0.1:8080/status | jq`
   - `curl -s http://127.0.0.1:8080/readiness | jq`
   - `curl -s 'http://127.0.0.1:8080/runtime/events?limit=50' | jq`

## Expected outcomes
- `sync/rebuild` succeeds with `rebuilt=true` and `consistency_ok=true`.
- Best height and selected tip match expected chain progression.
- Runtime logs include restore drill completion evidence when drill instrumentation is used.
- `maintenance/report` includes read-only `state_audit` output with explicit pass/fail issues.
- Use `maintenance/report?deep=true` only for manual, opt-in deep verification runs.

## Corrupt/invalid snapshot behavior
- If snapshot decode or snapshot+delta replay fails and persisted blocks exist, storage falls back to full replay from persisted blocks and emits warning runtime events.
- If snapshot decode fails and no persisted blocks are available, restore fails explicitly (no partial block mutation).

## Drill repeatability
Use:

```bash
scripts/restore-drill-evidence.sh
```

This command runs targeted storage tests that assert:
1. Snapshot + delta restore reproduces expected best state.
2. Corrupt snapshot fallback is safe (or explicit fail when fallback is impossible).
3. Prune + replay remains coherent post-restore.
4. Restore timing/evidence is emitted in a reproducible report path.
