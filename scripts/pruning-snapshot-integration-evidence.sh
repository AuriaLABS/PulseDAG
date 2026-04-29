#!/usr/bin/env bash
set -euo pipefail

echo "[pruning-snapshot] running coherence checks for prune + snapshot + restore/rebuild workflows"
cargo test -p pulsedag-storage prune_safety_retains_recovery_viability_after_cleanup -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture
cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture
cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture
cargo test -p pulsedag-storage restore_drill_preserves_recovery_entrypoint_coherence -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_normal_startup_path_has_no_regression_without_snapshot -- --nocapture

echo "[pruning-snapshot] completed"
