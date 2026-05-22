#!/usr/bin/env bash
set -euo pipefail

TOPOLOGY_FILE=${1:-}
[[ -n "$TOPOLOGY_FILE" ]] || { echo "usage: $0 <topology.json>"; exit 1; }
[[ -f "$TOPOLOGY_FILE" ]] || { echo "missing topology file: $TOPOLOGY_FILE"; exit 1; }

jq -e '.manifest_version == "v2.2.19" and (.hosts|length)>=1 and (.nodes|length)>=1 and (.miners|length)>=1' "$TOPOLOGY_FILE" >/dev/null

DUP_NODES=$(jq -r '.nodes[].id' "$TOPOLOGY_FILE" | sort | uniq -d)
[[ -z "$DUP_NODES" ]] || { echo "duplicate node ids: $DUP_NODES"; exit 1; }

for nid in $(jq -r '.nodes[].id' "$TOPOLOGY_FILE"); do
  jq -e --arg id "$nid" '.nodes[] | select(.id==$id) | .host_id and .rpc_port and .p2p_port' "$TOPOLOGY_FILE" >/dev/null
done

echo "topology validation passed: $TOPOLOGY_FILE"
