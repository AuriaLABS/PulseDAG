#!/usr/bin/env bash
set -euo pipefail

echo "[snapshot-productization] running export/import/verification/restore coverage"
cargo test -p pulsedag-storage export_import_snapshot_bundle_round_trip_is_coherent -- --nocapture
cargo test -p pulsedag-storage export_import_workflow_is_repeatable_across_multiple_targets -- --nocapture
cargo test -p pulsedag-storage verify_snapshot_bundle_signals_missing_anchor_explicitly -- --nocapture
cargo test -p pulsedag-storage verification_signals_are_explicit_for_chain_mismatch -- --nocapture
cargo test -p pulsedag-storage restore_drill_preserves_recovery_entrypoint_coherence -- --nocapture
cargo test -p pulsedag-storage restore_drill_repeated_runs_produce_coherent_timing_evidence -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_normal_startup_path_has_no_regression_without_snapshot -- --nocapture

echo "[snapshot-productization] completed"
