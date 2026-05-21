# Rehearsal Duration Modes (v2.2.18)

This document defines standardized rehearsal modes for v2.2.18 so operators can choose an execution depth that matches the objective.

> Guardrails:
> - `smoke` and `local` are for developer and operator sanity checks only.
> - They are **not** sufficient evidence to claim v2.3.0 readiness.
> - Quick developer checks must **not** require a 24h run.

## Mode matrix

| Mode | Topology | Duration target | Required evidence | Pass/Fail threshold | Operator effort | Windows/WSL | VPS |
|---|---|---|---|---|---|---|---|
| `smoke` | 1 node + 0-1 miner (local process) | 10-15 minutes | startup log, `/status` response sample, one process snapshot | PASS if node stays healthy and serves `/status`; FAIL on startup crash or repeated health failures | Low | Yes | Yes |
| `local` | 3 nodes + 1 miner | 1-2 hours | timeline, node/miner logs, periodic health checks, tip convergence samples | PASS if all 3 nodes stay reachable and converge repeatedly with no unresolved crashes | Medium | Yes | Yes |
| `staging` | 3-5 nodes + 2 miners | 6-12 hours | all local evidence + restart drill + summary report | PASS if topology remains stable for target window and restart drill reconverges within run window | Medium/High | Yes (resource-dependent) | Yes |
| `rc-full` | 5 nodes + 4 miners | 24 hours | full RC bundle (`sync/`, `miners/`, `snapshot/`, `perturbation/`, `security/`, `go/no-go`) | PASS only with sustained stability + complete evidence pack + explicit unresolved-risk review | High | Usually No (unless strong host) | Yes (recommended) |

## Usage guidance

- Use `smoke` before commits, local environment upgrades, or quick troubleshooting.
- Use `local` for pre-merge rehearsal and daily operational sanity.
- Use `staging` before candidate tags or major environment changes.
- Use `rc-full` for release-candidate closeout evidence.

## Script support (v2.2.18)

Where practical, scripts support a mode selector to map defaults automatically:

- `scripts/v2_2_18_start_vps_rehearsal.sh`
  - `REHEARSAL_MODE=smoke|local|staging|rc-full`
  - Auto-derives default node/miner shape from mode unless overridden.
- `scripts/v2_2_18_supervisor.sh`
  - `REHEARSAL_MODE=...` metadata is recorded in timeline and snapshots for traceability.

## Evidence minimums by mode

- `smoke`
  - health check output (`/status`) from at least one sample interval.
  - startup and shutdown process snapshot.
- `local`
  - `timeline.md`, node logs, miner logs, and at least one convergence observation.
- `staging`
  - local evidence + at least one controlled restart/recovery drill with timestamps.
- `rc-full`
  - staged evidence + full private-testnet RC structure and go/no-go conclusion.

