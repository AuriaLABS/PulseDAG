# v2.2.4 p2p/sync/runtime/rpc baseline methodology

Date: **2026-04-28**

This methodology adds repeatable, operator-facing baseline measurements for restart/rejoin recovery, sync convergence, restore/rebuild drill timing, and key read-side RPC surfaces.

## Guardrails

- No consensus behavior is changed by this work.
- Miner remains external/standalone (`apps/pulsedag-miner`).
- No pool logic is introduced.
- The focus is baseline measurement + documentation, not premature optimization.

## Surfaces captured

1. **Runtime/status responsiveness**
   - `GET /runtime/status`
2. **Read-side RPC latency baselines**
   - `GET /status`
   - `GET /sync/status`
   - `GET /p2p/status`
   - `GET /blocks/latest`
   - `GET /tx/mempool`
   - `GET /address/ping`
3. **Sync selected-peer stabilization timing**
   - Polls `GET /sync/status` until selected peer stays stable for N polls and lag is at/under threshold.
4. **Restore/rebuild drill command timing**
   - Times selected restore/replay drill tests (storage crate) with wall-clock duration.

## Repeatable command

From repository root:

```bash
scripts/p2p-sync-rpc-baseline.sh http://127.0.0.1:8080
```

Manual equivalent:

```bash
python3 scripts/p2p_sync_rpc_baselines.py \
  --base-url http://127.0.0.1:8080 \
  --iterations 30 \
  --sync-stable-polls 5 \
  --sync-max-wait-seconds 180 \
  --sync-lag-threshold 0 \
  --drill-command "cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture"
```

## Churn/rejoin and restart capture workflow

Use normal operator actions (or existing node start/stop scripts) while running the baseline harness before and after churn:

1. Capture pre-churn baseline.
2. Induce churn/restart (example: stop node B and bring it back with `scripts/start-node-b.ps1`).
3. Re-run baseline capture.
4. Compare artifacts for stabilization time and latency drift.

This keeps operational baselines grounded in measured behavior without introducing runtime logic changes.

## Output and comparison format

Each run writes artifacts under:

```text
docs/benchmarks/artifacts/v2_2_4_<timestamp>/
```

Produced files:

- `run_meta.json` — run ID, target URL, endpoints, iterations.
- `rpc_latency_samples.csv` — per-sample endpoint latency (ms), status, and errors.
- `rpc_latency_summary.json` — mean/p50/p95/max and success rate per endpoint.
- `sync_stabilization.json` — selected peer stabilization result and elapsed seconds.
- `drill_command_results.json` — timed command results for restore/rebuild drills.
- `BASELINE_REPORT.md` — operator-facing markdown summary table.

## Baseline hygiene

- Keep topology, seed set, and node configuration constant between runs.
- Capture at least 3 runs per scenario (startup, post-restart, post-churn).
- Compare p95 latency and stabilization timing first; treat single-run outliers cautiously.
- Re-capture after dependency updates, host class changes, or major config changes.
