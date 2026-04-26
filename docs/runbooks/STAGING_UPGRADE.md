# Staging Upgrade Validation Runbook (v2.2)

## Purpose
Provide a **repeatable** staging upgrade procedure for v2.2 that verifies node health and chain/runtime coherence without changing consensus or miner behavior.

## Scope and guardrails
- Operational procedure only (release readiness validation).
- No consensus rule changes.
- No miner changes.
- Intended for staging prior to production cutover.

## Preconditions
1. Maintenance window announced.
2. Node is reachable (`/health`, `/status`).
3. Snapshot is present or freshly created (`/snapshot`, `/snapshot/create`).
4. Baseline evidence directory exists (example: `artifacts/staging-upgrade/<date>/`).

## Baseline capture (before upgrade)
Capture these API responses to files:
- `GET /release`
- `GET /status`
- `GET /snapshot`
- `GET /maintenance/report`
- `GET /sync/verify`
- `GET /readiness`
- `GET /runtime/events?limit=200`

Recommended helper:
```bash
scripts/staging/validate_upgrade_rollback.sh baseline \
  --node http://127.0.0.1:8080 \
  --out artifacts/staging-upgrade/$(date -u +%Y%m%dT%H%M%SZ)
```

## Upgrade package verification (before stop/deploy)
1. Download the target release archive and matching `.sha256` sidecar.
2. Verify archive integrity:
   ```bash
   sha256sum -c pulsedagd-<tag>-<target>.tar.gz.sha256
   ```
   (Use `.zip.sha256` on Windows artifacts.)
3. Record archive filename + checksum in the staging evidence notes.

## Upgrade procedure
1. **Freeze writes** in staging traffic tooling (if used) to reduce noisy mempool churn during package swap.
2. Stop the node process cleanly.
3. Deploy v2.2 node artifact/configuration for staging.
4. Start node process.
5. Wait for RPC health to return.
6. Run post-upgrade validation checks.

## Post-upgrade validation checks
Execute:

```bash
scripts/staging/validate_upgrade_rollback.sh post-upgrade \
  --node http://127.0.0.1:8080 \
  --baseline artifacts/staging-upgrade/<run_id>/baseline/status.json \
  --out artifacts/staging-upgrade/<run_id>
```

Checks performed:
1. `/release` returns `ok=true` and includes version/stage metadata.
2. `/status` returns `ok=true`, `best_height >= baseline.best_height`, and non-empty `chain_id`.
3. `/sync/verify` returns `consistent=true`.
4. `/readiness` returns `ready_for_release=true` (or explicitly documented warnings only).
5. `/maintenance/report` returns `consistent=true` and a non-regressive `recommended_keep_from_height` relationship.
6. `/runtime/events` is captured for operator review.

## Pass criteria
- Upgrade validation script exits 0.
- No coherence blockers in `/sync/verify` or `/readiness`.
- Height progression is monotonic versus baseline.

## Failure criteria and response
- If any validation check fails, execute rollback runbook immediately: `docs/runbooks/STAGING_ROLLBACK.md`.
- Preserve evidence directory for root-cause analysis.

## Evidence to attach to v2.2 release gate
- Baseline JSON set.
- Post-upgrade JSON set.
- Validation script output log.
- Operator notes (timing, anomalies, corrective actions).
