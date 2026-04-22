# v2.1 release evidence bundle

This document defines the standard evidence package for v2.1 burn-in and release readiness.

## Evidence bundle generation
Generate the base structure with:

```bash
scripts/release/generate_burnin_evidence.sh <run_id> <run_date_utc>
```

Example:

```bash
scripts/release/generate_burnin_evidence.sh v2.1-burnin-2026-05-01 2026-05-01
```

Or via GitHub Actions workflow: `v2.1 Burn-in Evidence Bundle`.

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
- `restart-recovery-notes/restart-log.md`: restart incidents and operator notes.

## Deterministic artifact expectation
The scaffold generated for the same `run_id` + `run_date_utc` must be byte-for-byte deterministic.
The `v2.1 Burn-in Evidence Bundle` workflow includes a determinism check to enforce this.

## Release manager checklist
Before approving v2.1:
1. Confirm evidence completeness for all 14 days.
2. Confirm unresolved Sev-1 incidents are zero, or release is blocked.
3. Confirm cadence records match planned snapshot/pruning schedule.
4. Confirm restart and p2p recovery notes include outcomes and follow-up items.
5. Attach sign-off to the final evidence bundle.


## Staging reversibility evidence (upgrade + rollback)
For the v2.1 release gate, attach a staging rehearsal bundle that demonstrates operational reversibility:
- Baseline capture prior to upgrade.
- Post-upgrade validation output.
- Post-rollback validation output (including health/coherence checks).
- Operator notes that indicate whether snapshot/restore rebuild steps were required.

Primary runbooks:
- `docs/runbooks/STAGING_UPGRADE.md`
- `docs/runbooks/STAGING_ROLLBACK.md`

