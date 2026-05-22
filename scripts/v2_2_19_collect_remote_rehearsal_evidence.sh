#!/usr/bin/env bash
set -euo pipefail

TOPOLOGY_FILE=${1:-configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
OUT_DIR=${OUT_DIR:-artifacts/private-testnet/v2_2_19/remote-evidence/${RUN_ID}}
mkdir -p "$OUT_DIR"

jq -c '.nodes[]' "$TOPOLOGY_FILE" | while read -r node; do
  node_id=$(jq -r '.id' <<<"$node")
  host_id=$(jq -r '.host_id' <<<"$node")
  rpc_port=$(jq -r '.rpc_port' <<<"$node")
  ssh_host=$(jq -r --arg hid "$host_id" '.hosts[] | select(.host_id==$hid) | .ssh_host' "$TOPOLOGY_FILE")
  ssh_user=$(jq -r --arg hid "$host_id" '.hosts[] | select(.host_id==$hid) | .ssh_user' "$TOPOLOGY_FILE")

  mkdir -p "$OUT_DIR/$node_id"
  ssh "${ssh_user}@${ssh_host}" "curl -fsS http://127.0.0.1:${rpc_port}/status" > "$OUT_DIR/$node_id/status.json"
  ssh "${ssh_user}@${ssh_host}" "curl -fsS http://127.0.0.1:${rpc_port}/p2p/status" > "$OUT_DIR/$node_id/p2p-status.json" || true
  ssh "${ssh_user}@${ssh_host}" "journalctl -u pulsedagd -n 200 --no-pager" > "$OUT_DIR/$node_id/pulsedagd.log"
  ssh "${ssh_user}@${ssh_host}" "journalctl -u pulsedag-miner -n 200 --no-pager" > "$OUT_DIR/$node_id/pulsedag-miner.log" || true
  echo "collected evidence for ${node_id} from ${ssh_host}"
done

echo "evidence directory: $OUT_DIR"
