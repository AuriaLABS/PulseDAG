#!/usr/bin/env bash
set -euo pipefail

echo "[restore-drill] running targeted storage tests for snapshot restore evidence"
cargo test -p pulsedag-storage export_import_snapshot_bundle_round_trip_is_coherent -- --nocapture
cargo test -p pulsedag-storage verify_snapshot_bundle_signals_missing_anchor_explicitly -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_falls_back_when_snapshot_is_corrupt -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_fails_explicitly_with_corrupt_snapshot_and_no_blocks -- --nocapture
cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture
cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture
cargo test -p pulsedag-storage prune_safety_plan_explicitly_caps_to_rollback_window_floor -- --nocapture
cargo test -p pulsedag-storage prune_safety_retains_recovery_viability_after_cleanup -- --nocapture

echo "[restore-drill] completed"
