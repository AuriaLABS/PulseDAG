# v2.2.5 release evidence bundle

This document defines the standard evidence package for v2.2.5 burn-in and release readiness.

## Evidence bundle generation
Generate the base structure with:

```bash
scripts/release/generate_burnin_evidence.sh <run_id> <run_date_utc>
```

## Required content
Run folder: `artifacts/release-evidence/<run_id>/`

- `runtime-alerts/alerts.csv`: alert timeline with class/severity/source/ticket references.
- `runtime-alerts/status-rollup.jsonl`: daily captures from `/status`, `/runtime/status`, `/sync/status`.
- `snapshot-cadence/snapshot-events.csv`: snapshot/export/import/verification cadence and outcomes.
- `pruning-cadence/pruning-events.csv`: prune cadence and reclaimed bytes trend.
- `p2p-recovery/recovery-events.csv`: peer lifecycle/rejoin/relay-lane recovery timing evidence.
- `baselines/daily-baseline.md`: daily pass/fail notes for p2p/sync/mempool/mining/query-pack surfaces.
- `baselines/rpc-consistency.csv`: read-side consistency checks and outcomes.
- `restore-rebuild/restore-timing.csv`: restore/rebuild timing, repeated-run comparisons, lineage references.
- `mining-telemetry/daily-summary.csv`: external miner freshness/reject taxonomy/stale-invalid trends.
- `release-packaging/verification.md`: node+miner release matrix v2 and install verification evidence.
- `restart-recovery-notes/restart-log.md`: restart cause, startup mode, recovery duration, and rejoin outcome.
- `dry-run/go-no-go.md`: explicit auditable final go/no-go rationale with sign-offs.
- `chaos-suite/*`: scenario manifest, event timeline, and machine-readable outcomes.

## Validation path mapping (v2.2.5)
Evidence must explicitly map to active validation paths:
- Peer lifecycle / topology-awareness / relay lanes.
- Sync catch-up explainability + restart/rejoin hardening.
- Package-aware mempool pressure/backpressure signals.
- External miner freshness, stale-work controls, and rejection taxonomy.
- Runtime alert classes, SLO-style rollups, dashboard trend windows, incident snapshots.
- Snapshot export/import/verification/restore productization.
- Pruning + snapshot integration evidence path (coherence + repeatability across rebuild/restore workflows).
- Snapshot lineage/state audit/recovery confidence surfaces.
- Explorer/indexer activity surfaces and operator query pack.
- Release matrix v2 and install verification (`docs/release/ARTIFACTS.md`).
- Public-testnet readiness docs and hot-path measurements.

## Evidence minimums (explicit)
1. At least 14 UTC days of runtime, alert, and status-rollup evidence.
2. At least 3 restart/churn/rejoin drills across the run window.
3. At least 2 snapshot restore/rebuild timing captures including repeated-run comparison.
4. Daily external miner telemetry entries with rejection taxonomy.
5. Start-of-run and closeout release matrix/install verification for standalone node + external miner.
6. Final `GO`/`NO-GO` record with release + ops owner sign-offs.

## Go / no-go evidence expectations
A `GO` decision is allowed only if all are true:
1. Evidence completeness for all required categories and all 14 UTC days.
2. No unresolved Sev-1 tied to consensus/sync safety.
3. Recovery readiness proven: restart/churn/rejoin + snapshot restore/rebuild checks passed.
4. External miner health: no unresolved rejection/stale-work regression.
5. Packaging assurance: standalone node+miner release matrix/install verification complete.
6. Recovery confidence and lineage/state-audit surfaces captured and non-misleading.
7. Release owner + ops owner sign-offs present with UTC timestamp.

If any check is missing/failed, decision is `NO-GO`; blockers must be listed in `CHECKLIST.md` and `dry-run/go-no-go.md`.

## v2.2.5 closeout evidence index
Use `docs/checklists/V2_2_5_BURNIN_CLOSEOUT.md` as the release-manager closeout wrapper.

## Runtime remediation/no-go surfaces
Operator evidence should include `remediation_summary`, `no_go_escalation`, and `no_go_reasons` from `/runtime/status` or `/operator/query-pack` as bounded escalation artifacts linked to the incident timeline.
