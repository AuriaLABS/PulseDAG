# Snapshot Restore Drill (v2.3.0 private testnet)

## Purpose

Validate that a v2.3.0 candidate node can restore state from a validated snapshot plus retained delta blocks after prune, and capture reproducible restore-time objective (RTO), consistency, readiness, and fallback evidence.

## Candidate and safety guardrails

- Run the drill against the exact private-testnet release candidate SHA being evaluated.
- Record `VERSION=v2.3.0`, Cargo workspace version `2.3.0`, the candidate SHA, node release metadata, UTC start/end times, and evidence paths.
- This drill does **not** modify consensus rules.
- This drill does **not** modify miner behavior.
- Run on a disposable or recoverable private-testnet node before release closeout.
- Keep `data/` backups before destructive actions.
- Prune safety is deterministic: the node always retains at least a 16-block rollback window even if a smaller `keep_recent_blocks` value is requested.
- A successful private-testnet restore drill does not authorize public-testnet launch or start the 30-day clock.

## Preconditions

1. Node has persisted blocks and runtime APIs available at the configured loopback RPC address; the canonical v2.3.0 private-testnet examples use `http://127.0.0.1:8280`.
2. Snapshot exists (`GET /snapshot` -> `snapshot_exists: true`).
3. Snapshot height is at or above prune base (`snapshot_height >= recommended_keep_from_height`).
4. Snapshot restore anchor metadata exists (`snapshot_captured_at_unix` present in storage metadata).
5. Baseline `/health`, `/readiness`, `/status`, `/sync/verify`, and `/p2p/status` evidence has been captured.

## Drill workflow

1. Capture baseline:
   - `GET /health`
   - `GET /readiness`
   - `GET /status`
   - `GET /snapshot`
   - `GET /sync/verify`
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
   - `GET /maintenance/report`
   - `GET /runtime/events?limit=50` (look for restore/fallback warnings)
6. Operator-grade export/import verification loop:
   - Run `scripts/snapshot-productization-evidence.sh`.
   - This validates snapshot export/import bundle coherence, explicit verification signals, and restore repeatability.
7. Preserve the complete output bundle with checksums and bind it to the exact candidate SHA.

## Failure handling expectations

- Corrupt snapshot with retained blocks: restore falls back to full persisted-block replay safely.
- Corrupt snapshot with no retained blocks: restore fails explicitly and does not mutate stored blocks.
- Snapshot+delta replay failure before/after prune: prune endpoint returns explicit error and does not report success.
- Any failed coherence, readiness, replay-gap, timing, or evidence-integrity check is a private-testnet closeout blocker until resolved or explicitly waived under the release policy.

## Historical measured RTO baseline

The retained v2.2 automated baseline is historical reference data, not substitute evidence for the v2.3.0 final candidate. Its measurement source was the automated storage restore drill tests `restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence` and `restore_drill_repeated_runs_produce_coherent_timing_evidence` in CI-like container conditions.

| Scenario | Retained delta window | Best height | Historical observed restore duration |
|---|---:|---:|---:|
| Snapshot + delta restore drill | heights >= 5 | 6 | **0-1 ms** |
| Snapshot + delta restore drill (3 repeated runs) | heights >= 6 | 7 | **0-5 ms per run** |

Operational RTO target for this small drill profile remains **<= 5 seconds**. The v2.3.0 closeout record must include a fresh candidate-scoped measurement; larger snapshots or slower disks require an explicitly documented target and result.

## Evidence capture

- Exact candidate SHA, `VERSION`, Cargo version, release endpoint output, chain ID, node identity, and UTC timestamps.
- Pre/post `/health`, `/readiness`, `/status`, `/snapshot`, `/sync/verify`, replay-plan, rebuild-preview, and maintenance-report captures.
- Runtime event emitted at completion: `restore_drill_completed`.
- Runtime warning event emitted on fallback: `restore_drill_snapshot_decode_failed_fallback_full` or `restore_drill_snapshot_delta_failed_fallback_full`.
- Drill report fields: `chain_id`, `best_tip_hash`, `started_at_unix`, `completed_at_unix`, `restore_duration_ms`.
- Storage audit confidence surfaces:
  - `recovery_confidence` is `low|medium|high`.
  - `confidence_reason` explains why confidence is at that level.
  - `restore_drill_confirms_recovery=true` is required for `high` confidence.
  - Missing snapshot anchor metadata forces `recovery_confidence=low`.
- Repeatable command: `scripts/restore-drill-evidence.sh`.
- Productized snapshot command: `scripts/snapshot-productization-evidence.sh`.
- Pruning/snapshot integration command: `scripts/pruning-snapshot-integration-evidence.sh`.
- SHA-256 manifest covering every retained evidence file.

## Related workflows

- Guarded execution: `docs/runbooks/SNAPSHOT_PRUNE_RESTORE_DRILL.md`.
- Recovery triage: `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
- Full rebuild path: `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
- Maintenance gate before/after drill: `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.
- Release closeout: `docs/checklists/V2_3_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md`.
