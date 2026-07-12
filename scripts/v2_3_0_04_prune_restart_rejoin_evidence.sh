#!/usr/bin/env bash
# v2.3.0 Task 04 — Non-zero pruning, restart, and rejoin evidence gate
#
# Validates the retained-set prune, snapshot+delta restart, and offline rejoin
# lifecycle. Fails fast if any required test category reports zero coverage or
# a hard failure.
#
# Usage:
#   ./scripts/v2_3_0_04_prune_restart_rejoin_evidence.sh
#
# The script mirrors the validation section of:
#   docs/codex_tasks/v2_3_0_04_prune_restart_rejoin_evidence.md

set -euo pipefail

echo "[v2_3_0_04] starting prune / restart / rejoin evidence gate"
echo "[v2_3_0_04] commit: $(git rev-parse HEAD)"
echo "[v2_3_0_04] branch: $(git rev-parse --abbrev-ref HEAD)"

# ── 1. Format check ──────────────────────────────────────────────────────────
echo "[v2_3_0_04] 1/8  cargo fmt --check"
cargo fmt --all -- --check

# ── 2. Workspace check ───────────────────────────────────────────────────────
echo "[v2_3_0_04] 2/8  cargo check"
cargo check --workspace --locked

# ── 3. Storage: pruning (retained-set non-zero invariant) ────────────────────
echo "[v2_3_0_04] 3/8  cargo test -p pulsedag-storage pruning"
cargo test -p pulsedag-storage pruning --locked

# ── 4. Storage: snapshot round-trip and verification ────────────────────────
echo "[v2_3_0_04] 4/8  cargo test -p pulsedag-storage snapshot"
cargo test -p pulsedag-storage snapshot --locked

# ── 5. Core: replay determinism (includes snapshot_restore) ─────────────────
echo "[v2_3_0_04] 5/8  cargo test -p pulsedag-core replay"
cargo test -p pulsedag-core replay --locked

# ── 6. Daemon: auto_prune gate and retained-set model ───────────────────────
echo "[v2_3_0_04] 6/8  cargo test -p pulsedagd auto_prune"
cargo test -p pulsedagd auto_prune --locked

# ── 7. Daemon: restart_rejoin lifecycle ─────────────────────────────────────
echo "[v2_3_0_04] 7/8  cargo test -p pulsedagd restart_rejoin"
cargo test -p pulsedagd restart_rejoin --locked

# ── 8. Workspace: snapshot_restore coverage ─────────────────────────────────
echo "[v2_3_0_04] 8/8  cargo test --workspace snapshot_restore"
cargo test --workspace --locked snapshot_restore

echo "[v2_3_0_04] all evidence gates passed"
