# v2.2.5 burn-in closeout checklist

Use this checklist after the 14-day burn-in to formally close out `v2.2.5` release readiness.

## Scope guardrail (must all be true)
- [ ] No consensus changes in this closeout PR.
- [ ] Miner remains external and standalone; no miner behavior changes in this closeout PR.
- [ ] No pool logic additions in this closeout PR.
- [ ] Only release/ops artifacts and evidence links are updated.

## Required evidence package (before tag)
Run folder: `artifacts/release-evidence/<run_id>/`

- [ ] `README.md` includes run window (UTC), owner names, commit SHAs, and release artifact references.
- [ ] `CHECKLIST.md` is fully checked and signed by release + ops owners.
- [ ] `runtime-alerts/alerts.csv` covers all 14 days with class/severity/timestamps and incident links.
- [ ] `runtime-alerts/status-rollup.jsonl` contains daily `/status`, `/runtime/status`, `/sync/status` captures.
- [ ] `p2p-recovery/recovery-events.csv` includes peer lifecycle/topology-awareness/relay-lane recovery evidence.
- [ ] `baselines/daily-baseline.md` and `baselines/rpc-consistency.csv` record p2p/sync/mempool/query-pack outcomes.
- [ ] `mining-telemetry/daily-summary.csv` captures external miner freshness/stale-work/rejection taxonomy trends.
- [ ] `snapshot-cadence/snapshot-events.csv` and `restore-rebuild/restore-timing.csv` capture export/import/verify/restore evidence plus repeated-run timing.
- [ ] `release-packaging/verification.md` includes release matrix v2 + install verification for standalone node+miner.
- [ ] `restart-recovery-notes/restart-log.md` includes restart reason, startup mode, rejoin behavior, and recovery duration.
- [ ] `dry-run/go-no-go.md` exists and includes final decision rationale with release + ops signatures.

## Operational verification (must be demonstrated)
- [ ] Chaos restart/churn/recovery suite executed and all declared P0 scenarios passed in one contiguous run.
- [ ] Sync catch-up explainability captured for at least one restart and one rejoin event.
- [ ] Runtime alert classes, SLO rollups, trend windows, and incident snapshots are represented in evidence.
- [ ] Snapshot lineage/state-audit/recovery-confidence surfaces are included and coherent.
- [ ] Explorer/indexer activity surfaces and operator query pack checks are included.
- [ ] Hot-path measurements and readiness criteria comparisons are attached.
- [ ] Staging upgrade + rollback rehearsals attached (`docs/runbooks/STAGING_UPGRADE.md`, `docs/runbooks/STAGING_ROLLBACK.md`).

## Go / no-go decision flow (auditable)
1. **Evidence complete for all required categories and all 14 UTC days?** If no, **NO-GO**.
2. **Any unresolved Sev-1 tied to consensus/sync safety?** If yes, **NO-GO**.
3. **Peer lifecycle/relay-lane and sync catch-up evidence coherent?** If no, **NO-GO**.
4. **Restart/churn/rejoin and snapshot restore/rebuild checks successful and timestamped?** If no, **NO-GO**.
5. **External miner freshness/stale-work/rejection taxonomy free of unresolved regressions?** If no, **NO-GO**.
6. **Release matrix v2 + install verification complete for standalone node+miner?** If no, **NO-GO**.
7. **Recovery confidence + lineage/state-audit surfaces captured and non-misleading?** If no, **NO-GO**.
8. **Release + ops sign-off captured in `dry-run/go-no-go.md`?** If yes, **GO** to finalize `v2.2.5` closeout.

## Closeout sign-off
- Release owner: ____________________  Date (UTC): __________
- Ops owner: ________________________  Date (UTC): __________
- Final decision: **GO / NO-GO**
- If NO-GO, blocking issue IDs: ___________________________________________
