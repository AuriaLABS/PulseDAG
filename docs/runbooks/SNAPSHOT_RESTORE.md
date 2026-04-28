# Snapshot Restore Drill (v2.2)

## Purpose
Validate that a node can restore state from a validated snapshot plus retained delta blocks after prune, and capture a reproducible restore-time objective (RTO) measurement.

## Safety guardrails
- This drill does **not** modify consensus rules.
- This drill does **not** modify miner behavior.
- Run on staging/testnet first.
- Keep `data/` backups before destructive actions.
- Prune safety is deterministic: the node always retains at least a 16-block rollback window even if a smaller `keep_recent_blocks` value is requested.

## Preconditions
1. Node has persisted blocks and runtime APIs available.
2. Snapshot exists (`GET /snapshot` -> `snapshot_exists: true`).
3. Snapshot height is at or above prune base (`snapshot_height >= recommended_keep_from_height`).
4. Snapshot restore anchor metadata exists (`snapshot_captured_at_unix` present in storage metadata).

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
6. Operator-grade export/import verification loop (offline confidence check):
   - Run `scripts/snapshot-productization-evidence.sh`.
   - This validates snapshot export/import bundle coherence, explicit verification signals, and restore repeatability.

## Failure handling expectations
- Corrupt snapshot with retained blocks: restore falls back to full persisted-block replay safely.
- Corrupt snapshot with no retained blocks: restore fails explicitly and does not mutate stored blocks.
- Snapshot+delta replay failure before/after prune: prune endpoint returns explicit error and does not report success.

## Measured RTO (v2.2 evidence baseline)
Measurement source: automated storage restore drill tests (`restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence` and `restore_drill_repeated_runs_produce_coherent_timing_evidence`) executed in CI-like container conditions.

| Scenario | Retained delta window | Best height | Observed restore duration |
|---|---:|---:|---:|
| Snapshot + delta restore drill | heights >= 5 | 6 | **0-1 ms** |
| Snapshot + delta restore drill (3 repeated runs) | heights >= 6 | 7 | **0-5 ms per run** |

Operational RTO target for this drill profile: **<= 5 seconds** (headroom above measured baseline for non-containerized disks and larger snapshots).

## Evidence capture
- Runtime event emitted at completion: `restore_drill_completed`
- Runtime warning event emitted on fallback: `restore_drill_snapshot_decode_failed_fallback_full` or `restore_drill_snapshot_delta_failed_fallback_full`
- Drill report includes explicit chain/tip/timing fields: `chain_id`, `best_tip_hash`, `started_at_unix`, `completed_at_unix`, `restore_duration_ms`
- Storage audit confidence surfaces are explicit and non-misleading:
  - `recovery_confidence` is `low|medium|high`.
  - `confidence_reason` explains *why* confidence is at that level.
  - `restore_drill_confirms_recovery=true` is required for `high` confidence.
  - Missing snapshot anchor metadata forces `recovery_confidence=low`.
- Repeatable command: `scripts/restore-drill-evidence.sh`
- Productized snapshot workflow command: `scripts/snapshot-productization-evidence.sh` (export/import + verification + restore coherence checks)

## Related workflows
- Recovery triage: `docs/runbooks/RECOVERY_ORCHESTRATION.md`
- Full rebuild path: `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- Maintenance gate before/after drill: `docs/runbooks/MAINTENANCE_SELF_CHECK.md`
