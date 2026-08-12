#!/usr/bin/env bash
set -euo pipefail

CONFIG_FILE="${1:-}"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

pass() {
  printf 'PASS: %s\n' "$*"
}

usage() {
  cat <<'EOF'
Usage: scripts/v2_4_0_public_testnet_preflight.sh <rendered-env-file>

Validates the pre-GO v2.4.0 public-testnet deployment contract. It does not
start a node and it does not authorize public-testnet launch or Day 0.
EOF
}

[[ -n "${CONFIG_FILE}" ]] || { usage >&2; exit 2; }
[[ -f "${CONFIG_FILE}" ]] || fail "configuration file not found: ${CONFIG_FILE}"

get_value() {
  local key="$1"
  local line
  line="$(grep -E "^[[:space:]]*${key}=" "${CONFIG_FILE}" | tail -n 1 || true)"
  [[ -n "${line}" ]] || fail "missing required setting ${key}"
  line="${line#*=}"
  line="${line%$'\r'}"
  printf '%s' "${line}"
}

require_exact() {
  local key="$1"
  local expected="$2"
  local actual
  actual="$(get_value "${key}")"
  [[ "${actual}" == "${expected}" ]] || fail "${key} must be '${expected}' (got '${actual}')"
}

require_false() {
  require_exact "$1" "false"
}

require_true() {
  require_exact "$1" "true"
}

require_not_placeholder() {
  local key="$1"
  local value
  value="$(get_value "${key}")"
  [[ -n "${value}" ]] || fail "${key} must not be empty"
  [[ "${value}" != *REPLACE_* ]] || fail "${key} still contains a REPLACE_* placeholder"
}

require_public_identity() {
  local key="$1"
  local value
  value="$(get_value "${key}")"
  [[ -n "${value}" ]] || fail "${key} must not be empty"
  [[ "${value}" != *REPLACE_* ]] || fail "${key} still contains a REPLACE_* placeholder"
  [[ "${value,,}" == *public-testnet* ]] || fail "${key} must identify the public testnet explicitly"
  [[ "${value,,}" != *private-testnet* ]] || fail "${key} must not reuse a private-testnet identity"
}

