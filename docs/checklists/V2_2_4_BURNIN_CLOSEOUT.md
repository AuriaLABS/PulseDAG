# v2.2.4 burn-in closeout checklist

Use this checklist after the 14-day burn-in to formally close out `v2.2.4` release readiness.

## Scope guardrail (must all be true)
- [ ] No consensus changes in this closeout PR.
- [ ] Miner remains external and standalone; no miner behavior changes in this closeout PR.
- [ ] No pool logic additions in this closeout PR.
- [ ] Only release/ops artifacts and evidence links are updated.

## Required evidence package (before tag)
Run folder: `artifacts/release-evidence/<run_id>/`

- [ ] `README.md` includes run window (UTC), owner names, commit SHAs, and release artifact references.
- [ ] `CHECKLIST.md` is fully checked and signed by release + ops owners.
- [ ] `runtime-alerts/alerts.csv` covers all 14 days with Sev level, timestamp, and incident/ticket links.
- [ ] `runtime-alerts/status-rollup.jsonl` contains daily `/status`, `/runtime/status`, `/sync/status` captures.
- [ ] `snapshot-cadence/snapshot-events.csv` shows scheduled snapshot attempts + outcomes.
- [ ] `pruning-cadence/pruning-events.csv` shows pruning cadence and reclaim metrics.
- [ ] `p2p-recovery/recovery-events.csv` includes churn/rejoin timing evidence.
- [ ] `baselines/daily-baseline.md` and `baselines/rpc-consistency.csv` show p2p/sync/runtime/rpc baseline outcomes.
- [ ] `restore-rebuild/restore-timing.csv` includes restore/rebuild timing evidence with at least one repeated-run comparison.
- [ ] `mining-telemetry/daily-summary.csv` captures external miner acceptance/rejection/stale-invalid trends.
- [ ] `release-packaging/verification.md` includes node+miner standalone packaging and release E2E verification evidence.
- [ ] `restart-recovery-notes/restart-log.md` includes restart reason, startup mode (fast-boot/replay/fallback), and recovery duration.
- [ ] `dry-run/go-no-go.md` exists and includes final decision rationale for public-testnet readiness.

## Operational verification (must be demonstrated)
- [ ] Chaos restart/churn/recovery suite executed and all P0 scenarios passed in one contiguous run.
- [ ] Recovery path validated (peer churn/rejoin and restore/rebuild path where applicable).
- [ ] Snapshot creation and restore path verified with timestamps and operator notes.
- [ ] Staging upgrade + rollback rehearsals attached (`docs/runbooks/STAGING_UPGRADE.md`, `docs/runbooks/STAGING_ROLLBACK.md`).
- [ ] Read-side RPC consistency checks reference current v2.2.4 baseline methodology/output.

## Go / no-go decision flow (auditable)
1. **Evidence complete for all required categories and all 14 UTC days?** If no, **NO-GO**.
2. **Any unresolved Sev-1 tied to consensus/sync safety?** If yes, **NO-GO**.
3. **Restart/chaos/recovery/snapshot restore+rebuild checks successful and timestamped?** If no, **NO-GO**.
4. **Standalone node+miner packaging verification and release E2E checks complete?** If no, **NO-GO**.
5. **Read-side RPC consistency and p2p/sync/runtime baselines reconciled?** If no, **NO-GO**.
6. **Mining telemetry free of unresolved rejection regressions?** If no, **NO-GO**.
7. **Release + ops sign-off captured in `dry-run/go-no-go.md`?** If yes, **GO** to finalize `v2.2.4` closeout.

## Closeout sign-off
- Release owner: ____________________  Date (UTC): __________
- Ops owner: ________________________  Date (UTC): __________
- Final decision: **GO / NO-GO**
- If NO-GO, blocking issue IDs: ___________________________________________
