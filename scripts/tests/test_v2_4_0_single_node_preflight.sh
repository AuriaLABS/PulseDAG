#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PREFLIGHT="$ROOT_DIR/scripts/v2_4_0_single_node_preflight.sh"
REFERENCE="$ROOT_DIR/configs/single-node/single-node.env.example"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

expect_pass() {
  local file="$1"
  bash "$PREFLIGHT" "$file" >/dev/null
}

expect_fail() {
  local file="$1"
  if bash "$PREFLIGHT" "$file" >/dev/null 2>&1; then
    echo "expected preflight failure for $file" >&2
    exit 1
  fi
}

cp "$REFERENCE" "$TMP_DIR/valid.env"
expect_pass "$TMP_DIR/valid.env"

cp "$REFERENCE" "$TMP_DIR/implicit.env"
sed -i 's/^PULSEDAG_SINGLE_NODE_MODE=true/PULSEDAG_SINGLE_NODE_MODE=false/' "$TMP_DIR/implicit.env"
expect_fail "$TMP_DIR/implicit.env"

cp "$REFERENCE" "$TMP_DIR/seed-role.env"
sed -i 's/^PULSEDAG_PRIVATE_TESTNET_ROLE=single/PULSEDAG_PRIVATE_TESTNET_ROLE=seed/' "$TMP_DIR/seed-role.env"
expect_fail "$TMP_DIR/seed-role.env"

cp "$REFERENCE" "$TMP_DIR/p2p-enabled.env"
sed -i 's/^PULSEDAG_P2P_ENABLED=false/PULSEDAG_P2P_ENABLED=true/' "$TMP_DIR/p2p-enabled.env"
expect_fail "$TMP_DIR/p2p-enabled.env"

cp "$REFERENCE" "$TMP_DIR/bootnode.env"
sed -i 's#^PULSEDAG_P2P_BOOTSTRAP=.*#PULSEDAG_P2P_BOOTSTRAP=/dns4/seed.example.net/tcp/32333/p2p/12D3KooWExample#' "$TMP_DIR/bootnode.env"
expect_fail "$TMP_DIR/bootnode.env"

cp "$REFERENCE" "$TMP_DIR/public-p2p.env"
sed -i 's#^PULSEDAG_PUBLIC_P2P_MULTIADDR=.*#PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/node.example.net/tcp/32333#' "$TMP_DIR/public-p2p.env"
expect_fail "$TMP_DIR/public-p2p.env"

cp "$REFERENCE" "$TMP_DIR/public-rpc.env"
sed -i 's/^PULSEDAG_RPC_BIND=.*/PULSEDAG_RPC_BIND=0.0.0.0:8280/' "$TMP_DIR/public-rpc.env"
expect_fail "$TMP_DIR/public-rpc.env"

cp "$REFERENCE" "$TMP_DIR/public-ready.env"
sed -i 's/^PULSEDAG_PUBLIC_TESTNET_READY=false/PULSEDAG_PUBLIC_TESTNET_READY=true/' "$TMP_DIR/public-ready.env"
expect_fail "$TMP_DIR/public-ready.env"

cp "$REFERENCE" "$TMP_DIR/clock.env"
sed -i 's/^PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false/PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true/' "$TMP_DIR/clock.env"
expect_fail "$TMP_DIR/clock.env"

cp "$REFERENCE" "$TMP_DIR/rehearsal.env"
sed -i 's/^PULSEDAG_MULTI_HOST_REHEARSAL=false/PULSEDAG_MULTI_HOST_REHEARSAL=true/' "$TMP_DIR/rehearsal.env"
expect_fail "$TMP_DIR/rehearsal.env"

cp "$REFERENCE" "$TMP_DIR/contracts.env"
sed -i 's/^PULSEDAG_CONTRACTS_ENABLED=false/PULSEDAG_CONTRACTS_ENABLED=true/' "$TMP_DIR/contracts.env"
expect_fail "$TMP_DIR/contracts.env"

cp "$REFERENCE" "$TMP_DIR/tmp-storage.env"
sed -i 's#^PULSEDAG_ROCKSDB_PATH=.*#PULSEDAG_ROCKSDB_PATH=/tmp/pulsedag#' "$TMP_DIR/tmp-storage.env"
expect_fail "$TMP_DIR/tmp-storage.env"

OUT_DIR="$TMP_DIR/evidence" bash "$PREFLIGHT" "$TMP_DIR/valid.env" >/dev/null
grep -q '"result": "PASS"' "$TMP_DIR/evidence/single-node-preflight.json"
grep -q '"operator_mode": "single-node"' "$TMP_DIR/evidence/single-node-preflight.json"
grep -q '"isolated_mining_authorized": true' "$TMP_DIR/evidence/single-node-preflight.json"
grep -q '"public_testnet_ready": false' "$TMP_DIR/evidence/single-node-preflight.json"
grep -q '"contracts_enabled": false' "$TMP_DIR/evidence/single-node-preflight.json"

echo "PASS: v2.4.0 explicit single-node preflight contract"
