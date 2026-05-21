# PulseDAG v2.2.18 Private-Testnet RC Plan

## Status
- **PLANNED / BLOCKED BY v2.2.17 EVIDENCE**.
- This plan is staged in advance; execution starts after v2.2.17 closeout evidence is complete.

## Objective
Run a deterministic private-testnet readiness rehearsal for **5 nodes / 4 miners** across:
1. local Windows/WSL path, and
2. Ubuntu/VPS path.

## Topology manifest (target)
| Role | Count | Notes |
|---|---:|---|
| Bootstrap node | 1 | Fixed peer list seed. |
| Validator/full nodes | 4 | Total nodes = 5 including bootstrap. |
| External miners | 4 | Standalone processes, no pool logic. |

Manifest fields (template):
- `node_id`, `host`, `p2p_port`, `rpc_port`, `role`, `miner_binding`, `data_dir`.
- `startup_order` and `shutdown_order` for deterministic orchestration.

## Rehearsal tracks

### A) Local Windows/WSL track
- Define host/port map for 5 node processes and 4 miner processes.
- Validate deterministic startup and shutdown sequence.
- Capture sync convergence and miner telemetry outputs.

### B) Ubuntu/VPS track
- Define multi-host mapping and secure RPC exposure profile.
- Execute same deterministic sequence and record comparable metrics.
- Validate snapshot/restore and perturbation recovery behavior.

## Deterministic startup/shutdown
Startup target flow:
1. Bootstrap node up and healthy.
2. Remaining 4 nodes up in fixed order.
3. 4 miners attach in fixed order.
4. Start timing window for sync convergence.

Shutdown target flow:
1. Stop miners in reverse order.
2. Stop non-bootstrap nodes in reverse order.
3. Stop bootstrap node last.
4. Archive logs + metrics bundle.

## Sync convergence measurement
Record:
- time-to-first-peer for each node,
- time-to-tip-convergence across all 5 nodes,
- divergence window duration during perturbation.

Suggested evidence files:
- `sync/convergence_summary.md`
- `sync/node_tip_samples.csv`
- `sync/peer_counts.csv`

## Miner acceptance/rejection telemetry
Collect per miner:
- accepted shares/blocks,
- rejected shares/blocks,
- reject reasons (stale, invalid, timeout, policy).

Suggested evidence files:
- `miners/accept_reject_summary.md`
- `miners/miner_<id>_events.log`

## Snapshot/restore drill
- Produce snapshot at stable height.
- Restore one follower from snapshot.
- Rejoin and verify reconvergence within threshold.

Suggested evidence files:
- `snapshot/restore_runbook_steps.md`
- `snapshot/restore_timing.csv`

## Perturbation drills
Minimum drills:
- single-node restart,
- transient network partition/rejoin,
- miner disconnect/reconnect storm.

For each drill capture:
- trigger timestamp,
- recovery timestamp,
- steady-state confirmation criteria.

## RPC security verification (reused from v2.2.17)
Reuse v2.2.17 security controls and scripts as baseline:
- endpoint exposure profile checks,
- unsafe admin exposure checks,
- optional operator auth checks,
- body/rate limits and diagnostics redaction checks.

Reference artifacts:
- `docs/OPERATOR_SECURITY_RUNBOOK_V2_2_17.md`
- `docs/RPC_ENDPOINT_INVENTORY_V2_2_17.md`
- `scripts/v2_2_17_rpc_security_smoke.sh`

## Evidence bundle + go/no-go report
Bundle structure (target):
- `checks/` command outputs,
- `topology/manifest.yaml`,
- `sync/`, `miners/`, `snapshot/`, `perturbation/`,
- `security/` reused RPC security verification outputs,
- `summary.md` and `go_no_go_report.md`.

Decision rule:
- go/no-go report may recommend follow-up fixes,
- **must not** claim v2.3.0 readiness from v2.2.18 rehearsal alone.
