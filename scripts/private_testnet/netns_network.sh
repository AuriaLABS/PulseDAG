# Fixed Linux network topology and cleanup for the Task 12 rehearsal.

NODE_NAMES=("seed-1" "node-1" "node-2" "node-3" "node-4")
NAMESPACES=("pdg-s1" "pdg-n1" "pdg-n2" "pdg-n3" "pdg-n4")
NODE_IPS=("10.230.0.10" "10.230.0.11" "10.230.0.12" "10.230.0.13" "10.230.0.14")
HOST_VETHS=("pds1h" "pdn1h" "pdn2h" "pdn3h" "pdn4h")

kill_namespace_processes() {
  local namespace="$1"
  local pids=""
  pids="$(sudo -n ip netns pids "$namespace" 2>/dev/null || true)"
  if [[ -n "$pids" ]]; then
    # These namespaces are created exclusively for this rehearsal.
    sudo -n kill -TERM $pids 2>/dev/null || true
    sleep 1
    pids="$(sudo -n ip netns pids "$namespace" 2>/dev/null || true)"
    [[ -z "$pids" ]] || sudo -n kill -KILL $pids 2>/dev/null || true
  fi
}

remove_previous_topology() {
  local namespace
  for namespace in "${NAMESPACES[@]}"; do
    if sudo -n ip netns list | awk '{print $1}' | grep -Fxq "$namespace"; then
      kill_namespace_processes "$namespace"
      sudo -n ip netns delete "$namespace"
    fi
  done
  sudo -n ip link delete "$BRIDGE" type bridge 2>/dev/null || true
}

create_topology() {
  sudo -n ip link add name "$BRIDGE" type bridge
  sudo -n ip link set dev "$BRIDGE" up

  local index namespace host_veth peer_veth ip_address
  for index in "${!NAMESPACES[@]}"; do
    namespace="${NAMESPACES[$index]}"
    host_veth="${HOST_VETHS[$index]}"
    peer_veth="pdpeer$index"
    ip_address="${NODE_IPS[$index]}"

    sudo -n ip netns add "$namespace"
    sudo -n ip link add "$host_veth" type veth peer name "$peer_veth"
    sudo -n ip link set "$host_veth" master "$BRIDGE"
    sudo -n ip link set "$host_veth" up
    sudo -n ip link set "$peer_veth" netns "$namespace"
    sudo -n ip netns exec "$namespace" ip link set lo up
    sudo -n ip netns exec "$namespace" ip link set "$peer_veth" name eth0
    sudo -n ip netns exec "$namespace" ip address add "$ip_address/24" dev eth0
    sudo -n ip netns exec "$namespace" ip link set eth0 up
  done

  for index in "${!NAMESPACES[@]}"; do
    sudo -n ip netns exec "${NAMESPACES[$index]}" \
      ping -c 1 -W 2 "${NODE_IPS[0]}" >/dev/null
  done
}

collect_network_evidence() {
  {
    echo "candidate_sha=$CANDIDATE_SHA"
    echo "captured_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    sudo -n ip -details link show "$BRIDGE" 2>/dev/null || true
    for namespace in "${NAMESPACES[@]}"; do
      echo "===== namespace: $namespace ====="
      sudo -n ip netns exec "$namespace" ip -brief address 2>/dev/null || true
      sudo -n ip netns exec "$namespace" ip route show 2>/dev/null || true
      sudo -n ip netns exec "$namespace" ss -lntp 2>/dev/null || true
    done
  } >> "$NETWORK_LOG"
}

cleanup_topology() {
  local namespace
  for namespace in "${NAMESPACES[@]}"; do
    if sudo -n ip netns list | awk '{print $1}' | grep -Fxq "$namespace"; then
      kill_namespace_processes "$namespace"
      sudo -n ip netns delete "$namespace" || true
    fi
  done
  sudo -n ip link delete "$BRIDGE" type bridge 2>/dev/null || true
}
