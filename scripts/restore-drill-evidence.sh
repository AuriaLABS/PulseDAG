#!/usr/bin/env bash
set -euo pipefail

echo "[restore-drill] running targeted storage tests for snapshot restore evidence"
cargo test -p pulsedag-storage replay_blocks_or_init_uses_snapshot_plus_delta_after_prune -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_falls_back_when_snapshot_is_corrupt -- --nocapture
cargo test -p pulsedag-storage replay_blocks_or_init_fails_explicitly_with_corrupt_snapshot_and_no_blocks -- --nocapture
cargo test -p pulsedag-storage restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence -- --nocapture

echo "[restore-drill] completed"