require_persistent_path() {
  local key="$1"
  local value normalized
  value="$(get_value "${key}")"
  [[ -n "${value}" ]] || fail "${key} must not be empty"
  normalized="${value//\\//}"
  if [[ ! "${normalized}" =~ ^/ && ! "${normalized}" =~ ^[A-Za-z]:/ ]]; then
    fail "${key} must be an absolute persistent path"
  fi
  case "${normalized,,}" in
    /tmp|/tmp/*|/run|/run/*|*/windows/temp|*/windows/temp/*|*/appdata/local/temp|*/appdata/local/temp/*)
      fail "${key} must not use an ephemeral temp/runtime path"
      ;;
  esac
}

require_loopback_rpc() {
  local bind
  bind="$(get_value PULSEDAG_RPC_BIND)"
  case "${bind}" in
    127.0.0.1:[0-9]*|\[::1\]:[0-9]*) ;;
    *) fail "PULSEDAG_RPC_BIND must be a numeric loopback listener behind the managed public proxy" ;;
  esac
}

require_positive_integer_at_most() {
  local key="$1"
  local max="$2"
  local value
  value="$(get_value "${key}")"
  [[ "${value}" =~ ^[0-9]+$ ]] || fail "${key} must be an integer"
  (( value > 0 )) || fail "${key} must be greater than zero"
  (( value <= max )) || fail "${key} must be <= ${max}"
}

validate_multiaddr() {
  local label="$1"
  local value="$2"
  [[ -n "${value}" ]] || fail "${label} must not be empty"
  [[ "${value}" != *REPLACE_* ]] || fail "${label} still contains a REPLACE_* placeholder"
  [[ "${value}" == */p2p/* ]] || fail "${label} must include a /p2p/<peer-id> component"
  [[ "${value}" == /ip4/* || "${value}" == /ip6/* || "${value}" == /dns4/* || "${value}" == /dns6/* ]] \
    || fail "${label} must use an ip4/ip6/dns4/dns6 multiaddr"
}

validate_bootnodes() {
  local role="$1"
  local raw entry trimmed
  local -a entries=()
  declare -A seen=()

  raw="$(get_value PULSEDAG_P2P_BOOTSTRAP)"
  IFS=',' read -r -a entries <<< "${raw}"

  local count=0
  for entry in "${entries[@]}"; do
    trimmed="${entry#"${entry%%[![:space:]]*}"}"
    trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
    [[ -n "${trimmed}" ]] || continue
    validate_multiaddr "PULSEDAG_P2P_BOOTSTRAP entry" "${trimmed}"
    [[ -z "${seen[${trimmed}]:-}" ]] || fail "PULSEDAG_P2P_BOOTSTRAP contains a duplicate bootnode"
    seen["${trimmed}"]=1
    count=$((count + 1))
  done

  if [[ "${role}" == "node" ]]; then
    (( count >= 2 )) || fail "ordinary public nodes require at least two distinct bootnodes"
  else
    (( count >= 1 )) || fail "public seed nodes require at least one peer seed/bootstrap for redundancy"
  fi
}

role="$(get_value PULSEDAG_PUBLIC_TESTNET_ROLE)"
[[ "${role}" == "seed" || "${role}" == "node" ]] || fail "PULSEDAG_PUBLIC_TESTNET_ROLE must be seed or node"

require_exact PULSEDAG_CONFIG_PROFILE testnet
require_public_identity PULSEDAG_NETWORK_PROFILE
require_public_identity PULSEDAG_CHAIN_ID
require_exact PULSEDAG_CONSENSUS_MODE legacy

require_true PULSEDAG_P2P_ENABLED
require_exact PULSEDAG_P2P_MODE libp2p-real
require_false PULSEDAG_P2P_MDNS
# Current v2.4 runtime public deployment contract uses explicit bootnodes. Do not
# advertise discovery capabilities that are not part of the accepted public surface.
require_false PULSEDAG_P2P_KADEMLIA
require_persistent_path PULSEDAG_P2P_IDENTITY_KEY
require_not_placeholder PULSEDAG_PUBLIC_P2P_MULTIADDR
validate_multiaddr PULSEDAG_PUBLIC_P2P_MULTIADDR "$(get_value PULSEDAG_PUBLIC_P2P_MULTIADDR)"
validate_bootnodes "${role}"

require_loopback_rpc
require_exact PULSEDAG_API_PROFILE public_safe
require_false PULSEDAG_ADMIN_ENABLED
require_positive_integer_at_most PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES 131072
require_positive_integer_at_most PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE 120
require_true PULSEDAG_RPC_RATE_LIMIT_PER_IP
cors="$(get_value PULSEDAG_RPC_CORS_ALLOWLIST)"
[[ "${cors}" != *'*'* ]] || fail "PULSEDAG_RPC_CORS_ALLOWLIST must not contain wildcard origins"

require_persistent_path PULSEDAG_ROCKSDB_PATH
require_true PULSEDAG_PERSIST_SNAPSHOT_ON_START
require_true PULSEDAG_PRUNE_REQUIRE_SNAPSHOT
if [[ "${role}" == "seed" ]]; then
  # Until v2.4 gains a reviewed checkpoint/state-sync bootstrap path, public bootnodes
  # must retain complete history so a fresh node always has a viable synchronization peer.
  require_false PULSEDAG_AUTO_PRUNE_ENABLED
else
  require_true PULSEDAG_AUTO_PRUNE_ENABLED
fi

# This script validates a candidate/rehearsal configuration before launch authorization.
require_false PULSEDAG_PUBLIC_TESTNET_READY
require_false PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED
require_false PULSEDAG_CONTRACTS_ENABLED

pass "v2.4.0 public-testnet ${role} candidate profile is fail-closed"
printf 'role=%s\n' "${role}"
printf 'network_profile=%s\n' "$(get_value PULSEDAG_NETWORK_PROFILE)"
printf 'chain_id=%s\n' "$(get_value PULSEDAG_CHAIN_ID)"
printf 'rpc_profile=%s\n' "$(get_value PULSEDAG_API_PROFILE)"
printf 'rpc_bind=%s\n' "$(get_value PULSEDAG_RPC_BIND)"
printf 'p2p_identity_path=%s\n' "$(get_value PULSEDAG_P2P_IDENTITY_KEY)"
printf 'rocksdb_path=%s\n' "$(get_value PULSEDAG_ROCKSDB_PATH)"
