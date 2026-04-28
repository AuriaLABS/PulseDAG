# v2.2.4 14-day burn-in execution guide

This document defines the practical v2.2.4 release burn-in process for PulseDAG operator readiness.

## Non-negotiable guardrails
- Do **not** change consensus during the 14-day run.
- Keep miner external and standalone; do **not** change miner behavior during the 14-day run.
- Do **not** add pool logic during the 14-day run.
- Keep closeout changes release/ops focused; no product feature scope.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## v2.2.4 burn-in scope (what this validates)
The burn-in must explicitly validate the v2.2.4 release machinery and operational surfaces already landed:
- live operator console/status rollup behavior (`/status`, `/runtime/status`, `/sync/status`),
- standalone node + external miner packaging and smoke verification,
- read-side RPC consistency baselines,
- release end-to-end artifact verification,
- deterministic release hygiene for `Cargo.lock`,
- p2p/sync/runtime/rpc baselines,
- chaos/restart/recovery drills,
- restore/rebuild timing evidence.

## Public-testnet prerequisite (final PoW dry-run)
Before public testnet open and before counting day-1 of the 14-day burn-in, execute:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

That dry-run is the readiness gate for:
- multi-node + multi-miner topology validation,
- restart/churn/recovery verification,
- explicit go/no-go decision evidence.

## Practical burn-in matrix (v2.2.4)
Use this matrix daily. Keep entries short and evidence-linked.

| Area | Frequency | What to run/check | Evidence output | No-go trigger |
|---|---|---|---|---|
| Operator status rollup | Daily | Capture `/status`, `/runtime/status`, `/sync/status` snapshots; verify chain/tip/peer/sync fields are coherent | `runtime-alerts/status-rollup.jsonl` + operator notes | Incoherent status rollup with no mitigation plan |
| Runtime alerts + event stream | Daily | Review alerts, sample runtime/network events, triage incidents | `runtime-alerts/alerts.csv` + linked event snippets | Unresolved Sev-1 touching consensus/sync safety |
| Snapshot cadence | Daily or scheduled cadence | Verify snapshot job start/end, duration, and result | `snapshot-cadence/snapshot-events.csv` | Missed cadence without approved exception |
| Pruning cadence | Scheduled cadence | Verify prune execution and reclaimed bytes trend | `pruning-cadence/pruning-events.csv` | Repeated prune failure with no mitigation |
| P2P/sync/runtime/RPC baseline checks | Daily smoke + D1/D7/D14 deep pass | Run baseline scripts/queries for p2p, sync lag, runtime counters, and read-side RPC ordering consistency | `baselines/daily-baseline.md` + `baselines/rpc-consistency.csv` | Baseline regression with no approved waiver |
| Restart + chaos recovery drills | At least 3x over 14 days (e.g., D3/D8/D13) | Run chaos restart/churn suite and capture startup mode (fast-boot/replay/fallback), time to healthy | `chaos-suite/*` + `restart-recovery-notes/restart-log.md` | P0 scenario failure not re-tested to pass |
| Snapshot restore/rebuild timing drill | At least 2x over 14 days | Execute restore/rebuild paths, capture `restore_duration_ms`, compare repeated runs | `restore-rebuild/restore-timing.csv` + runbook notes | Restore/rebuild path not demonstrated or timing evidence missing |
| External miner telemetry (standalone flow) | Daily | Review accepted/rejected submits and stale/invalid taxonomy from node runtime status | `mining-telemetry/daily-summary.csv` | Persistent unresolved rejection regression |
| Release packaging E2E verification | Start + closeout | Verify node+miner archives, checksums, manifests, provenance, unpack layout, smoke commands | `release-packaging/verification.md` | Artifact identity or E2E verification is incomplete |

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Record release artifact references produced by `release-binaries` workflow (or equivalent manually built release binaries).
3. Run continuous network operation for **14 consecutive days**.
4. Collect daily runtime/network evidence (status rollups, event stream, alert timeline, and operator notes).
5. Execute snapshot and pruning operations on planned cadence.
6. Execute planned chaos/restart/recovery drills and record startup mode outcomes.
7. Record p2p/sync/runtime/rpc baseline status with explicit pass/fail per checkpoint.
8. Execute restore/rebuild timing drills with at least one repeated-run comparison.
9. Record mining-flow acceptance/rejection telemetry and stale/invalid template signals.
10. Complete release packaging E2E verification evidence for standalone node + miner artifacts.

## Daily operator checklist (10-minute pass)
For each UTC day, confirm and record:
- Node health/status rollup reviewed; incidents triaged or linked to tickets.
- Snapshot/pruning jobs completed or explicitly deferred with reason.
- At least one runtime/event-stream sample captured for the day.
- Mining telemetry trend reviewed (accept/reject + stale/invalid signals).
- Baseline spot-check for p2p/sync/runtime/rpc consistency captured.
- Any restart/recovery activity logged with startup mode and elapsed recovery time.

## Pass/fail criteria for release managers
A v2.2.4 burn-in is complete only when all of the following are true:
- 14 full days completed with no unresolved Sev-1 incident tied to consensus/sync safety.
- Evidence bundle is complete for all required categories and days.
- Restart + chaos recovery + snapshot restore/rebuild checks include clear outcomes and follow-up actions.
- Snapshot/pruning cadence was run as configured and remained stable.
- Runtime status rollup and dashboard/alert data are consistent with operator incident logs.
- Baseline checks for p2p/sync/runtime/rpc are recorded and reconciled.
- Mining-flow telemetry shows no unresolved rejection pattern requiring code change.
- Standalone node+miner release packaging E2E verification evidence is complete.
- Release manager and ops owner sign-off are attached to the final evidence bundle.

## Navigation quick links
- Burn-in evidence package format: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`
- Chaos/restart/recovery suite: `docs/runbooks/CHAOS_RESTART_RECOVERY_SUITE.md`
- Snapshot restore drill: `docs/runbooks/SNAPSHOT_RESTORE.md`
- Rebuild from snapshot+delta: `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- Release artifacts verification: `docs/release/ARTIFACTS.md`
- Dashboard package: `docs/dashboard/README.md`
- Alert guide: `docs/dashboard/ALERTS.md`

## Release closeout gate (post day-14)
After day 14 completes, run the final closeout checklist in `docs/checklists/V2_2_4_BURNIN_CLOSEOUT.md` to verify:
- recovery evidence completeness,
- startup path visibility capture (fast-boot/replay/fallback),
- standalone node+miner packaging verification evidence,
- release E2E verification evidence,
- restore/rebuild timing evidence,
- mining telemetry verification coverage, and
- explicit auditable go/no-go decision record.

Closeout remains release-hygiene only: no consensus/miner/pool feature changes are permitted in this stage.
