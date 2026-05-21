# Supervisor v2.2.18 (Deterministic Nodes + Miners)

`script/v2_2_18_supervisor.sh` (path below) provides a deterministic operator wrapper for private-testnet rehearsals using the topology manifest.

- Script: `scripts/v2_2_18_supervisor.sh`
- Default topology: `configs/private-testnet/v2_2_18/topology.local-3n-1m.json`

## Scope

This supervisor is intentionally narrow:

- Starts node processes from manifest entries.
- Starts miner processes from manifest entries.
- Polls node health using `/status` RPC checks.
- Tracks node/miner PIDs in a run-scoped state directory.
- Supports clean shutdown (`stop`) and targeted restarts.
- Writes deterministic event timeline to `timeline.md`.
- Writes process table snapshots for evidence capture.

## Non-goals

- No mining pool coordination.
- No shares/payout management.
- No miner strategy/behavior changes.
- No requirement for public network access.

## Commands

```bash
# start all nodes and miners
bash scripts/v2_2_18_supervisor.sh start

# one-shot health pass/fail checks across nodes
bash scripts/v2_2_18_supervisor.sh health-once

# continuous health loop
bash scripts/v2_2_18_supervisor.sh monitor-health

# restart a specific node/miner by id from topology manifest
bash scripts/v2_2_18_supervisor.sh restart-node node-2
bash scripts/v2_2_18_supervisor.sh restart-miner miner-local-1

# collect one process table snapshot
bash scripts/v2_2_18_supervisor.sh snapshot

# graceful stop all known miners then nodes
bash scripts/v2_2_18_supervisor.sh stop
```

## Required timeline events

The supervisor records these events in `timeline.md`:

- `node_started`
- `node_stopped`
- `node_restarted`
- `miner_started`
- `miner_stopped`
- `miner_restarted`
- `health_check_pass`
- `health_check_fail`
- `evidence_collected`

## Output layout

Derived from `evidence_directory` in topology:

- `timeline.md`
- `logs/nodes/*.log`
- `logs/miners/*.log`
- `process_snapshots/process-table-*.txt`

PID files are stored under:

- `.supervisor-v2_2_18/<run_id>/nodes/*.pid`
- `.supervisor-v2_2_18/<run_id>/miners/*.pid`

## Environment overrides

- `TOPOLOGY_MANIFEST` (path to JSON topology)
- `PULSEDAGD_BIN` (node binary path)
- `PULSEDAG_MINER_BIN` (miner binary path)
- `SUPERVISOR_STATE_DIR` (state root)
- `SUPERVISOR_HEALTH_INTERVAL_SECS`
- `SUPERVISOR_HEALTH_TIMEOUT_SECS`
