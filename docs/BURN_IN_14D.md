# v2.2.2 14-day burn-in execution guide

This document defines the practical v2.2.2 release burn-in process for PulseDAG operator readiness.

## Non-negotiable guardrails
- Do **not** change consensus during the 14-day run.
- Do **not** change miner behavior during the 14-day run.
- Do **not** add pool logic during the 14-day run.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## Public-testnet prerequisite (final PoW dry-run)
Before public testnet open and before counting day-1 of the 14-day burn-in, execute:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

That dry-run is the readiness gate for:
- multi-node + multi-miner topology validation,
- restart/churn/recovery verification,
- explicit go/no-go decision evidence.

## What CI covers vs what release burn-in covers
- `Soak Smoke (short CI signal)` workflow: short regression signal for obvious breakage.
- 14-day release burn-in: continuously operated testnet/staging run with active operator monitoring, incident handling, and formal evidence collection.

## Operator surfaces that must be exercised during burn-in
1. **Runtime + network event streaming**
   - `docs/RUNTIME_EVENT_STREAM.md`
2. **Dashboards + alerts**
   - `docs/dashboard/README.md`
   - `docs/dashboard/ALERTS.md`
3. **Config profiles and startup posture**
   - `README.md` (`PULSEDAG_CONFIG_PROFILE` guidance)
4. **Operational runbooks**
   - `docs/runbooks/INDEX.md`
5. **Fast-boot/replay visibility and fallback interpretation**
   - `docs/runbooks/FAST_BOOT_AND_FALLBACK.md`
6. **Mining telemetry health (external miner flow)**
   - `docs/dashboard/README.md` (runtime telemetry fields under `GET /runtime/status`)
7. **Release binaries workflow awareness**
   - `.github/workflows/release-binaries.yml`

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Record release artifact references produced by `release-binaries` workflow (or equivalent manually built release binaries).
3. Run continuous network operation for **14 consecutive days**.
4. Collect daily runtime/network evidence (event stream, alert timeline, dashboard snapshots, and operator notes).
5. Execute snapshot and pruning operations on planned cadence.
6. Execute planned restart/recovery drills and record fast-boot vs replay/fallback outcomes.
7. Record p2p recovery timing during churn/rejoin events.
8. Record mining-flow acceptance/rejection telemetry and stale/invalid template signals.

## Required evidence categories (daily coverage)
Every day of the 14-day run must be represented in evidence under these categories:
1. Runtime alerts and event-stream incidents
2. Snapshot cadence
3. Pruning cadence
4. P2P recovery stats
5. Restart/recovery notes (including startup mode visibility)
6. Mining telemetry summary

Use `docs/RELEASE_EVIDENCE.md` for the artifact structure and release acceptance checklist.


## Daily operator checklist (10-minute pass)
For each UTC day, confirm and record:
- Node health and alerts reviewed; incidents triaged or linked to tickets.
- Snapshot/pruning jobs completed or explicitly deferred with reason.
- At least one runtime/event-stream sample captured for the day.
- Mining telemetry trend reviewed (accept/reject + stale/invalid signals).
- Any restart/recovery activity logged with startup mode and elapsed recovery time.

## Pass/fail criteria for release managers
A v2.2.2 burn-in is complete only when all of the following are true:
- 14 full days completed with no unresolved Sev-1 incident tied to consensus/sync safety.
- Evidence bundle is complete for all required categories and days.
- Restart + recovery notes include clear outcomes and follow-up actions.
- Snapshot/pruning cadence was run as configured and remained stable.
- Runtime event stream and dashboard/alert data are consistent with operator incident logs.
- Mining-flow telemetry shows no unresolved rejection pattern requiring code change.
- Release manager sign-off is attached to the final evidence bundle.

## Navigation quick links
- Burn-in evidence package format: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`
- Dashboard package: `docs/dashboard/README.md`
- Alert guide: `docs/dashboard/ALERTS.md`

## Release closeout gate (post day-14)
After day 14 completes, run the final closeout checklist in `docs/checklists/V2_2_2_BURNIN_CLOSEOUT.md` to verify:
- recovery evidence completeness,
- startup path visibility capture (fast-boot/replay/fallback),
- mining telemetry verification coverage,
- upgrade/rollback rehearsal evidence, and
- release binaries provenance references.

Closeout remains release-hygiene only: no consensus/miner/pool feature changes are permitted in this stage.
