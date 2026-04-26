# v2.2.3 burn-in closeout checklist

Use this checklist after the 14-day burn-in to decide whether to tag `v2.2.3`.

## Scope guardrail (must all be true)
- [ ] No consensus changes in this closeout PR.
- [ ] Miner remains external and standalone; no miner behavior changes in this closeout PR.
- [ ] No pool logic additions in this closeout PR.
- [ ] Only release/ops artifacts and evidence links are updated.

## Required evidence package (before tag)
Run folder: `artifacts/release-evidence/<run_id>/`

- [ ] `README.md` includes run window (UTC), owner names, commit SHAs, and release artifact references.
- [ ] `CHECKLIST.md` is fully checked and signed by release + ops owners.
- [ ] `runtime-alerts/alerts.csv` covers all 14 days with severity, timestamp, and incident/ticket links.
- [ ] `snapshot-cadence/snapshot-events.csv` shows scheduled snapshot attempts + outcomes.
- [ ] `pruning-cadence/pruning-events.csv` shows pruning cadence and reclaim metrics.
- [ ] `p2p-recovery/recovery-events.csv` includes churn/rejoin timing evidence.
- [ ] `restart-recovery-notes/restart-log.md` includes restart reason, startup mode (fast-boot/replay/fallback), and recovery duration.
- [ ] `dry-run/go-no-go.md` exists and includes final decision rationale for public-testnet readiness.

## Drill minimums (must be demonstrated)
- [ ] Restart drill completed at least 3 times across burn-in (e.g., D3/D8/D13) with startup-mode evidence.
- [ ] Recovery drill (peer churn/rejoin) completed at least 2 times with measured timings.
- [ ] Snapshot restore/rebuild path validated at least 1 time with timestamps and operator notes.
- [ ] Staging upgrade + rollback rehearsals attached (`docs/runbooks/STAGING_UPGRADE.md`, `docs/runbooks/STAGING_ROLLBACK.md`).

## Go / no-go decision flow
1. **Evidence complete?** If no, **NO-GO**.
2. **Any unresolved Sev-1 tied to consensus/sync safety?** If yes, **NO-GO**.
3. **Restart/recovery/snapshot checks successful and documented?** If no, **NO-GO**.
4. **Mining telemetry free of unresolved rejection regressions?** If no, **NO-GO**.
5. **Release + ops sign-off captured?** If yes, **GO** to tag `v2.2.3`.

## Closeout sign-off
- Release owner: ____________________  Date (UTC): __________
- Ops owner: ________________________  Date (UTC): __________
- Final decision: **GO / NO-GO**
- If NO-GO, blocking issue IDs: ___________________________________________
