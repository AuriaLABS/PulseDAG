# Public testnet readiness criteria (preparation scope)

These criteria define operational readiness for a later public-testnet decision.

Scope guardrails:
- no consensus changes,
- miner stays external and standalone,
- no pool logic,
- operations/release readiness only.

## A. Explicit auditable criteria

For each item, evidence path and owner must be recorded.

1. **Topology and continuity**
   - Minimum dry-run shape completed: 5 nodes and 4 external miners for 24h.
   - Evidence: `artifacts/release-evidence/<run_id>/dry-run/topology.md` + `timeline.md`.
2. **Sync reconvergence under perturbation**
   - Restart/churn drills show `sync_lag_blocks` returns to baseline after each event.
   - Evidence: `dry-run/timeline.md`, `metrics-summary.md`, and raw captures.
3. **Recovery execution confidence**
   - Snapshot restore/rebuild drill completes and node rejoins healthy propagation.
   - Evidence: `restore-rebuild/restore-timing.csv` + dry-run recovery notes.
4. **External miner health stability**
   - Submit acceptance/rejection trend has no sustained unexplained degradation.
   - Evidence: `mining-telemetry/daily-summary.csv` + `metrics-summary.md`.
5. **Safety incident threshold**
   - Zero unresolved Sev-1 consensus/sync incidents at closeout.
   - Evidence: `runtime-alerts/alerts.csv` + `dry-run/incident-log.md`.
6. **Evidence completeness and traceability**
   - All required evidence paths in `docs/RELEASE_EVIDENCE.md` present and reviewable.
   - Evidence: `CHECKLIST.md` completeness sign-off.

## B. Staging drill set (must be practical)

1. **Bootstrap + attach drill**
   - Bring up 5 nodes, attach 4 external miners, validate tip progression.
2. **Restart drill**
   - Restart one seed and one non-seed node; measure time-to-healthy.
3. **Miner churn drill**
   - Disconnect one miner for 15 minutes; reattach and validate submit recovery.
4. **Peer isolation drill**
   - Isolate one non-seed node for 10 minutes; verify rejoin and no persistent desync.
5. **Restore/rebuild drill**
   - Run snapshot restore/rebuild on one node and confirm reconvergence.

All staging drills require UTC timestamps, owner attribution, pass/fail result, and follow-up action if failed.

## C. Decision posture

- Passing this checklist means **ready for public-testnet decision review**, not launch.
- Any failed criterion or missing evidence keeps status at **not ready / no-go**.
