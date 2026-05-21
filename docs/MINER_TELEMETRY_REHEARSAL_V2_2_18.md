# Miner Telemetry Rehearsal v2.2.18

This runbook defines the v2.2.18 miner telemetry rehearsal workflow and the expected telemetry output fields.

## Deliverables

- Script: `scripts/v2_2_18_miner_telemetry_rehearsal.sh`
- Artifacts:
  - `artifacts/v2.2.18/miner-telemetry-rehearsal/<run_id>/summary.json`
  - `artifacts/v2.2.18/miner-telemetry-rehearsal/<run_id>/summary.md`

## Collected telemetry

The rehearsal collects and reports:

- templates received
- stale templates skipped
- submits total
- submits accepted
- submits rejected
- reject codes
- backend cpu/gpu
- hashes/sec if available
- last accepted height
- miner restarts
- node target URL

## Scenarios

Set `SCENARIO` before each run:

1. `one-miner-node-a` (1 miner against node A)
2. `two-miners-node-a` (2 miners against node A)
3. `four-miners-rc-shape` (4 miners distributed across nodes in RC shape)

Example:

```bash
SCENARIO=two-miners-node-a \
NODE_TARGET_URL=http://127.0.0.1:18080 \
MINER_LOG_GLOB='artifacts/v2.2.18/miners/scenario2/*.log' \
bash scripts/v2_2_18_miner_telemetry_rehearsal.sh
```

## GPU policy

- GPU is optional.
- If GPU is unavailable or not implemented, record `SKIP` (not `FAIL`).
- GPU must never bypass CPU/core verification.

## Guardrails

- Do not add pool shares.
- Do not add payouts.
- Do not require GPU.
- Do not change the mining protocol.

## Environment variables

- `RUN_ID` (default: UTC timestamp)
- `ARTIFACT_ROOT` (default: `artifacts/v2.2.18/miner-telemetry-rehearsal`)
- `SCENARIO` (default: `one-miner-node-a`)
- `NODE_TARGET_URL` (default: `http://127.0.0.1:18080`)
- `GPU_REQUESTED` (`auto|true|false`, default: `auto`)
- `MINER_LOG_GLOB` (glob of miner logs to parse)

## Validation

Required syntax check:

```bash
bash -n scripts/v2_2_18_miner_telemetry_rehearsal.sh
```
