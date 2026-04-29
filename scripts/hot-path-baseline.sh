#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${1:-http://127.0.0.1:8080}"

python3 scripts/p2p_sync_rpc_baselines.py \
  --base-url "${BASE_URL}" \
  --iterations 30 \
  --sync-stable-polls 5 \
  --sync-max-wait-seconds 180 \
  --sync-lag-threshold 0 \
  --hot-path sync \
  --hot-path relay \
  --hot-path mempool \
  --hot-path mining \
  --hot-path recovery \
  --drill-command "cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture" \
  --drill-command "cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture"
