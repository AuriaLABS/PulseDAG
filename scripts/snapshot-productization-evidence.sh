#!/usr/bin/env bash
set -euo pipefail

echo "[snapshot-productization] running export/import/verification/restore coverage"
cargo test -p pulsedag-storage export_import_snapshot_bundle_round_trip_is_coherent -- --nocapture
cargo test -p pulsedag-storage verify_snapshot_bundle_signals_missing_anchor_explicitly -- --nocapture
cargo test -p pulsedag-storage restore_drill_preserves_recovery_entrypoint_coherence -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_normal_startup_path_has_no_regression_without_snapshot -- --nocapture

echo "[snapshot-productization] completed"
