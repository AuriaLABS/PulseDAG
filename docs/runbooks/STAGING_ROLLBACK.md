# Staging Rollback Validation Runbook (v2.2)

## Purpose
Provide an explicit rollback path for staging v2.2 upgrade validation, including snapshot/restore interaction and objective post-rollback health checks.

## Scope and guardrails
- Operational rollback only.
- No consensus changes.
- No miner changes.

## Trigger conditions
Initiate rollback if any of the following occurs after upgrade:
- `/sync/verify` reports `consistent=false`.
- `/readiness` reports `ready_for_release=false`.
- Service instability or unexpected runtime alerts.

## Rollback inputs
- Pre-upgrade baseline evidence directory.
- Previous known-good node artifact/configuration.
- Snapshot and persisted block data directory backup.

## Rollback package verification (before redeploy)
1. Locate the previous known-good archive plus its `.sha256` sidecar.
2. Re-verify integrity before restore:
   ```bash
   sha256sum -c pulsedagd-<previous-tag>-<target>.tar.gz.sha256
   ```
   (Use `.zip.sha256` on Windows artifacts.)
3. Record verification output in rollback evidence.

## Rollback procedure
1. Stop upgraded node process.
2. Restore previous known-good artifact/configuration.
3. Validate data strategy:
   - If data directory remains coherent, restart with existing data.
   - If data corruption is suspected, restore data from backup and use snapshot-assisted rebuild path.
4. Start previous version node process.
5. Run post-rollback validation checks.

## Snapshot/restore interaction
- Preferred path: preserve existing `data/` and reuse snapshot metadata.
- If coherence checks fail after restart, run:
  1. `POST /snapshot/create` on healthy state when possible.
  2. `POST /sync/rebuild` with:
     ```json
     {
       "force": true,
       "allow_partial_replay": false,
       "persist_after_rebuild": true,
       "reconcile_mempool": true
     }
     ```
- Detailed restore drill semantics and failure behavior are documented in `docs/runbooks/SNAPSHOT_RESTORE.md`.

## Post-rollback validation checks
Execute:

```bash
scripts/staging/validate_upgrade_rollback.sh post-rollback \
  --node http://127.0.0.1:8080 \
  --baseline artifacts/staging-upgrade/<run_id>/baseline/status.json \
  --out artifacts/staging-upgrade/<run_id>
```

Checks performed:
1. `/status` returns `ok=true` and `best_height >= baseline.best_height`.
2. `/sync/verify` returns `consistent=true`.
3. `/readiness` returns `ready_for_release=true`.
4. `/maintenance/report` returns `consistent=true`.
5. Runtime events are captured for rollback audit.

## Pass criteria
- Rollback validation script exits 0.
- Node is healthy and coherent by `/sync/verify` + `/readiness`.
- Chain head and persistence are stable for at least one check interval.

## Evidence to attach to v2.2 release gate
- Post-rollback JSON set.
- Validation output log.
- Snapshot/restore decision notes (whether rebuild was required).
