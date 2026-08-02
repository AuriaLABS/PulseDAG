#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${1:-}"
if [[ -z "$ENV_FILE" || ! -f "$ENV_FILE" ]]; then
  echo "usage: bash scripts/v2_4_0_single_node_preflight.sh <env-file>" >&2
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

clear_pulsedag_environment() {
  local key
  for key in ${!PULSEDAG_@}; do
    unset "$key"
  done
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
    if [[ ! "$key" =~ ^PULSEDAG_[A-Z0-9_]+$ ]]; then
      printf 'invalid environment key on line %d: only PULSEDAG_* keys are allowed: %s\n' \
        "$line_number" "$key" >&2
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

is_true() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

is_false() {
  case "${1,,}" in
    0|false|no|off|'') return 0 ;;
    *) return 1 ;;
  esac
}

is_loopback_rpc_bind() {
  local value="$1"
  [[ "$value" =~ ^127\.0\.0\.1:[0-9]+$ || "$value" =~ ^\[::1\]:[0-9]+$ ]]
}

is_absolute_persistent_path() {
  local value="$1"
  [[ "$value" == /* ]] &&
    [[ "$value" != /tmp ]] &&
    [[ "$value" != /tmp/* ]] &&
    [[ "$value" != /run ]] &&
    [[ "$value" != /run/* ]]
}

is_supported_api_profile() {
  local value="$1"
  [[ "$value" == "private_operator" || "$value" == "local_dev" ]]
}

has_minimum_length() {
  local value="$1"
  local minimum="$2"
  (( ${#value} >= minimum ))
}

clear_pulsedag_environment
load_env_file

single_node_mode="${PULSEDAG_SINGLE_NODE_MODE:-false}"
role="${PULSEDAG_PRIVATE_TESTNET_ROLE:-}"
bootstrap="${PULSEDAG_P2P_BOOTSTRAP:-}"
public_multiaddr="${PULSEDAG_PUBLIC_P2P_MULTIADDR:-}"
admin_enabled="${PULSEDAG_ADMIN_ENABLED:-false}"
operator_token="${PULSEDAG_OPERATOR_AUTH_TOKEN:-}"

check "single-node mode is explicitly enabled" is_true "$single_node_mode"
check "operator role is explicitly single" test "$role" = "single"
check "config profile remains private" test "${PULSEDAG_CONFIG_PROFILE:-}" = "private"
check "network profile is private-testnet-v2.3.0" test "${PULSEDAG_NETWORK_PROFILE:-}" = "private-testnet-v2.3.0"
check "chain id is pulsedag-private-v2.3.0" test "${PULSEDAG_CHAIN_ID:-}" = "pulsedag-private-v2.3.0"
check "consensus mode remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"
check "P2P is disabled by policy" is_false "${PULSEDAG_P2P_ENABLED:-true}"
check "bootnodes are empty" test -z "$bootstrap"
check "public P2P advertisement is empty" test -z "$public_multiaddr"
check "RPC is loopback-only" is_loopback_rpc_bind "${PULSEDAG_RPC_BIND:-}"
check "API profile is private_operator or local_dev" is_supported_api_profile "${PULSEDAG_API_PROFILE:-}"
check "RocksDB path is present" test -n "${PULSEDAG_ROCKSDB_PATH:-}"
check "RocksDB path is absolute and persistent" is_absolute_persistent_path "${PULSEDAG_ROCKSDB_PATH:-}"
check "snapshot-gated pruning is enabled" is_true "${PULSEDAG_PRUNE_REQUIRE_SNAPSHOT:-false}"
check "smart contracts remain disabled" is_false "${PULSEDAG_CONTRACTS_ENABLED:-false}"

case "${PULSEDAG_CONFIG_PROFILE:-}" in
  rehearsal-a|rehearsal-b|rehearsal-c)
    check "single-node mode is not combined with a rehearsal profile" false
    ;;
  *)
    check "single-node mode is not combined with a rehearsal profile" true
    ;;
esac

if is_true "$admin_enabled"; then
  check "admin token is at least 16 characters" has_minimum_length "$operator_token" 16
else
  check "admin endpoints remain disabled" true
fi

for forbidden in \
  PULSEDAG_PUBLIC_TESTNET_READY \
  PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED \
  PULSEDAG_MULTI_HOST_REHEARSAL; do
  value="${!forbidden:-false}"
  check "$forbidden is not true" is_false "$value"
done

result="PASS"
if (( fail != 0 )); then
  result="FAIL"
fi

if [[ -n "${OUT_DIR:-}" ]]; then
  mkdir -p "$OUT_DIR"
  cat > "$OUT_DIR/single-node-preflight.json" <<JSON
{
  "gate": "v2.4.0-single-node-operator-preflight",
  "operator_mode": "single-node",
  "role": "${role}",
  "network_profile": "${PULSEDAG_NETWORK_PROFILE:-}",
  "chain_id": "${PULSEDAG_CHAIN_ID:-}",
  "p2p_enabled": false,
  "connected_peers_expected": false,
  "rpc_bind": "${PULSEDAG_RPC_BIND:-}",
  "isolated_mining_authorized": true,
  "checks": ${checks},
  "passes": ${passes},
  "result": "${result}",
  "public_testnet_ready": false,
  "thirty_day_public_testnet_clock_started": false,
  "contracts_enabled": false
}
JSON
fi

printf 'SUMMARY: %s (%d/%d checks passed)\n' "$result" "$passes" "$checks"
exit "$fail"
