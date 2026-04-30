# v2.2.6 14-day burn-in execution guide

This document defines the practical v2.2.6 pre-burn-in maturity execution and release closeout process for PulseDAG operator readiness.

## Non-negotiable guardrails
- Do **not** change consensus during this release closeout.
- Keep miner external and standalone; do **not** couple miner into node runtime.
- Do **not** add pool logic.
- Keep all changes release/ops focused; no product feature scope.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## v2.2.6 burn-in scope (pre-burn-in maturity workstreams)
The burn-in validates operator-facing maturity workstreams being closed out in v2.2.6:
- miner multithread behavior evidence collection and stability checks;
- retarget diagnostics and threshold/variance explainability;
- bounded mempool behavior under sustained ingress pressure;
- relay lane performance and backpressure propagation handling;
- pruning + snapshot guarantees across export/import/verify/restore cadence;
- sync hardening under restart/churn/isolation perturbations;
- public-testnet readiness gating and drill scoring;
- verification gates for release artifacts, checksums, and install validation.

## Public-testnet prerequisite (final PoW dry-run)
Before public testnet open and before counting day-1 of the 14-day burn-in, execute:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

## Practical burn-in matrix (v2.2.6)
Use this matrix daily. Keep entries short and evidence-linked.

| Area | Frequency | What to run/check | Evidence output | No-go trigger |
|---|---|---|---|---|
| Miner multithread behavior | Daily + D1/D7/D14 deep pass | Capture thread count, hashrate distribution, queue contention, and stale-work behavior by thread bucket | `mining-telemetry/multithread-summary.csv` + `mining-telemetry/daily-summary.csv` | Unexplained thread starvation, contention spikes, or unresolved stale-work regression |
| Retarget diagnostics | Daily | Capture target shift deltas, timestamp windows, and difficulty convergence notes | `mining-telemetry/retarget-diagnostics.csv` + `baselines/daily-baseline.md` | Retarget instability without root cause and bounded mitigation |
| Bounded mempool behavior | Daily | Validate queue bounds, eviction policy outcomes, package rejection taxonomy, and pressure release recovery | `mempool-pressure/mempool-bounds.csv` + `runtime-alerts/alerts.csv` | Sustained out-of-bounds growth or unresolved backpressure alarms |
| Relay/backpressure behavior | Daily + perturbation days | Verify relay-lane throughput, retransmit/backoff, and recovery to baseline after ingress spikes | `p2p-recovery/relay-backpressure.csv` + `p2p-recovery/recovery-events.csv` | Relay degradation persists beyond declared recovery threshold |
| Pruning + snapshot guarantees | Daily cadence + drill days | Validate prune cadence, reclaimed-bytes trend, snapshot continuity, and restore correctness | `pruning-cadence/pruning-events.csv` + `snapshot-cadence/snapshot-events.csv` + `restore-rebuild/restore-timing.csv` | Prune/snapshot incoherence or restore verification mismatch |
| Sync hardening | Daily + restart/rejoin days | Capture `/sync/status` lag evolution during churn, restart, and peer isolation | `runtime-alerts/status-rollup.jsonl` + `restart-recovery-notes/restart-log.md` | Catch-up stalls, repeated desync, or missing restart/rejoin diagnosis |
| Public-testnet gating drills | D1 + D7 + D14 | Execute required drill set and score each drill (0/1/2) with re-test notes where needed | `dry-run/go-no-go.md` + `dry-run/timeline.md` | Drill total <8/10, any drill score 0, or missing re-test for conditionals |
| Verification gates (release packaging/install) | Start + closeout | Validate release matrix v2, checksums/manifests/provenance, and standalone node+miner install flow | `release-packaging/verification.md` + `release-packaging/install-verify-log.md` | Artifact mismatch, checksum drift, or incomplete install verification |

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Record release artifact references produced by `release-binaries` workflow.
3. Run continuous network operation for **14 consecutive UTC days**.
4. Collect daily runtime/network evidence (status rollups, alert timeline, incident notes).
5. Execute planned pruning/snapshot/restore cadence and record guarantees.
6. Execute planned restart/churn/rejoin/isolation drills and record startup mode outcomes.
7. Record baseline status for miner multithread, retarget diagnostics, mempool bounds, relay/backpressure, and sync hardening with explicit pass/fail.
8. Complete release matrix v2 verification for standalone node + external miner artifacts.
9. Record final explicit go/no-go decision with release + ops sign-off.

## Release closeout gate (post day-14)
After day 14 completes, run the final closeout checklist in `docs/checklists/V2_2_6_BURNIN_CLOSEOUT.md`.

Closeout remains release-hygiene only: no consensus/miner/pool feature changes are permitted in this stage.
