# v2.2.4 release evidence bundle

This document defines the standard evidence package for v2.2.4 burn-in and release readiness.

## Evidence bundle generation
Generate the base structure with:

```bash
scripts/release/generate_burnin_evidence.sh <run_id> <run_date_utc>
```

Example:

```bash
scripts/release/generate_burnin_evidence.sh v2.2.4-burnin-2026-05-01 2026-05-01
```

You can also run the legacy-named burn-in evidence workflow (`.github/workflows/v2_1-burnin-evidence.yml`).
Its naming is historical, but the generated artifact layout is valid for v2.2.x evidence collection.

## Directory layout

```text
artifacts/release-evidence/<run_id>/
  README.md
  CHECKLIST.md
  runtime-alerts/
    alerts.csv
    status-rollup.jsonl
  snapshot-cadence/snapshot-events.csv
  pruning-cadence/pruning-events.csv
  p2p-recovery/recovery-events.csv
  baselines/
    daily-baseline.md
    rpc-consistency.csv
  restore-rebuild/restore-timing.csv
  mining-telemetry/daily-summary.csv
  release-packaging/verification.md
  restart-recovery-notes/restart-log.md
  dry-run/go-no-go.md
  chaos-suite/
    manifest.csv
    events.csv
    summary.md
    scenario-outcomes.csv
    run-info.json
    raw/
  chaos-suite-<run_id>.tar.gz
  chaos-suite-<run_id>.tar.gz.sha256
```

## Required content
- `runtime-alerts/alerts.csv`: alert timeline with severity, source, and ticket references.
- `runtime-alerts/status-rollup.jsonl`: daily snapshots from `/status`, `/runtime/status`, `/sync/status` for operator console/status rollup coherence.
- `snapshot-cadence/snapshot-events.csv`: each snapshot attempt/result with duration.
- `pruning-cadence/pruning-events.csv`: each prune run/result and reclaimed bytes.
- `p2p-recovery/recovery-events.csv`: recovery timing under peer churn/rejoin.
- `baselines/daily-baseline.md`: p2p/sync/runtime/rpc baseline pass/fail checks with UTC timestamps.
- `baselines/rpc-consistency.csv`: read-side RPC ordering/pagination consistency checks and outcomes.
- `restore-rebuild/restore-timing.csv`: restore/rebuild timing captures including repeated-run comparisons.
- `mining-telemetry/daily-summary.csv`: external miner acceptance/rejection/stale-invalid trends.
- `release-packaging/verification.md`: node+miner standalone archive verification (checksum/manifest/provenance/unpack/smoke).
- `restart-recovery-notes/restart-log.md`: restart incidents, startup mode (fast-boot/replay/fallback), recovery duration, and follow-up notes.
- `dry-run/go-no-go.md`: explicit final go/no-go rationale with approver signatures.
- `chaos-suite/*`: scenario manifest, timestamped event captures, summary, and machine-readable scenario outcomes for crash/restart/churn/recovery drills.
- `chaos-suite-<run_id>.tar.gz` + `.sha256`: immutable transfer artifact for release evidence review.

## Burn-in drill evidence minimums (explicit)
The v2.2.4 package is incomplete unless the following minimums are present:
1. **Restart/chaos drills:** at least 3 restart/churn exercises distributed across burn-in window with startup mode and time-to-healthy.
2. **Peer recovery drills:** at least 2 entries with churn/rejoin timing.
3. **Snapshot restore/rebuild timing:** at least 2 timing captures, including one repeated-run comparison.
4. **Daily runtime + mining review:** one daily operator note or artifact pointer for all 14 UTC days.
5. **Packaging verification:** start-of-run and closeout evidence of standalone node+miner artifact verification.

## Validation path mapping (v2.2.4)
Evidence must explicitly map to these active validation paths:
- **Live operator console/status rollup:** `/status`, `/runtime/status`, `/sync/status` captures in `status-rollup.jsonl`.
- **Read-side RPC consistency improvements:** baseline checks reflected in `rpc-consistency.csv`.
- **Release E2E verification:** `release-packaging/verification.md` aligned with `docs/release/ARTIFACTS.md`.
- **Chaos/restart/recovery drills:** `chaos-suite/*` and `restart-log.md` aligned with `docs/runbooks/CHAOS_RESTART_RECOVERY_SUITE.md`.
- **Restore/rebuild timing evidence:** `restore-timing.csv` aligned with `docs/runbooks/SNAPSHOT_RESTORE.md` and `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
- **Snapshot productization checks:** include output/log pointers from `scripts/snapshot-productization-evidence.sh` proving export/import coherence, explicit verification signals, and repeatable restore behavior.
- **Recovery confidence evidence:** include audit outputs that capture `recovery_confidence`, `confidence_reason`, and restore-drill alignment (`restore_drill_confirms_recovery`) so operator confidence is explicit and non-misleading.
- **Deterministic release hygiene:** record lockfile + commit/tag provenance checks performed under locked builds.

## Public-testnet readiness preparation references
- Readiness criteria source: `docs/checklists/PUBLIC_TESTNET_READINESS_CRITERIA.md`
- Operator entry expectations source: `docs/checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md`
- Dry-run procedure source: `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

## Final PoW dry-run evidence (public testnet gate)
Execution and acceptance policy source:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

Dry-run evidence must show:
1. Multi-node and multi-miner topology (external miner only).
2. Restart, churn, and recovery drill timeline with UTC timestamps.
3. Explicit pass/fail outcomes against pre-declared acceptance criteria.
4. Final go/no-go decision rationale signed by release/operator owners.
5. Confirmation that no pool logic was introduced.

## v2.2.4 closeout evidence index
Use `docs/checklists/V2_2_4_BURNIN_CLOSEOUT.md` as the release-manager closeout wrapper and verify each referenced surface resolves in-repo.

## Go / no-go evidence expectations (release gate)
A `GO` decision is permitted only when all checks below are satisfied:
1. **Completeness:** all required folders/files exist and cover all 14 UTC days.
2. **Safety:** unresolved Sev-1 tied to consensus/sync safety is zero.
3. **Recovery readiness:** restart/chaos, peer recovery, and snapshot restore/rebuild timing drills pass and are timestamped.
4. **External miner health:** no unresolved regression in accepted/rejected/stale-invalid trends.
5. **Packaging assurance:** node+miner standalone release E2E verification evidence is complete.
6. **Provenance:** release artifact identity (tag/build/hash refs + lockfile policy checks) is traceable.
7. **Ownership:** release owner + ops owner sign-offs are present with UTC timestamp.

If any check is missing or failed, decision is `NO-GO` and blockers must be listed in `CHECKLIST.md` and `dry-run/go-no-go.md`.

## Closeout validation checks (release hygiene)
1. Checklist accuracy: no stale or fictional step remains in closeout documents.
2. Evidence references: every checklist reference maps to an existing repo doc/workflow/runtime surface.
3. Operator flow coherence: burn-in -> evidence collection -> chaos/recovery drills -> packaging verification -> release sign-off is traceable end-to-end.
4. Scope freeze: closeout commits include no consensus/miner/pool/product feature additions.
