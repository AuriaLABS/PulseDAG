#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"
INTERFACE="eth0"
STATE_DIR="/var/lib/pulsedag-task12-netns/fault"
LOG_FILE="$STATE_DIR/node-4.log"

mkdir -p "$STATE_DIR"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

case "$ACTION" in
  isolate)
    ip link show dev "$INTERFACE" >/dev/null
    ip link set dev "$INTERFACE" down
    ip link show dev "$INTERFACE" | grep -q "state DOWN"
    printf '%s action=isolate interface=%s netns=%s result=PASS\n' \
      "$timestamp" "$INTERFACE" "$(readlink /proc/self/ns/net)" >> "$LOG_FILE"
    ;;
  restore)
    ip link show dev "$INTERFACE" >/dev/null
    ip link set dev "$INTERFACE" up
    ip link show dev "$INTERFACE" | grep -q "state UP"
    printf '%s action=restore interface=%s netns=%s result=PASS\n' \
      "$timestamp" "$INTERFACE" "$(readlink /proc/self/ns/net)" >> "$LOG_FILE"
    ;;
  *)
    echo "usage: netns_fault.sh isolate|restore" >&2
    exit 2
    ;;
esac
