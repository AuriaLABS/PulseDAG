#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${1:-}"
if [[ -z "$ENV_FILE" || ! -f "$ENV_FILE" ]]; then
  echo "usage: bash scripts/v2_3_0_private_testnet_preflight.sh <env-file>" >&2
  exit 2
fi

fail=0
checks=0
passes=0

check() {
  local message="$1"
  shift
  checks=$((checks + 1))
  if "$@"; then
    passes=$((passes + 1))
    printf 'PASS: %s\n' "$message"
  else
    fail=1
    printf 'FAIL: %s\n' "$message" >&2
  fi
}

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

load_env_file() {
  local raw line key value line_number=0
  while IFS= read -r raw || [[ -n "$raw" ]]; do
    line_number=$((line_number + 1))
    line="$(trim "$raw")"
    [[ -z "$line" || "$line" == \#* ]] && continue
    if [[ "$line" == export\ * ]]; then
      line="$(trim "${line#export }")"
    fi
    if [[ "$line" != *=* ]]; then
      printf 'invalid environment line %d: expected KEY=VALUE\n' "$line_number" >&2
      return 2
    fi

    key="$(trim "${line%%=*}")"
    value="$(trim "${line#*=}")"
    if [[ ! "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
      printf 'invalid environment key on line %d: %s\n' "$line_number" "$key" >&2
      return 2
    fi

    if [[ "$value" == \"* || "$value" == \'* ]]; then
      if (( ${#value} < 2 )) || [[ "${value: -1}" != "${value:0:1}" ]]; then
        printf 'mismatched quotes on environment line %d\n' "$line_number" >&2
        return 2
      fi
      value="${value:1:${#value}-2}"
    fi

    if [[ "$value" == *'$('* || "$value" == *'${'* || "$value" == *'`'* ]]; then
      printf 'shell expansion is not allowed on environment line %d\n' "$line_number" >&2
      return 2
    fi

    printf -v "$key" '%s' "$value"
    export "$key"
  done < "$ENV_FILE"
}

require_nonempty() {
  local name="$1"
  [[ -n "${!name:-}" ]]
}

is_true() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

is_false() {
  case "${1,,}" in
    0|false|no|off) return 0 ;;
    *) return 1 ;;
  esac
}

is_absolute_persistent_path() {
  local value="$1"
  [[ "$value" == /* ]] && [[ "$value" != /tmp/* ]] && [[ "$value" != /run/* ]]
}

is_tcp_multiaddr() {
  local value="$1"
  [[ "$value" =~ ^/(ip4|ip6|dns4|dns6)/[^/]+/tcp/[0-9]+$ ]]
}

is_bootnode_multiaddr() {
  local value="$1"
  [[ "$value" =~ ^/(ip4|ip6|dns4|dns6)/[^/]+/tcp/[0-9]+/p2p/[^/]+$ ]]
}

without_peer_id() {
  local value="$1"
  printf '%s' "${value%/p2p/*}"
}

is_loopback_rpc_bind() {
  local value="$1"
  [[ "$value" =~ ^(127\.0\.0\.1|\[::1\]|localhost):[0-9]+$ ]]
}

load_env_file

role="${PULSEDAG_PRIVATE_TESTNET_ROLE:-}"
bootstrap="${PULSEDAG_P2P_BOOTSTRAP:-}"
public_multiaddr="${PULSEDAG_PUBLIC_P2P_MULTIADDR:-}"
admin_enabled="${PULSEDAG_ADMIN_ENABLED:-false}"
operator_token="${PULSEDAG_OPERATOR_AUTH_TOKEN:-}"

check "role is seed or node" bash -c '[[ "$1" == seed || "$1" == node ]]' _ "$role"
check "config profile is private" test "${PULSEDAG_CONFIG_PROFILE:-}" = "private"
check "network profile is private-testnet-v2.3.0" test "${PULSEDAG_NETWORK_PROFILE:-}" = "private-testnet-v2.3.0"
check "chain id is pulsedag-private-v2.3.0" test "${PULSEDAG_CHAIN_ID:-}" = "pulsedag-private-v2.3.0"
check "consensus mode remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"
check "real P2P is enabled" is_true "${PULSEDAG_P2P_ENABLED:-false}"
check "P2P mode is libp2p-real" test "${PULSEDAG_P2P_MODE:-}" = "libp2p-real"
check "mDNS is disabled for multi-host operation" is_false "${PULSEDAG_P2P_MDNS:-true}"
check "Kademlia is enabled" is_true "${PULSEDAG_P2P_KADEMLIA:-false}"
check "P2P listen address is a TCP multiaddr" is_tcp_multiaddr "${PULSEDAG_P2P_LISTEN:-}"
check "public P2P address is a TCP multiaddr" is_tcp_multiaddr "$public_multiaddr"
check "identity key path is present" require_nonempty PULSEDAG_P2P_IDENTITY_KEY
check "identity key path is absolute and persistent" is_absolute_persistent_path "${PULSEDAG_P2P_IDENTITY_KEY:-}"
check "RocksDB path is present" require_nonempty PULSEDAG_ROCKSDB_PATH
check "RocksDB path is absolute and persistent" is_absolute_persistent_path "${PULSEDAG_ROCKSDB_PATH:-}"
check "identity and RocksDB paths differ" test "${PULSEDAG_P2P_IDENTITY_KEY:-}" != "${PULSEDAG_ROCKSDB_PATH:-}"
check "RPC remains loopback-only in Task 07" is_loopback_rpc_bind "${PULSEDAG_RPC_BIND:-}"
check "API profile is private_operator" test "${PULSEDAG_API_PROFILE:-}" = "private_operator"
check "RPC rate limiting is enabled" bash -c '[[ "$1" =~ ^[0-9]+$ ]] && (( 10#$1 > 0 ))' _ "${PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE:-0}"
check "RPC limiting is per IP" is_true "${PULSEDAG_RPC_RATE_LIMIT_PER_IP:-false}"
check "snapshot-gated pruning is enabled" is_true "${PULSEDAG_PRUNE_REQUIRE_SNAPSHOT:-false}"

if [[ "$role" == "seed" ]]; then
  check "seed may start without bootnodes" true
else
  check "ordinary node has at least one bootnode" test -n "$bootstrap"
  IFS=',' read -r -a bootnodes <<< "$bootstrap"
  for bootnode in "${bootnodes[@]}"; do
    bootnode="${bootnode//[[:space:]]/}"
    check "bootnode includes a libp2p peer id: $bootnode" is_bootnode_multiaddr "$bootnode"
    check "node does not bootstrap to itself: $bootnode" \
      test "$(without_peer_id "$bootnode")" != "$(without_peer_id "$public_multiaddr")"
  done
fi

if is_true "$admin_enabled"; then
  check "admin token is at least 16 characters" bash -c '(( ${#1} >= 16 ))' _ "$operator_token"
else
  check "admin endpoints remain disabled" true
fi

for forbidden in PULSEDAG_PUBLIC_TESTNET_READY PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED; do
  value="${!forbidden:-false}"
  check "$forbidden is not true" is_false "$value"
done

result="PASS"
if (( fail != 0 )); then
  result="FAIL"
fi

if [[ -n "${OUT_DIR:-}" ]]; then
  mkdir -p "$OUT_DIR"
  cat > "$OUT_DIR/private-testnet-preflight.json" <<JSON
{
  "gate": "v2.3.0-private-testnet-bootstrap-preflight",
  "role": "${role}",
  "network_profile": "${PULSEDAG_NETWORK_PROFILE:-}",
  "chain_id": "${PULSEDAG_CHAIN_ID:-}",
  "checks": ${checks},
  "passes": ${passes},
  "result": "${result}",
  "public_testnet_ready": false,
  "thirty_day_public_testnet_clock_started": false
}
JSON
fi

printf 'SUMMARY: %s (%d/%d checks passed)\n' "$result" "$passes" "$checks"
exit "$fail"
