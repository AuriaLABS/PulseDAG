# v2.3.0 P2P, sync, runtime, and RPC baseline methodology

This methodology defines repeatable operator measurements for restart/rejoin recovery, sync convergence, restore/rebuild timing, hot paths, and key read-side RPC surfaces.

## Guardrails

- The methodology does not change consensus behavior.
- Mining remains external and standalone.
- Pool logic remains outside the node.
- Measurements are evidence, not public-testnet launch authorization.

## Captured surfaces

1. Runtime responsiveness: `GET /runtime/status`.
2. Read-side RPC latency:
   - `GET /status`
   - `GET /sync/status`
   - `GET /p2p/status`
   - `GET /blocks/latest`
   - `GET /tx/mempool`
   - `GET /address/ping`
3. Selected-peer stabilization and sync lag.
4. Optional restore, replay, and recovery command timing.
5. Optional hot-path groups: sync, relay, mempool, mining, and recovery.

## Repeatable command

```bash
scripts/p2p-sync-rpc-baseline.sh http://127.0.0.1:8080
```

Manual equivalent:

```bash
python3 scripts/p2p_sync_rpc_baselines_v2_3_0.py \
  --base-url http://127.0.0.1:8080 \
  --iterations 30 \
  --sync-stable-polls 5 \
  --sync-max-wait-seconds 180 \
  --sync-lag-threshold 0 \
  --drill-command "cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture"
```

## Restart and rejoin comparison

1. Capture the pre-event baseline.
2. Perform the documented restart, rejoin, or recovery operation.
3. Capture the post-event baseline using the same topology and host class.
4. Compare p95 latency, success rate, selected-peer stabilization, final lag, and drill timing.
5. Record the exact candidate SHA, node configuration, operator, and UTC window.

## Output

Each run writes a directory under:

```text
docs/benchmarks/artifacts/v2_3_0_<timestamp>/
```

Expected files:

- `run_meta.json`
- `rpc_latency_samples.csv`
- `rpc_latency_summary.json`
- `sync_stabilization.json`
- `drill_command_results.json`
- `regression_thresholds.json`
- `BASELINE_REPORT.md`

Use [`P2P_SYNC_RPC_BASELINE_OUTPUT_TEMPLATE_V2_3_0.md`](P2P_SYNC_RPC_BASELINE_OUTPUT_TEMPLATE_V2_3_0.md) for side-by-side comparison.

## Hot-path extension

```bash
scripts/hot-path-baseline.sh http://127.0.0.1:8080
```

Detailed hot-path guidance remains in [`HOT_PATH_MEASUREMENT_METHODOLOGY.md`](HOT_PATH_MEASUREMENT_METHODOLOGY.md).

## Evidence hygiene

- Keep topology, seed set, node configuration, and host class constant between compared runs.
- Capture at least three runs per scenario.
- Treat isolated outliers cautiously; compare p95 and stabilization timing first.
- Re-capture after dependency, host, configuration, or runtime-path changes.
- Do not use private-testnet baselines to claim that the 30-day public-testnet burn-in has started or completed.
