#!/usr/bin/env bash
set -euo pipefail

RUNNER="scripts/private_testnet/netns_rehearsal.sh"
NETWORK="scripts/private_testnet/netns_network.sh"
NODES="scripts/private_testnet/netns_nodes.sh"
INVENTORY="scripts/private_testnet/netns_inventory.py"
FAULT="scripts/private_testnet/netns_fault.sh"
CONTROLLER="scripts/private_testnet/multi_host_rehearsal.py"

bash -n "$RUNNER"
bash -n "$NETWORK"
bash -n "$NODES"
bash -n "$FAULT"
python3 - "$INVENTORY" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
compile(path.read_text(encoding="utf-8"), str(path), "exec")
PY
bash "$RUNNER" --help >/dev/null

grep -Fq 'timeout --signal=TERM --kill-after=30s 35m' "$RUNNER"
grep -Fq 'NODE_NAMES=("seed-1" "node-1" "node-2" "node-3" "node-4")' "$NETWORK"
grep -Fq 'NAMESPACES=("pdg-s1" "pdg-n1" "pdg-n2" "pdg-n3" "pdg-n4")' "$NETWORK"
grep -Fq 'PULSEDAG_P2P_MDNS=false' "$NODES"
grep -Fq 'PULSEDAG_P2P_KADEMLIA=true' "$NODES"
grep -Fq 'PULSEDAG_RPC_BIND=127.0.0.1:$RPC_PORT' "$NODES"
grep -Fq 'PULSEDAG_PUBLIC_TESTNET_READY=false' "$NODES"
grep -Fq 'PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false' "$NODES"
grep -Fq '"transport": ["sudo", "-n", "ip", "netns", "exec", namespace]' "$INVENTORY"
grep -Fq '"target": "node-4"' "$INVENTORY"
grep -Fq 'ip link set dev "$INTERFACE" down' "$FAULT"
grep -Fq 'ip link set dev "$INTERFACE" up' "$FAULT"

destructive_pattern='iptables[[:space:]]+-F|nft[[:space:]]+flush|'
destructive_pattern+='ip[[:space:]]+netns[[:space:]]+delete[[:space:]]+all'
if grep -Eq "$destructive_pattern" "$RUNNER" "$NETWORK" "$NODES" "$FAULT"; then
  echo "destructive network cleanup detected" >&2
  exit 1
fi

python3 - "$NETWORK" <<'PY'
import re
import sys
from pathlib import Path

content = Path(sys.argv[1]).read_text(encoding="utf-8")
ips = re.search(r'NODE_IPS=\(([^)]*)\)', content)
if ips is None:
    raise SystemExit("NODE_IPS declaration missing")
values = re.findall(r'"([^"]+)"', ips.group(1))
if len(values) != 5 or len(set(values)) != 5:
    raise SystemExit("expected exactly five unique namespace IPs")
if values[0] != "10.230.0.10":
    raise SystemExit("seed IP changed unexpectedly")
PY

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
candidate_sha="$(printf 'a%.0s' {1..40})"
python3 "$INVENTORY" \
  --output "$temporary/inventory.json" \
  --candidate-sha "$candidate_sha" \
  --workspace /opt/pulsedag/source \
  --state-root /var/lib/pulsedag-task12-netns \
  --fault-hook /usr/local/sbin/pulsedag-task12-netns-fault
python3 "$CONTROLLER" validate-inventory --inventory "$temporary/inventory.json"

echo "v2.3.0 isolated namespace rehearsal contract: PASS"
