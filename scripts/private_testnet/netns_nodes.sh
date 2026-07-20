# Node configuration, installation, and logs for the Task 12 rehearsal.

write_node_environment() {
  local index="$1"
  local node="${NODE_NAMES[$index]}"
  local role="node"
  local bootstrap="/ip4/${NODE_IPS[0]}/tcp/$P2P_PORT"
  local node_root="$STATE_ROOT/nodes/$node"
  local env_file="$node_root/node.env"
  local temporary

  if [[ "$index" -eq 0 ]]; then
    role="seed"
    bootstrap=""
  fi

  sudo -n install -d -m 0750 "$node_root" "$node_root/data" "$node_root/lifecycle"
  temporary="$(mktemp)"
  cat > "$temporary" <<EOF
PULSEDAG_PRIVATE_TESTNET_ROLE=$role
PULSEDAG_CONFIG_PROFILE=private
PULSEDAG_NETWORK_PROFILE=private-testnet-v2.3.0
PULSEDAG_CHAIN_ID=pulsedag-private-v2.3.0
PULSEDAG_CONSENSUS_MODE=legacy
PULSEDAG_P2P_ENABLED=true
PULSEDAG_P2P_MODE=libp2p-real
PULSEDAG_P2P_LISTEN=/ip4/0.0.0.0/tcp/$P2P_PORT
PULSEDAG_P2P_BOOTSTRAP=$bootstrap
PULSEDAG_P2P_MDNS=false
PULSEDAG_P2P_KADEMLIA=true
PULSEDAG_P2P_IDENTITY_KEY=$node_root/data/identity.key
PULSEDAG_PUBLIC_P2P_MULTIADDR=/ip4/${NODE_IPS[$index]}/tcp/$P2P_PORT
PULSEDAG_RPC_BIND=127.0.0.1:$RPC_PORT
PULSEDAG_API_PROFILE=private_operator
PULSEDAG_ADMIN_ENABLED=false
PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE=120
PULSEDAG_RPC_RATE_LIMIT_PER_IP=true
PULSEDAG_ROCKSDB_PATH=$node_root/data/rocksdb
PULSEDAG_AUTO_REBUILD_ON_START=true
PULSEDAG_PERSIST_SNAPSHOT_ON_START=true
PULSEDAG_AUTO_PRUNE_ENABLED=true
PULSEDAG_AUTO_PRUNE_EVERY_BLOCKS=100
PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS=800
PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true
PULSEDAG_PUBLIC_TESTNET_READY=false
PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false
RUST_LOG_STYLE=never
EOF
  sudo -n install -m 0640 "$temporary" "$env_file"
  rm -f "$temporary"
}

install_node_release() {
  local index="$1"
  local node="${NODE_NAMES[$index]}"
  local namespace="${NAMESPACES[$index]}"
  local node_root="$STATE_ROOT/nodes/$node"

  sudo -n ip netns exec "$namespace" \
    python3 "$LIFECYCLE" \
      --root "$node_root/lifecycle" \
      --env-file "$node_root/node.env" \
      --preflight-script "$PREFLIGHT" \
      install \
      --binary "$NODE_BINARY" \
      --release-id "$RELEASE_ID"
}

collect_node_evidence() {
  mkdir -p "$EVIDENCE_ROOT/node-logs"
  local index node lifecycle_log
  for index in "${!NODE_NAMES[@]}"; do
    node="${NODE_NAMES[$index]}"
    lifecycle_log="$STATE_ROOT/nodes/$node/lifecycle/logs/pulsedagd.log"
    if sudo -n test -f "$lifecycle_log"; then
      sudo -n cp "$lifecycle_log" "$EVIDENCE_ROOT/node-logs/$node.log"
      sudo -n chown "$(id -u):$(id -g)" "$EVIDENCE_ROOT/node-logs/$node.log"
    fi
  done
  if sudo -n test -f "$STATE_ROOT/fault/node-4.log"; then
    sudo -n cp "$STATE_ROOT/fault/node-4.log" "$EVIDENCE_ROOT/fault-node-4.log"
    sudo -n chown "$(id -u):$(id -g)" "$EVIDENCE_ROOT/fault-node-4.log"
  fi
}
