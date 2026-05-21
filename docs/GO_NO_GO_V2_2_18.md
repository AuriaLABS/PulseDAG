# Go/No-Go Generator (v2.2.18)

This document defines the automated go/no-go report generator for release `v2.2.18`.

## Script

- `scripts/v2_2_18_generate_go_no_go.sh`

## Inputs

The script requires:

- evidence directory
- topology manifest
- sync summary
- miner telemetry summary
- perturbation summary
- restore summary
- command check outputs

CLI flags:

- `--evidence-dir`
- `--topology-manifest`
- `--sync-summary`
- `--miner-telemetry-summary`
- `--perturbation-summary`
- `--restore-summary`
- `--command-check-outputs`
- `--output` (optional, default: `<evidence-dir>/go-no-go.md`)

## Output

- `go-no-go.md`

## Decision values

- `GO`
- `CONDITIONAL_GO`
- `NO_GO`
- `PENDING_EVIDENCE`

## Hard NO-GO rules

The generated decision is `NO_GO` when any of the following are detected:

- `cargo fmt/test/build` evidence missing
- any unresolved Sev-1 consensus/sync issue
- nodes do not converge
- miner submit path fails completely
- restore/rebuild drill fails without retest
- admin RPC exposed unsafely
- evidence bundle missing

## Explicit non-goals / guardrails

- Do not automatically mark `v2.3.0` ready.
- Do not hide missing evidence.
- Do not convert failures to warnings without waiver.

## Example

```bash
scripts/v2_2_18_generate_go_no_go.sh \
  --evidence-dir artifacts/v2_2_18_release/run-01 \
  --topology-manifest artifacts/v2_2_18_release/run-01/topology-manifest.md \
  --sync-summary artifacts/v2_2_18_release/run-01/sync-summary.md \
  --miner-telemetry-summary artifacts/v2_2_18_release/run-01/miner-telemetry-summary.md \
  --perturbation-summary artifacts/v2_2_18_release/run-01/perturbation-summary.md \
  --restore-summary artifacts/v2_2_18_release/run-01/restore-summary.md \
  --command-check-outputs artifacts/v2_2_18_release/run-01/command-check-outputs.md
```
