#!/usr/bin/env bash
set -euo pipefail

TOPOLOGY_FILE=${1:-configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json}
OUT_DIR=${2:-artifacts/private-testnet/v2_2_19/multi-host-env}
mkdir -p "$OUT_DIR"

jq -c '.nodes[]' "$TOPOLOGY_FILE" | while read -r node; do
  node_id=$(jq -r '.id' <<<"$node")
  host_id=$(jq -r '.host_id' <<<"$node")
  rpc_port=$(jq -r '.rpc_port' <<<"$node")
  p2p_port=$(jq -r '.p2p_port' <<<"$node")
  cat > "$OUT_DIR/${node_id}.env" <<ENV
NODE_ID=${node_id}
HOST_ID=${host_id}
RPC_BIND=127.0.0.1:${rpc_port}
P2P_BIND=/ip4/0.0.0.0/tcp/${p2p_port}
DATA_DIR=/home/pulsedag/data/${node_id}
ENV
  echo "rendered $OUT_DIR/${node_id}.env"
done
