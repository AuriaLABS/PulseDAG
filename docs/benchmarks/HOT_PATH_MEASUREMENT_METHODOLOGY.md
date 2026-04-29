# Hot-path measurement methodology (sync, relay, mempool, mining, recovery)

Date: **2026-04-29**

This methodology extends the existing v2.2.4 baseline harnesses to cover the hot paths that matter most for optimization planning and public-testnet readiness.

## Guardrails

- No consensus behavior is changed.
- Miner remains external/standalone (`apps/pulsedag-miner`).
- No pool logic is introduced.
- Work is measurement-first and repeatable (not premature optimization).
- Output format is practical and comparable run-to-run.

## Hot paths captured

The harness now supports explicit hot-path groups:

- `sync` → `/sync/status`, `/sync/lag`, `/runtime/status`
- `relay` → `/p2p/status`, `/runtime/status`
- `mempool` → `/tx/mempool`, `/runtime/status`
- `mining` → `/pow/health`, `/runtime/status`
- `recovery` → `/status`, `/sync/status`, `/runtime/status`

These groups are additive and can be mixed with manual `--endpoint` values.

## Repeatable capture command

```bash
scripts/hot-path-baseline.sh http://127.0.0.1:8080
```

Manual equivalent:

```bash
python3 scripts/p2p_sync_rpc_baselines.py \
  --base-url http://127.0.0.1:8080 \
  --iterations 30 \
  --hot-path sync \
  --hot-path relay \
  --hot-path mempool \
  --hot-path mining \
  --hot-path recovery \
  --sync-stable-polls 5 \
  --sync-max-wait-seconds 180 \
  --sync-lag-threshold 0
```

## Repeatability guidance

1. Keep topology and config constant between runs.
2. Capture at least 3 runs per scenario (`startup`, `post-restart`, `post-churn/rejoin`).
3. Compare p95 latency and stabilization timing before comparing means.
4. Re-capture after host class, compiler, dependency, or major config changes.

## Output and comparability

Artifacts are written under:

```text
docs/benchmarks/artifacts/v2_2_4_<timestamp>/
```

Comparison-critical outputs:

- `run_meta.json` (includes selected `hot_paths`)
- `rpc_latency_samples.csv`
- `rpc_latency_summary.json`
- `sync_stabilization.json`
- `drill_command_results.json`
- `BASELINE_REPORT.md`

Use `docs/benchmarks/V2_2_4_P2P_SYNC_RPC_BASELINE_OUTPUT_TEMPLATE.md` for side-by-side A/B/C run reviews.
