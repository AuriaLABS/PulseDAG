# v2.2.3 release evidence bundle

This document defines the standard evidence package for v2.2.3 burn-in and release readiness.

## Evidence bundle generation
Generate the base structure with:

```bash
scripts/release/generate_burnin_evidence.sh <run_id> <run_date_utc>
```

Example:

```bash
scripts/release/generate_burnin_evidence.sh v2.2.3-burnin-2026-05-01 2026-05-01
```

You can also run the legacy-named burn-in evidence workflow (`.github/workflows/v2_1-burnin-evidence.yml`).
Its naming is historical, but the generated artifact layout is valid for v2.2.x evidence collection.

## Directory layout

```text
artifacts/release-evidence/<run_id>/
  README.md
  CHECKLIST.md
  runtime-alerts/alerts.csv
  snapshot-cadence/snapshot-events.csv
  pruning-cadence/pruning-events.csv
  p2p-recovery/recovery-events.csv
  restart-recovery-notes/restart-log.md
  dry-run/
    topology.md
    timeline.md
    metrics-summary.md
    go-no-go.md
    incident-log.md
    raw/
```

## Required content
- `runtime-alerts/alerts.csv`: alert timeline with severity, source, and ticket references.
- `snapshot-cadence/snapshot-events.csv`: each snapshot attempt/result with duration.
- `pruning-cadence/pruning-events.csv`: each prune run/result and reclaimed bytes.
- `p2p-recovery/recovery-events.csv`: recovery timing under peer churn/rejoin.
- `restart-recovery-notes/restart-log.md`: restart incidents, startup mode (fast-boot/replay/fallback), recovery duration, and follow-up notes.
- `dry-run/go-no-go.md`: signed decision log for readiness gate.

## Burn-in drill evidence minimums (explicit)
The v2.2.3 package is incomplete unless the following minimums are present:
1. **Restart drills:** at least 3 entries distributed across burn-in window with startup mode and time-to-healthy.
2. **Peer recovery drills:** at least 2 entries with churn/rejoin timing.
3. **Snapshot restore/rebuild validation:** at least 1 successful exercise with operator notes.
4. **Daily runtime + mining review:** one daily operator note or artifact pointer for all 14 UTC days.

## Final PoW dry-run evidence (public testnet gate)
Execution and acceptance policy source:
- `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`

Dry-run evidence must show:
1. Multi-node and multi-miner topology (external miner only).
2. Restart, churn, and recovery drill timeline with UTC timestamps.
3. Explicit pass/fail outcomes against pre-declared acceptance criteria.
4. Final go/no-go decision rationale signed by release/operator owners.
5. Confirmation that no pool logic was introduced.

## Additional v2.2.3 operator evidence (attach to bundle)
These artifacts should be linked from `README.md` in the run directory:

1. **Runtime/network event stream extracts**
   - Source: `docs/RUNTIME_EVENT_STREAM.md`
2. **Dashboard and alert captures**
   - Source: `docs/dashboard/README.md`, `docs/dashboard/ALERTS.md`
3. **Config profile declaration for the run**
   - Source: `README.md` (`PULSEDAG_CONFIG_PROFILE`)
4. **Runbook execution notes**
   - Source: `docs/runbooks/INDEX.md`
5. **Release binary provenance**
   - Source: `.github/workflows/release-binaries.yml`
6. **Mining telemetry summary**
   - Source: `docs/dashboard/README.md` (`GET /runtime/status` mining telemetry fields)

## v2.2.3 closeout evidence index
Use `docs/checklists/V2_2_3_BURNIN_CLOSEOUT.md` as the release-manager closeout wrapper and verify each referenced surface resolves in-repo.

## Go / no-go evidence expectations (release gate)
A `GO` decision is permitted only when all checks below are satisfied:
1. **Completeness:** all required folders/files exist and cover all 14 UTC days.
2. **Safety:** unresolved Sev-1 tied to consensus/sync safety is zero.
3. **Recovery readiness:** restart, peer recovery, and snapshot restore/rebuild drills pass and are timestamped.
4. **External miner health:** no unresolved regression in accepted/rejected/stale-invalid trends.
5. **Provenance:** release artifact identity (tag/build/hash refs) is traceable.
6. **Ownership:** release owner + ops owner sign-offs are present with UTC timestamp.

If any check is missing or failed, decision is `NO-GO` and blockers must be listed in `CHECKLIST.md` and `dry-run/go-no-go.md`.

## Closeout validation checks (release hygiene)
1. Checklist accuracy: no stale or fictional step remains in closeout documents.
2. Evidence references: every checklist reference maps to an existing repo doc/workflow/runtime surface.
3. Operator flow coherence: burn-in -> evidence collection -> recovery drills -> upgrade/rollback rehearsal -> release sign-off is traceable end-to-end.
4. Scope freeze: closeout commits include no consensus/miner/pool/product feature additions.
