# v2.2.5 14-day burn-in execution guide

This document defines the practical v2.2.5 release burn-in process for PulseDAG operator readiness.

## Non-negotiable guardrails
- Do **not** change consensus during the 14-day run.
- Keep miner external and standalone; do **not** change miner behavior during the 14-day run.
- Do **not** add pool logic during the 14-day run.
- Keep closeout changes release/ops focused; no product feature scope.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## v2.2.5 burn-in scope (what this validates)
The burn-in validates operator-facing maturity work already landed in v2.2.5:
- peer lifecycle, topology-awareness, and relay lanes;
- sync catch-up explainability and restart/rejoin hardening;
- package-aware mempool behavior and pressure/backpressure visibility;
- external mining freshness, stale-work controls, rejection taxonomy, and miner operator flow;
- runtime alert classes, SLO-style rollups, dashboard trend windows, and incident snapshots;
- snapshot export/import/verification/restore productization;
- snapshot lineage, state audit, and recovery confidence surfaces;
- explorer/indexer activity surfaces and operator query pack;
- release matrix v2 and install verification;
- public-testnet readiness docs and hot-path measurements.

## Public-testnet prerequisite (final PoW dry-run)
Before public testnet open and before counting day-1 of the 14-day burn-in, execute:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

## Practical burn-in matrix (v2.2.5)
Use this matrix daily. Keep entries short and evidence-linked.

| Area | Frequency | What to run/check | Evidence output | No-go trigger |
|---|---|---|---|---|
| Peer lifecycle + relay lanes | Daily | Confirm peer join/leave/rejoin coherence and relay-lane activity using operator query pack + runtime counters | `p2p-recovery/recovery-events.csv` + `baselines/daily-baseline.md` | Repeated unexplained peer instability or relay-lane degradation |
| Sync catch-up explainability | Daily + restart days | Capture `/sync/status` lag evolution and restart/rejoin catch-up path notes | `runtime-alerts/status-rollup.jsonl` + `restart-recovery-notes/restart-log.md` | Catch-up stalls with no root cause or mitigation plan |
| Mempool package pressure/backpressure | Daily | Review package-aware queue pressure/backpressure signals and rejection trends | `baselines/daily-baseline.md` + `runtime-alerts/alerts.csv` | Sustained pressure alarms without operator response path |
| External miner freshness + rejection taxonomy | Daily | Review template freshness, stale-work controls, and reject categories (external miner only) | `mining-telemetry/daily-summary.csv` | Unresolved rise in stale/invalid/reject classes |
| Runtime alerts + SLO rollups | Daily | Validate alert class routing, SLO rollups, dashboard trend windows, incident snapshot logging | `runtime-alerts/alerts.csv` + `runtime-alerts/status-rollup.jsonl` | Sev-1 unresolved incident tied to consensus/sync safety |
| Snapshot productization + lineage/audit | Daily cadence + drill days | Validate export/import/verification/restore success, lineage markers, state-audit references | `snapshot-cadence/snapshot-events.csv` + `restore-rebuild/restore-timing.csv` | Restore/verification path missing or inconsistent lineage evidence |
| Recovery confidence drills | At least 3x over 14 days | Run restart/churn/recovery drills and confirm confidence surfaces remain explicit/non-misleading | `chaos-suite/*` + `restart-recovery-notes/restart-log.md` | P0 drill failure not re-tested to pass |
| Explorer/indexer activity + query pack | Daily smoke + D1/D7/D14 deep pass | Execute operator query pack and validate explorer/indexer freshness and expected activity surfaces | `baselines/daily-baseline.md` | Data freshness gaps with no triage path |
| Release matrix v2 + install verification | Start + closeout | Validate node+miner archives, checksums, manifests, install verification, and smoke flow | `release-packaging/verification.md` | Artifact identity or install verification incomplete |
| Hot-path measurements/readiness | D1 + D7 + D14 | Capture hot-path measurements and compare against declared readiness thresholds | `baselines/daily-baseline.md` + benchmark links | Regressed hot-path metrics without accepted waiver |

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Record release artifact references produced by `release-binaries` workflow.
3. Run continuous network operation for **14 consecutive days**.
4. Collect daily runtime/network evidence (status rollups, alert timeline, incident notes).
5. Execute planned snapshot, verification, and pruning cadence.
6. Execute planned restart/churn/rejoin drills and record startup mode outcomes.
7. Record p2p/sync/mempool/mining/query-pack baseline status with explicit pass/fail.
8. Execute restore/rebuild timing drills with repeated-run comparison and lineage/audit references.
9. Complete release matrix v2 verification for standalone node + external miner artifacts.
10. Record final explicit go/no-go decision with release + ops sign-off.

## Pass/fail criteria for release managers
A v2.2.5 burn-in is complete only when all of the following are true:
- 14 full days completed with no unresolved Sev-1 incident tied to consensus/sync safety.
- Evidence bundle is complete for all required categories and days.
- Peer lifecycle/relay-lane, sync catch-up, and mempool pressure signals are all reconciled.
- External miner freshness/rejection telemetry shows no unresolved regression.
- Snapshot export/import/verify/restore evidence includes lineage and recovery confidence surfaces.
- Explorer/indexer query-pack checks and hot-path measurements are recorded and within readiness criteria.
- Standalone node+miner release matrix v2 and install verification evidence is complete.
- Release manager and ops owner sign-off are attached to the final evidence bundle.

## Navigation quick links
- Burn-in evidence package format: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`
- Closeout checklist: `docs/checklists/V2_2_5_BURNIN_CLOSEOUT.md`
- Release artifacts verification: `docs/release/ARTIFACTS.md`

## Release closeout gate (post day-14)
After day 14 completes, run the final closeout checklist in `docs/checklists/V2_2_5_BURNIN_CLOSEOUT.md`.

Closeout remains release-hygiene only: no consensus/miner/pool feature changes are permitted in this stage.
