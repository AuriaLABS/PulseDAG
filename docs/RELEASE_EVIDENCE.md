# v2.2 release evidence bundle

This document defines the standard evidence package for v2.2 burn-in and release readiness.

## Evidence bundle generation
Generate the base structure with:

```bash
scripts/release/generate_burnin_evidence.sh <run_id> <run_date_utc>
```

Example:

```bash
scripts/release/generate_burnin_evidence.sh v2.2-burnin-2026-05-01 2026-05-01
```

You can also run the legacy-named burn-in evidence workflow (`.github/workflows/v2_1-burnin-evidence.yml`).
Its naming is historical, but the generated artifact layout is valid for v2.2 evidence collection.

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
```

## Required content
- `runtime-alerts/alerts.csv`: alert timeline with severity, source, and ticket references.
- `snapshot-cadence/snapshot-events.csv`: each snapshot attempt/result with duration.
- `pruning-cadence/pruning-events.csv`: each prune run/result and reclaimed bytes.
- `p2p-recovery/recovery-events.csv`: recovery timing under peer churn/rejoin.
- `restart-recovery-notes/restart-log.md`: restart incidents, startup mode (fast-boot/replay), recovery duration, and follow-up notes.

## Additional v2.2 operator evidence (attach to bundle)
These artifacts should be linked from `README.md` in the run directory:

1. **Runtime/network event stream extracts**
   - Source: `docs/RUNTIME_EVENT_STREAM.md`
   - Include representative events for node health, network churn, and recovery moments.
2. **Dashboard and alert captures**
   - Source: `docs/dashboard/README.md`, `docs/dashboard/ALERTS.md`
   - Include snapshots or exports showing alert transitions and operator acknowledgment.
3. **Config profile declaration for the run**
   - Source: `README.md` (`PULSEDAG_CONFIG_PROFILE`)
   - Record exact profile (`testnet` / `operator`) plus any explicit env overrides.
4. **Runbook execution notes**
   - Source: `docs/runbooks/INDEX.md`
   - Link incident handling steps to concrete runbook paths used by operators.
5. **Release binary provenance**
   - Source: `.github/workflows/release-binaries.yml`
   - Record release artifact identity (tag/build reference, hashes if available).
6. **Mining telemetry summary**
   - Source: `docs/dashboard/README.md` (`GET /runtime/status` mining telemetry fields)
   - Record accepted/rejected submit trends and rejection taxonomy highlights.

## Deterministic artifact expectation
The scaffold generated for the same `run_id` + `run_date_utc` must be byte-for-byte deterministic.
The workflow at `.github/workflows/v2_1-burnin-evidence.yml` includes a determinism check to enforce this.

## Release manager checklist
Before approving v2.2:
1. Confirm evidence completeness for all 14 days.
2. Confirm unresolved Sev-1 incidents are zero, or release is blocked.
3. Confirm cadence records match planned snapshot/pruning schedule.
4. Confirm restart and p2p recovery notes include outcomes and follow-up items.
5. Confirm event-stream and dashboard/alert evidence aligns with incident timeline.
6. Confirm startup mode visibility (fast-boot/replay/fallback counters) is captured for restart drills.
7. Confirm mining telemetry does not show unresolved regression patterns.
8. Attach release manager sign-off to the final evidence bundle.

## Staging reversibility evidence (upgrade + rollback)
For the v2.2 release gate, attach a staging rehearsal bundle that demonstrates operational reversibility:
- Baseline capture prior to upgrade.
- Post-upgrade validation output.
- Post-rollback validation output (including health/coherence checks).
- Operator notes indicating whether snapshot/restore or rebuild steps were required.

Primary runbooks:
- `docs/runbooks/STAGING_UPGRADE.md`
- `docs/runbooks/STAGING_ROLLBACK.md`
