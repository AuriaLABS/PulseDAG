#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  exec python3 scripts/p2p_sync_rpc_baselines.py --help
fi

args=()
if [[ "${1:-}" != "" && "${1:-}" != --* ]]; then
  base_url="$1"
  shift
  args+=(--base-url "$base_url")
fi

exec python3 scripts/p2p_sync_rpc_baselines.py \
  "${args[@]}" \
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
  --drill-command "cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture" \
  "$@"
