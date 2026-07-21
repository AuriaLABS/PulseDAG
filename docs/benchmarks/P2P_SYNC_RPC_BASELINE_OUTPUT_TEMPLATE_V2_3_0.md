# v2.3.0 P2P, sync, runtime, and RPC baseline comparison

Use this template to compare startup, restart, churn, rejoin, and recovery runs.

## Run metadata

- Exact candidate SHA:
- Run A ID:
- Run B ID:
- Run C ID:
- Node URL:
- Hot paths captured:
- Topology and host notes:
- Operator:
- UTC capture window:

## RPC latency comparison

| Endpoint | Run A p95 ms | Run B p95 ms | Run C p95 ms | Success-rate notes |
|---|---:|---:|---:|---|
| `/runtime/status` | | | | |
| `/status` | | | | |
| `/sync/status` | | | | |
| `/p2p/status` | | | | |
| `/blocks/latest` | | | | |
| `/tx/mempool` | | | | |
| `/address/ping` | | | | |

## Selected-peer and sync stabilization

| Metric | Run A | Run B | Run C |
|---|---:|---:|---:|
| Result | | | |
| Elapsed seconds | | | |
| Polls | | | |
| Selected peer | | | |
| Final lag | | | |

## Restore and recovery timing

| Command or drill | Run A seconds | Run B seconds | Run C seconds | Drift % |
|---|---:|---:|---:|---:|
| Snapshot plus delta restore | | | | |
| Repeated restore coherence | | | | |
| Replay after prune | | | | |

## Threshold classification

- Threshold profile:
- Overall result: **PASS / FAIL**
- Exceptions or environmental noise:
- Required retest:

## Artifact references

- `docs/benchmarks/artifacts/<run-id>/BASELINE_REPORT.md`
- `docs/benchmarks/artifacts/<run-id>/run_meta.json`
- `docs/benchmarks/artifacts/<run-id>/rpc_latency_samples.csv`
- `docs/benchmarks/artifacts/<run-id>/rpc_latency_summary.json`
- `docs/benchmarks/artifacts/<run-id>/sync_stabilization.json`
- `docs/benchmarks/artifacts/<run-id>/drill_command_results.json`
- `docs/benchmarks/artifacts/<run-id>/regression_thresholds.json`

## Review

- Reviewer:
- Review date (UTC):
- Result accepted: **YES / NO**
- Blocking issue IDs:
