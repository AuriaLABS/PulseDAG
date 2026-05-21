# Upgrade/Rollback Dry Run (v2.2.18)

This document describes a **non-destructive** upgrade/rollback dry run for one non-seed node on v2.2.18.

## Purpose

- Rehearse operational upgrade and rollback mechanics without changing storage format.
- Capture objective evidence (timestamps, health checks, and logs).
- Verify the node can rejoin after both upgrade and rollback actions.

## Guardrails

- Do **not** change storage format as part of this run.
- Do **not** auto-delete node data.
- Do **not** force a destructive rollback path.
- Do **not** claim compatibility beyond captured evidence from this dry run.

## Prerequisites

1. A running non-seed node managed by systemd (or equivalent), with:
   - known service name
   - known binary path
   - known data path reference
2. Candidate v2.2.18 binary path (or simulated release artifact path).
3. Optional rollback binary path (current/stable binary).
4. `curl` available to query node health endpoint(s).
5. Sufficient permissions to stop/start the node service and read logs.

## Recommended execution

Use:

```bash
scripts/v2_2_18_upgrade_rollback_dry_run.sh \
  --service pulsedagd \
  --health-url http://127.0.0.1:8080/status \
  --binary-current /opt/pulsedag/bin/pulsedagd-current \
  --binary-candidate /opt/pulsedag/bin/pulsedagd-v2.2.18 \
  --binary-active-link /opt/pulsedag/bin/pulsedagd-active \
  --data-path /var/lib/pulsedag \
  --logs-dir artifacts/upgrade-rollback-dry-run \
  --rollback-binary /opt/pulsedag/bin/pulsedagd-current
```

If `--rollback-binary` is omitted, the script records that rollback was skipped by configuration.

## Expected scenario flow

1. Record current binary version.
2. Stop one non-seed node.
3. Record backup references (binary and data path metadata).
4. Switch to candidate binary path (or simulated release artifact).
5. Restart node.
6. Verify health and rejoin.
7. Roll back to previous binary **if configured**.
8. Verify health and rejoin again.
9. Collect timings and logs.

## Evidence produced

The script writes a UTC run directory under `--logs-dir`, including:

- `timeline.log` (step-by-step timestamps)
- `summary.env` (key run metadata and timing values)
- `health_post_upgrade.txt`
- `health_post_rollback.txt` (if rollback executed)
- `journal.log` (best-effort service log extract)

Keep these artifacts with release validation evidence; they support operational confidence, not a broad compatibility claim.
