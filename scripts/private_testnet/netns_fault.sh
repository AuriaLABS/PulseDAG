#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"
INTERFACE="eth0"
P2P_PORT="32333"
STATE_DIR="/var/lib/pulsedag-task12-netns/fault"
LOG_FILE="$STATE_DIR/node-4.log"

mkdir -p "$STATE_DIR"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

p2p_established_count() {
  ss -Htn state established \
    "( sport = :$P2P_PORT or dport = :$P2P_PORT )" \
    | wc -l \
    | tr -d '[:space:]'
}

case "$ACTION" in
  isolate)
    ip link show dev "$INTERFACE" >/dev/null
    connections_before="$(p2p_established_count)"
    ip link set dev "$INTERFACE" down
    ip link show dev "$INTERFACE" | grep -q "state DOWN"

    # Link-down stops traffic but Linux may retain established TCP state for
    # minutes. Destroy only PulseDAG P2P sockets so the runtime observes the
    # intentional partition immediately while loopback RPC remains available.
    ss -Ktn state established \
      "( sport = :$P2P_PORT or dport = :$P2P_PORT )" >/dev/null 2>&1 || true

    deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
      connections_after="$(p2p_established_count)"
      if [[ "$connections_after" == "0" ]]; then
        printf '%s action=isolate interface=%s p2p_port=%s connections_before=%s connections_after=0 netns=%s result=PASS\n' \
          "$timestamp" \
          "$INTERFACE" \
          "$P2P_PORT" \
          "$connections_before" \
          "$(readlink /proc/self/ns/net)" >> "$LOG_FILE"
        exit 0
      fi
      sleep 1
    done

    connections_after="$(p2p_established_count)"
    printf '%s action=isolate interface=%s p2p_port=%s connections_before=%s connections_after=%s netns=%s result=FAIL\n' \
      "$timestamp" \
      "$INTERFACE" \
      "$P2P_PORT" \
      "$connections_before" \
      "$connections_after" \
      "$(readlink /proc/self/ns/net)" >> "$LOG_FILE"
    echo "failed to destroy isolated PulseDAG P2P sockets" >&2
    exit 1
    ;;
  restore)
    ip link show dev "$INTERFACE" >/dev/null
    ip link set dev "$INTERFACE" up
    ip link show dev "$INTERFACE" | grep -q "state UP"
    printf '%s action=restore interface=%s p2p_port=%s netns=%s result=PASS\n' \
      "$timestamp" \
      "$INTERFACE" \
      "$P2P_PORT" \
      "$(readlink /proc/self/ns/net)" >> "$LOG_FILE"
    ;;
  *)
    echo "usage: netns_fault.sh isolate|restore" >&2
    exit 2
    ;;
esac
