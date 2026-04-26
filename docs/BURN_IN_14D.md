# v2.2.3 14-day burn-in execution guide

This document defines the practical v2.2.3 release burn-in process for PulseDAG operator readiness.

## Non-negotiable guardrails
- Do **not** change consensus during the 14-day run.
- Keep miner external and standalone; do **not** change miner behavior during the 14-day run.
- Do **not** add pool logic during the 14-day run.
- Keep closeout changes release/ops focused; no product feature scope.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## Public-testnet prerequisite (final PoW dry-run)
Before public testnet open and before counting day-1 of the 14-day burn-in, execute:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

That dry-run is the readiness gate for:
- multi-node + multi-miner topology validation,
- restart/churn/recovery verification,
- explicit go/no-go decision evidence.

## Practical burn-in matrix (v2.2.3)
Use this matrix daily. Keep entries short and evidence-linked.

| Area | Frequency | What to run/check | Evidence output | No-go trigger |
|---|---|---|---|---|
| Runtime alerts + event stream | Daily | Review alerts, sample runtime/network events, triage incidents | `runtime-alerts/alerts.csv` + linked event snippets | Unresolved Sev-1 touching consensus/sync safety |
| Snapshot cadence | Daily or scheduled cadence | Verify snapshot job start/end, duration, and result | `snapshot-cadence/snapshot-events.csv` | Missed cadence without approved exception |
| Pruning cadence | Scheduled cadence | Verify prune execution and reclaimed bytes trend | `pruning-cadence/pruning-events.csv` | Repeated prune failure with no mitigation |
| Restart drill | At least 3x over 14 days (e.g., D3/D8/D13) | Controlled restart, capture startup mode (fast-boot/replay/fallback), time to healthy | `restart-recovery-notes/restart-log.md` | Restart fails to recover or mode is unexplained |
| Recovery drill (peer churn/rejoin) | At least 2x over 14 days | Induce peer loss/rejoin and measure recovery timing | `p2p-recovery/recovery-events.csv` | Recovery exceeds agreed SLO with no plan |
| Snapshot restore/rebuild validation | At least 1x over 14 days | Execute restore or rebuild path and validate health | `restart-recovery-notes/restart-log.md` + operator note | Restore/rebuild path not demonstrated |
| Mining telemetry (external miner flow) | Daily | Review accepted/rejected submits and stale/invalid taxonomy | run `README.md`-linked telemetry notes in run `README.md` | Persistent unresolved rejection regression |
| Release artifacts provenance | Start + closeout | Record tag/build refs and hashes for release binaries | run `README.md` + `CHECKLIST.md` | Artifact identity not traceable |

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Record release artifact references produced by `release-binaries` workflow (or equivalent manually built release binaries).
3. Run continuous network operation for **14 consecutive days**.
4. Collect daily runtime/network evidence (event stream, alert timeline, dashboard snapshots, and operator notes).
5. Execute snapshot and pruning operations on planned cadence.
6. Execute planned restart/recovery drills and record fast-boot vs replay/fallback outcomes.
7. Record p2p recovery timing during churn/rejoin events.
8. Record mining-flow acceptance/rejection telemetry and stale/invalid template signals.

## Daily operator checklist (10-minute pass)
For each UTC day, confirm and record:
- Node health and alerts reviewed; incidents triaged or linked to tickets.
- Snapshot/pruning jobs completed or explicitly deferred with reason.
- At least one runtime/event-stream sample captured for the day.
- Mining telemetry trend reviewed (accept/reject + stale/invalid signals).
- Any restart/recovery activity logged with startup mode and elapsed recovery time.

## Pass/fail criteria for release managers
A v2.2.3 burn-in is complete only when all of the following are true:
- 14 full days completed with no unresolved Sev-1 incident tied to consensus/sync safety.
- Evidence bundle is complete for all required categories and days.
- Restart + recovery + snapshot restore/rebuild checks include clear outcomes and follow-up actions.
- Snapshot/pruning cadence was run as configured and remained stable.
- Runtime event stream and dashboard/alert data are consistent with operator incident logs.
- Mining-flow telemetry shows no unresolved rejection pattern requiring code change.
- Release manager and ops owner sign-off are attached to the final evidence bundle.

## Navigation quick links
- Burn-in evidence package format: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`
- Dashboard package: `docs/dashboard/README.md`
- Alert guide: `docs/dashboard/ALERTS.md`

## Release closeout gate (post day-14)
After day 14 completes, run the final closeout checklist in `docs/checklists/V2_2_3_BURNIN_CLOSEOUT.md` to verify:
- recovery evidence completeness,
- startup path visibility capture (fast-boot/replay/fallback),
- mining telemetry verification coverage,
- upgrade/rollback rehearsal evidence, and
- release binaries provenance references.

Closeout remains release-hygiene only: no consensus/miner/pool feature changes are permitted in this stage.
