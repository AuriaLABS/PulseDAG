# Snapshot Restore Drill (v2.2)

## Purpose
Validate that a node can restore state from a validated snapshot plus retained delta blocks after prune, and capture a reproducible restore-time objective (RTO) measurement.

## Safety guardrails
- This drill does **not** modify consensus rules.
- This drill does **not** modify miner behavior.
- Run on staging/testnet first.
- Keep `data/` backups before destructive actions.

## Preconditions
1. Node has persisted blocks and runtime APIs available.
2. Snapshot exists (`GET /snapshot` -> `snapshot_exists: true`).
3. Snapshot height is at or above prune base (`snapshot_height >= recommended_keep_from_height`).

## Drill workflow
1. Capture baseline:
   - `GET /status`
   - `GET /snapshot`
   - `GET /sync/replay-plan`
   - `GET /sync/rebuild-preview`
2. Force a fresh snapshot:
   - `POST /snapshot/create`
3. Prune with explicit retention window:
   - `POST /prune` with JSON body like `{ "keep_recent_blocks": 64 }`
4. Run rebuild through the normal operator path:
   - `POST /sync/rebuild` with `{ "force": true, "allow_partial_replay": false, "persist_after_rebuild": true, "reconcile_mempool": true }`
5. Verify coherence:
   - `GET /sync/verify`
   - `GET /readiness`
   - `GET /status` (best height/tip stable)
   - `GET /runtime/events?limit=50` (look for restore/fallback warnings)

## Failure handling expectations
- Corrupt snapshot with retained blocks: restore falls back to full persisted-block replay safely.
- Corrupt snapshot with no retained blocks: restore fails explicitly and does not mutate stored blocks.
- Snapshot+delta replay failure before/after prune: prune endpoint returns explicit error and does not report success.

## Measured RTO (v2.2 evidence baseline)
Measurement source: automated storage restore drill test (`restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence`) executed on April 22, 2026 (UTC) in CI-like container conditions.

| Scenario | Retained delta window | Best height | Observed restore duration |
|---|---:|---:|---:|
| Snapshot + delta restore drill | heights >= 5 | 6 | **0-1 ms** |

Operational RTO target for this drill profile: **<= 5 seconds** (headroom above measured baseline for non-containerized disks and larger snapshots).

## Evidence capture
- Runtime event emitted at completion: `restore_drill_completed`
- Runtime warning event emitted on fallback: `restore_drill_snapshot_decode_failed_fallback_full` or `restore_drill_snapshot_delta_failed_fallback_full`
- Repeatable command: `scripts/restore-drill-evidence.sh`

## Related workflows
- Recovery triage: `docs/runbooks/RECOVERY_ORCHESTRATION.md`
- Full rebuild path: `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- Maintenance gate before/after drill: `docs/runbooks/MAINTENANCE_SELF_CHECK.md`
