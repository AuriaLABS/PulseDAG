#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PREFLIGHT="$ROOT_DIR/scripts/v2_3_0_private_testnet_preflight.sh"
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

# Active examples must remain on the dedicated v2.4.0 identity. Historical
# v2.3.0 compatibility is tested only against isolated temporary fixtures.
grep -q '^PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0$' \
  "$ROOT_DIR/configs/private-testnet/seed.env.example"
grep -q '^PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0$' \
  "$ROOT_DIR/configs/private-testnet/seed.env.example"
grep -q '^PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0$' \
  "$ROOT_DIR/configs/private-testnet/node.env.example"
grep -q '^PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0$' \
  "$ROOT_DIR/configs/private-testnet/node.env.example"

cp "$ROOT_DIR/configs/private-testnet/seed.env.example" "$TMP_DIR/seed.env"
cp "$ROOT_DIR/configs/private-testnet/node.env.example" "$TMP_DIR/node.env"
for fixture in "$TMP_DIR/seed.env" "$TMP_DIR/node.env"; do
  sed -i \
    -e 's/^PULSEDAG_NETWORK_PROFILE=.*/PULSEDAG_NETWORK_PROFILE=private-testnet-v2.3.0/' \
    -e 's/^PULSEDAG_CHAIN_ID=.*/PULSEDAG_CHAIN_ID=pulsedag-private-v2.3.0/' \
    "$fixture"
done
expect_pass "$TMP_DIR/seed.env"
expect_pass "$TMP_DIR/node.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/no-bootnode.env"
sed -i 's#^PULSEDAG_P2P_BOOTSTRAP=.*#PULSEDAG_P2P_BOOTSTRAP=#' "$TMP_DIR/no-bootnode.env"
expect_fail "$TMP_DIR/no-bootnode.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/missing-peer-id.env"
sed -i 's#/p2p/[^,]*##' "$TMP_DIR/missing-peer-id.env"
expect_fail "$TMP_DIR/missing-peer-id.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/self-bootstrap.env"
sed -i 's#^PULSEDAG_PUBLIC_P2P_MULTIADDR=.*#PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/seed-1.example.net/tcp/32333#' \
  "$TMP_DIR/self-bootstrap.env"
expect_fail "$TMP_DIR/self-bootstrap.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/mdns.env"
sed -i 's/^PULSEDAG_P2P_MDNS=false/PULSEDAG_P2P_MDNS=true/' "$TMP_DIR/mdns.env"
expect_fail "$TMP_DIR/mdns.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/chain.env"
sed -i 's/^PULSEDAG_CHAIN_ID=.*/PULSEDAG_CHAIN_ID=wrong-chain/' "$TMP_DIR/chain.env"
expect_fail "$TMP_DIR/chain.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/remote-rpc.env"
sed -i 's/^PULSEDAG_RPC_BIND=.*/PULSEDAG_RPC_BIND=0.0.0.0:8280/' "$TMP_DIR/remote-rpc.env"
expect_fail "$TMP_DIR/remote-rpc.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/admin-no-token.env"
sed -i 's/^PULSEDAG_ADMIN_ENABLED=false/PULSEDAG_ADMIN_ENABLED=true/' "$TMP_DIR/admin-no-token.env"
expect_fail "$TMP_DIR/admin-no-token.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/public-claim.env"
printf '\nPULSEDAG_PUBLIC_TESTNET_READY=true\n' >> "$TMP_DIR/public-claim.env"
expect_fail "$TMP_DIR/public-claim.env"

OUT_DIR="$TMP_DIR/evidence" bash "$PREFLIGHT" "$TMP_DIR/node.env" >/dev/null
grep -q '"result": "PASS"' "$TMP_DIR/evidence/private-testnet-preflight.json"
grep -q '"public_testnet_ready": false' "$TMP_DIR/evidence/private-testnet-preflight.json"

echo "PASS: isolated historical v2.3.0 private-testnet preflight contract"
