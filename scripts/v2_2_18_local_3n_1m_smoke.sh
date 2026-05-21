#!/usr/bin/env bash
set -euo pipefail
DURATION_SEC=${DURATION_SEC:-600}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
OUT_DIR="artifacts/v2_2_18_private_rc/local-3n-1m/${RUN_ID}"
mkdir -p "$OUT_DIR"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1
scripts/v2_2_18_preflight_check.sh
NODE_BIN=${NODE_BIN:-./target/release/pulsedag-node}
MINER_BIN=${MINER_BIN:-./target/release/pulsedag-miner}
[[ -x "$NODE_BIN" ]] || { echo "Missing node binary. Build with: cargo build --workspace --release"; exit 1; }
[[ -x "$MINER_BIN" ]] || { echo "Missing miner binary. Build with: cargo build --workspace --release"; exit 1; }
# Scaffold-only safe helper: provides evidence structure and endpoint capture; operators should inject exact launch args.
echo "No default launch args are imposed to avoid behavior changes. Provide NODE_CMD_A/B/C and MINER_CMD externally." > "$OUT_DIR/summary.md"
printf "SKIP: launch commands not set by default for safety\n" >> "$OUT_DIR/summary.md"
printf "127.0.0.1-only policy required\n" > "$OUT_DIR/endpoints-manifest.txt"
printf "N/A (no miner run in default safe scaffold)\n" > "$OUT_DIR/miner-telemetry-grep.txt"
printf "No PIDs started in default scaffold mode\n" > "$OUT_DIR/process-pids.txt"
echo "Smoke helper scaffold complete at $OUT_DIR"
