# Sync Convergence Measurement (v2.2.18)

This runbook defines the private RC sync convergence evidence flow for `v2.2.18`.

## Scope

The measurement loop periodically captures these endpoints per node:

- `/status`
- `/sync/status`
- `/p2p/status`
- `/dag/consistency` when available (best effort)

Per sample, the script records when available:

- current tip
- current height
- persisted block count
- peer count
- sync lag estimate

## Script

- `scripts/v2_2_18_measure_sync_convergence.sh`

## Outputs

The script writes evidence under `evidence/v2.2.18/sync-convergence/<run-id>/`:

- `sync-samples.csv`
- `sync-summary.md`
- `convergence-events.md`

## Pass/Fail Policy

A run is acceptable only when all checks are evidenced as PASS:

- nodes should converge after startup
- restarted node should return to baseline
- isolated node should recover after reconnect
- any persistent divergence is FAIL

## Safety Constraints

- Do **not** change consensus behavior for measurement.
- Do **not** hide divergence in reports.
- Do **not** mark PASS without endpoint evidence in generated artifacts.

## Usage

```bash
bash scripts/v2_2_18_measure_sync_convergence.sh
```

Optional environment controls:

- `PULSEDAG_SYNC_CONVERGENCE_NODE_LABELS` (CSV)
- `PULSEDAG_SYNC_CONVERGENCE_NODE_URLS` (CSV)
- `PULSEDAG_SYNC_CONVERGENCE_SAMPLE_INTERVAL_SECS`
- `PULSEDAG_SYNC_CONVERGENCE_SAMPLE_COUNT`
- `PULSEDAG_SYNC_CONVERGENCE_RESTARTED_NODE`
- `PULSEDAG_SYNC_CONVERGENCE_ISOLATED_NODE`

## Required Validation

```bash
bash -n scripts/v2_2_18_measure_sync_convergence.sh
```
