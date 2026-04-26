# v2.2.3 burn-in closeout checklist

Use this checklist after the 14-day burn-in to formally close out `v2.2.3` release readiness.

## Scope guardrail (must all be true)
- [ ] No consensus changes in this closeout PR.
- [ ] No miner behavior changes in this closeout PR.
- [ ] No pool logic additions in this closeout PR.
- [ ] Only release/ops artifacts and evidence links are updated.

## Required evidence package (before tag)
Run folder: `artifacts/release-evidence/<run_id>/`

- [ ] `README.md` includes run window (UTC), owner names, commit SHAs, and release artifact references.
- [ ] `CHECKLIST.md` is fully checked and signed by release + ops owners.
- [ ] `runtime-alerts/alerts.csv` covers all 14 days with Sev level, timestamp, and incident/ticket links.
- [ ] `snapshot-cadence/snapshot-events.csv` shows scheduled snapshot attempts + outcomes.
- [ ] `pruning-cadence/pruning-events.csv` shows pruning cadence and reclaim metrics.
- [ ] `p2p-recovery/recovery-events.csv` includes churn/rejoin timing evidence.
- [ ] `restart-recovery-notes/restart-log.md` includes restart reason, startup mode (fast-boot/replay/fallback), and recovery duration.
- [ ] `dry-run/go-no-go.md` exists and includes final decision rationale for public-testnet readiness.

## Operational verification (must be demonstrated)
- [ ] Restart drill completed at least once during burn-in with startup-mode evidence.
- [ ] Recovery path validated (peer churn/rejoin and restore/rebuild path where applicable).
- [ ] Snapshot creation and restore path verified with timestamps and operator notes.
- [ ] Staging upgrade + rollback rehearsals attached (`docs/runbooks/STAGING_UPGRADE.md`, `docs/runbooks/STAGING_ROLLBACK.md`).

## Go / no-go decision flow
1. **Evidence complete?** If no, **NO-GO**.
2. **Any unresolved Sev-1 tied to consensus/sync safety?** If yes, **NO-GO**.
3. **Restart/recovery/snapshot checks successful and documented?** If no, **NO-GO**.
4. **Mining telemetry free of unresolved rejection regressions?** If no, **NO-GO**.
5. **Release + ops sign-off captured?** If yes, **GO** to finalize `v2.2.3` closeout.

## Closeout sign-off
- Release owner: ____________________  Date (UTC): __________
- Ops owner: ________________________  Date (UTC): __________
- Final decision: **GO / NO-GO**
- If NO-GO, blocking issue IDs: ___________________________________________
