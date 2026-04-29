# v2.2.4 p2p/sync/runtime/rpc baseline output template

Use this template to compare startup/restart/churn runs side-by-side.

## Run metadata

- Run A ID:
- Run B ID:
- Run C ID:
- Node URL:
- Hot paths captured (`sync`, `relay`, `mempool`, `mining`, `recovery`):
- Topology notes:
- Date captured (UTC):

## RPC latency comparison (p95 ms)

| Endpoint | Run A | Run B | Run C | Notes |
| --- | ---: | ---: | ---: | --- |
| `/runtime/status` |  |  |  |  |
| `/status` |  |  |  |  |
| `/sync/status` |  |  |  |  |
| `/p2p/status` |  |  |  |  |
| `/blocks/latest` |  |  |  |  |
| `/tx/mempool` |  |  |  |  |
| `/address/ping` |  |  |  |  |

## Sync selected-peer stabilization comparison

| Metric | Run A | Run B | Run C |
| --- | ---: | ---: | ---: |
| Result |  |  |  |
| Elapsed seconds |  |  |  |
| Polls |  |  |  |
| Selected peer |  |  |  |
| Final lag |  |  |  |

## Restore/rebuild drill timing comparison

| Command | Run A (s) | Run B (s) | Run C (s) | Drift (%) |
| --- | ---: | ---: | ---: | ---: |
| `restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence` |  |  |  |  |
| `restore_drill_repeated_runs_produce_coherent_timing_evidence` |  |  |  |  |
| `replay_blocks_or_init_uses_snapshot_plus_delta_after_prune` |  |  |  |  |

## Artifact references

- `docs/benchmarks/artifacts/<run-id>/BASELINE_REPORT.md`
- `docs/benchmarks/artifacts/<run-id>/rpc_latency_samples.csv`
- `docs/benchmarks/artifacts/<run-id>/rpc_latency_summary.json`
- `docs/benchmarks/artifacts/<run-id>/sync_stabilization.json`
- `docs/benchmarks/artifacts/<run-id>/drill_command_results.json`
