#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PREFLIGHT="${ROOT_DIR}/scripts/v2_4_0_public_testnet_preflight.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

write_valid_node() {
  local path="$1"
  cat >"${path}" <<'EOF'
PULSEDAG_PUBLIC_TESTNET_ROLE=node
PULSEDAG_CONFIG_PROFILE=testnet
PULSEDAG_NETWORK_PROFILE=public-testnet-v2.4.0-candidate
PULSEDAG_CHAIN_ID=pulsedag-public-testnet-v2.4.0-candidate
PULSEDAG_CONSENSUS_MODE=legacy
PULSEDAG_P2P_ENABLED=true
PULSEDAG_P2P_MODE=libp2p-real
PULSEDAG_P2P_LISTEN=/ip4/0.0.0.0/tcp/30333
PULSEDAG_P2P_BOOTSTRAP=/dns4/seed-1.example.net/tcp/30333/p2p/12D3KooWSeedOne111111111111111111111111111111111,/dns4/seed-2.example.net/tcp/30333/p2p/12D3KooWSeedTwo222222222222222222222222222222222
PULSEDAG_P2P_MDNS=false
PULSEDAG_P2P_KADEMLIA=false
PULSEDAG_P2P_IDENTITY_KEY=/var/lib/pulsedag/public-testnet/node-1/identity.key
PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/node-1.example.net/tcp/30333/p2p/12D3KooWNodeOne333333333333333333333333333333333
PULSEDAG_RPC_BIND=127.0.0.1:8080
PULSEDAG_API_PROFILE=public_safe
PULSEDAG_ADMIN_ENABLED=false
PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES=131072
PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE=30
PULSEDAG_RPC_RATE_LIMIT_PER_IP=true
PULSEDAG_RPC_CORS_ALLOWLIST=https://explorer.example.net
PULSEDAG_ROCKSDB_PATH=/var/lib/pulsedag/public-testnet/node-1/rocksdb
PULSEDAG_PERSIST_SNAPSHOT_ON_START=true
PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true
PULSEDAG_PUBLIC_TESTNET_READY=false
PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false
PULSEDAG_CONTRACTS_ENABLED=false
EOF
}

write_valid_seed() {
  local path="$1"
  write_valid_node "${path}"
  sed -i 's/PULSEDAG_PUBLIC_TESTNET_ROLE=node/PULSEDAG_PUBLIC_TESTNET_ROLE=seed/' "${path}"
  sed -i 's|PULSEDAG_P2P_BOOTSTRAP=.*|PULSEDAG_P2P_BOOTSTRAP=/dns4/seed-2.example.net/tcp/30333/p2p/12D3KooWSeedTwo222222222222222222222222222222222|' "${path}"
  sed -i 's|PULSEDAG_P2P_IDENTITY_KEY=.*|PULSEDAG_P2P_IDENTITY_KEY=/var/lib/pulsedag/public-testnet/seed-1/identity.key|' "${path}"
  sed -i 's|PULSEDAG_PUBLIC_P2P_MULTIADDR=.*|PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/seed-1.example.net/tcp/30333/p2p/12D3KooWSeedOne111111111111111111111111111111111|' "${path}"
  sed -i 's|PULSEDAG_ROCKSDB_PATH=.*|PULSEDAG_ROCKSDB_PATH=/var/lib/pulsedag/public-testnet/seed-1/rocksdb|' "${path}"
}

expect_pass() {
  local config="$1"
  bash "${PREFLIGHT}" "${config}" >/dev/null || fail "expected preflight PASS for ${config}"
}

expect_fail() {
  local needle="$1"
  local config="$2"
  local output
  if output="$(bash "${PREFLIGHT}" "${config}" 2>&1)"; then
    fail "expected preflight failure containing '${needle}'"
  fi
  grep -Fq "${needle}" <<<"${output}" || fail "failure did not contain '${needle}': ${output}"
}

NODE="${TMP_DIR}/node.env"
SEED="${TMP_DIR}/seed.env"
write_valid_node "${NODE}"
write_valid_seed "${SEED}"
expect_pass "${NODE}"
expect_pass "${SEED}"

case_file="${TMP_DIR}/private-operator.env"
cp "${NODE}" "${case_file}"
sed -i 's/PULSEDAG_API_PROFILE=public_safe/PULSEDAG_API_PROFILE=private_operator/' "${case_file}"
expect_fail "PULSEDAG_API_PROFILE must be 'public_safe'" "${case_file}"

case_file="${TMP_DIR}/admin.env"
cp "${NODE}" "${case_file}"
sed -i 's/PULSEDAG_ADMIN_ENABLED=false/PULSEDAG_ADMIN_ENABLED=true/' "${case_file}"
expect_fail "PULSEDAG_ADMIN_ENABLED must be 'false'" "${case_file}"

case_file="${TMP_DIR}/rate.env"
cp "${NODE}" "${case_file}"
sed -i 's/PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE=30/PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE=0/' "${case_file}"
expect_fail "PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE must be greater than zero" "${case_file}"

case_file="${TMP_DIR}/cors.env"
cp "${NODE}" "${case_file}"
sed -i 's|PULSEDAG_RPC_CORS_ALLOWLIST=https://explorer.example.net|PULSEDAG_RPC_CORS_ALLOWLIST=*|' "${case_file}"
expect_fail "must not contain wildcard origins" "${case_file}"

case_file="${TMP_DIR}/single-bootnode.env"
cp "${NODE}" "${case_file}"
sed -i 's|PULSEDAG_P2P_BOOTSTRAP=.*|PULSEDAG_P2P_BOOTSTRAP=/dns4/seed-1.example.net/tcp/30333/p2p/12D3KooWSeedOne111111111111111111111111111111111|' "${case_file}"
expect_fail "ordinary public nodes require at least two distinct bootnodes" "${case_file}"

case_file="${TMP_DIR}/kad.env"
cp "${NODE}" "${case_file}"
sed -i 's/PULSEDAG_P2P_KADEMLIA=false/PULSEDAG_P2P_KADEMLIA=true/' "${case_file}"
expect_fail "PULSEDAG_P2P_KADEMLIA must be 'false'" "${case_file}"

case_file="${TMP_DIR}/ephemeral-identity.env"
cp "${NODE}" "${case_file}"
sed -i 's|PULSEDAG_P2P_IDENTITY_KEY=.*|PULSEDAG_P2P_IDENTITY_KEY=/tmp/identity.key|' "${case_file}"
expect_fail "must not use an ephemeral temp/runtime path" "${case_file}"

case_file="${TMP_DIR}/private-chain.env"
cp "${NODE}" "${case_file}"
sed -i 's/pulsedag-public-testnet-v2.4.0-candidate/pulsedag-private-testnet-v2.4.0/' "${case_file}"
expect_fail "must identify the public testnet explicitly" "${case_file}"

case_file="${TMP_DIR}/launch-flag.env"
cp "${NODE}" "${case_file}"
sed -i 's/PULSEDAG_PUBLIC_TESTNET_READY=false/PULSEDAG_PUBLIC_TESTNET_READY=true/' "${case_file}"
expect_fail "PULSEDAG_PUBLIC_TESTNET_READY must be 'false'" "${case_file}"

case_file="${TMP_DIR}/placeholder.env"
cp "${NODE}" "${case_file}"
sed -i 's|PULSEDAG_PUBLIC_P2P_MULTIADDR=.*|PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/node.example.net/tcp/30333/p2p/REPLACE_WITH_PEER_ID|' "${case_file}"
expect_fail "still contains a REPLACE_* placeholder" "${case_file}"

printf 'PASS: v2.4.0 public-testnet preflight regression suite\n'
