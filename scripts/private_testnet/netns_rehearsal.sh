#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage:
  bash scripts/private_testnet/netns_rehearsal.sh \
    --candidate-sha <40-char-sha> \
    --workspace <absolute-repository-path> \
    --evidence-root <absolute-output-path>
EOF
}

CANDIDATE_SHA=""
WORKSPACE=""
EVIDENCE_ROOT=""

while (($# > 0)); do
  case "$1" in
    --candidate-sha)
      CANDIDATE_SHA="${2:-}"
      shift 2
      ;;
    --workspace)
      WORKSPACE="${2:-}"
      shift 2
      ;;
    --evidence-root)
      EVIDENCE_ROOT="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ "$CANDIDATE_SHA" =~ ^[0-9a-f]{40}$ ]] || {
  echo "--candidate-sha must be 40 lowercase hexadecimal characters" >&2
  exit 2
}
[[ "$WORKSPACE" == /* && -d "$WORKSPACE/.git" ]] || {
  echo "--workspace must be an absolute Git checkout path" >&2
  exit 2
}
[[ "$EVIDENCE_ROOT" == /* ]] || {
  echo "--evidence-root must be absolute" >&2
  exit 2
}

WORKSPACE="$(realpath "$WORKSPACE")"
mkdir -p "$EVIDENCE_ROOT"
EVIDENCE_ROOT="$(realpath "$EVIDENCE_ROOT")"
ACTUAL_SHA="$(git -C "$WORKSPACE" rev-parse HEAD)"

[[ "$ACTUAL_SHA" == "$CANDIDATE_SHA" ]] || {
  printf 'candidate mismatch: expected=%s actual=%s\n' "$CANDIDATE_SHA" "$ACTUAL_SHA" >&2
  exit 1
}
[[ -z "$(git -C "$WORKSPACE" status --porcelain)" ]] || {
  echo "candidate checkout must be clean before provisioning" >&2
  exit 1
}

STATE_ROOT="/var/lib/pulsedag-task12-netns"
BRIDGE="pdgbr0"
P2P_PORT="32333"
RPC_PORT="8280"
RELEASE_ID="task12-${CANDIDATE_SHA:0:12}"
NODE_BINARY="$WORKSPACE/target/release/pulsedagd"
MINER_BINARY="$WORKSPACE/target/release/pulsedag-miner"
CONTROLLER="$WORKSPACE/scripts/private_testnet/multi_host_rehearsal.py"
LIFECYCLE="$WORKSPACE/scripts/private_testnet/node_lifecycle.py"
PREFLIGHT="$WORKSPACE/scripts/v2_3_0_private_testnet_preflight.sh"
NETWORK_LIBRARY="$WORKSPACE/scripts/private_testnet/netns_network.sh"
NODE_LIBRARY="$WORKSPACE/scripts/private_testnet/netns_nodes.sh"
INVENTORY_WRITER="$WORKSPACE/scripts/private_testnet/netns_inventory.py"
FAULT_SOURCE="$WORKSPACE/scripts/private_testnet/netns_fault.sh"
FAULT_HOOK="$STATE_ROOT/bin/netns_fault.sh"
INVENTORY="$EVIDENCE_ROOT/inventory.json"
CONTROLLER_EVIDENCE="$EVIDENCE_ROOT/controller"
PROVISION_LOG="$EVIDENCE_ROOT/provisioning.log"
MINER_LOG="$EVIDENCE_ROOT/external-miner.log"
NETWORK_LOG="$EVIDENCE_ROOT/network-state.log"

for command in git realpath sudo ip ping ss python3 timeout tee install; do
  command -v "$command" >/dev/null
 done
for required in \
  "$NODE_BINARY" \
  "$MINER_BINARY" \
  "$CONTROLLER" \
  "$LIFECYCLE" \
  "$PREFLIGHT" \
  "$NETWORK_LIBRARY" \
  "$NODE_LIBRARY" \
  "$INVENTORY_WRITER" \
  "$FAULT_SOURCE"; do
  test -f "$required"
done
test -x "$NODE_BINARY"
test -x "$MINER_BINARY"

# These sourced files define only fixed Task 12 constants and functions.
source "$NETWORK_LIBRARY"
source "$NODE_LIBRARY"

exec > >(tee -a "$PROVISION_LOG") 2>&1
MINER_LOOP_PID=""

cleanup() {
  local original_rc=$?
  trap - EXIT INT TERM
  set +e
  collect_network_evidence
  collect_node_evidence
  if [[ -n "$MINER_LOOP_PID" ]]; then
    kill "$MINER_LOOP_PID" 2>/dev/null || true
    wait "$MINER_LOOP_PID" 2>/dev/null || true
  fi
  cleanup_topology
  exit "$original_rc"
}
trap cleanup EXIT INT TERM

external_miner_loop() {
  while true; do
    sudo -n ip netns exec "${NAMESPACES[0]}" \
      "$MINER_BINARY" \
        --node "http://127.0.0.1:$RPC_PORT" \
        --miner-address task12-netns-external \
        --max-tries 100000 >> "$MINER_LOG" 2>&1 || true
    sleep 3
  done
}

echo "Preparing isolated Task 12 topology for candidate $CANDIDATE_SHA"
remove_previous_topology
if sudo -n test -L "$STATE_ROOT"; then
  echo "refusing to remove symlinked Task 12 state root: $STATE_ROOT" >&2
  exit 1
fi
sudo -n rm -rf --one-file-system "$STATE_ROOT"
sudo -n install -d -m 0750 \
  "$STATE_ROOT" \
  "$STATE_ROOT/bin" \
  "$STATE_ROOT/nodes" \
  "$STATE_ROOT/fault"
sudo -n git config --system --add safe.directory "$WORKSPACE"
sudo -n install -m 0755 "$FAULT_SOURCE" "$FAULT_HOOK"
create_topology

for index in "${!NODE_NAMES[@]}"; do
  write_node_environment "$index"
  install_node_release "$index"
done

python3 "$INVENTORY_WRITER" \
  --output "$INVENTORY" \
  --candidate-sha "$CANDIDATE_SHA" \
  --workspace "$WORKSPACE" \
  --state-root "$STATE_ROOT" \
  --fault-hook "$FAULT_HOOK"
python3 "$CONTROLLER" validate-inventory --inventory "$INVENTORY"

external_miner_loop &
MINER_LOOP_PID=$!

set +e
timeout --signal=TERM --kill-after=30s 35m \
  python3 "$CONTROLLER" \
    run \
    --inventory "$INVENTORY" \
    --out-dir "$CONTROLLER_EVIDENCE"
controller_rc=$?
set -e

if [[ "$controller_rc" -eq 0 ]]; then
  python3 "$CONTROLLER" verify-evidence --evidence-dir "$CONTROLLER_EVIDENCE"
fi

python3 - "$EVIDENCE_ROOT/run-summary.json" "$CANDIDATE_SHA" "$controller_rc" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

path = Path(sys.argv[1])
candidate_sha = sys.argv[2]
controller_rc = int(sys.argv[3])
payload = {
    "gate": "v2.3.0-isolated-netns-live-rehearsal",
    "candidate_sha": candidate_sha,
    "controller_exit_code": controller_rc,
    "result": "PASS" if controller_rc == 0 else "FAIL",
    "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "public_testnet_ready": False,
    "thirty_day_public_testnet_clock_started": False,
}
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

exit "$controller_rc"
