# Sync Recovery and Troubleshooting v2.2.12

This guide diagnoses v2.2.12 full private-testnet rehearsal and hardening runs using real `libp2p-real` nodes. It extends the v2.2.11 troubleshooting baseline for longer-running, multi-node, and multi-operator validation. It does not claim official private-testnet readiness; v2.3.0 remains the readiness decision milestone.

## First-response checklist

Run these on every node before deep diagnosis:

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/p2p/peers
curl -fsS http://127.0.0.1:18080/p2p/propagation
curl -fsS http://127.0.0.1:18080/sync/status
curl -fsS http://127.0.0.1:18080/sync/missing
```

For B/C or external nodes, replace port `18080` with the node-local RPC bind. Capture the same endpoint set before and after every restart/rejoin action.

Useful fields:

- `/status`: version, chain id, best height, tips, orphan count, peer count, P2P mode, and sync state.
- `/p2p/status`: mode, peer id, listening addresses, connected peers, topics, message counters, drop counters, duplicate counters, and peer lifecycle/backoff state.
- `/p2p/propagation`: block and transaction propagation counters, duplicate suppression, and relay-loop indicators.
- `/sync/status`: chain id, best height, selected tip, orphan count, pending block requests, missing parents, catch-up stage, lag band, recovery reason, and last rejected peer block reason.
- `/sync/missing`: pending request and missing parent hashes.

## Scenario: peer count is zero

Symptoms:

- `/p2p/status` has an empty `connected_peers` array.
- `/sync/status` reports no usable peer or no connected peers.
- Lagging nodes do not catch up even though A is mining.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/p2p/status
ss -ltnp | rg '18181|18182|18183'
sudo ufw status verbose
```

Remediation:

1. Confirm all nodes use `PULSEDAG_P2P_ENABLED=true` and `PULSEDAG_P2P_MODE=libp2p-real`.
2. Confirm bootnodes point at reachable P2P multiaddrs, not RPC addresses.
3. Open only the intended P2P TCP ports between nodes.
4. Keep RPC private unless protected by firewalling, VPN, or SSH forwarding.
5. Restart affected peers one at a time and capture before/after endpoint bundles.

## Scenario: chain-id mismatch

Symptoms:

- Peers connect, but propagation is ignored.
- `/p2p/status.inbound_chain_mismatch_dropped` rises.
- `/p2p/topics` differs between nodes.
- `/sync/status.chain_id` differs across nodes.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/sync/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18080/p2p/topics
curl -fsS http://127.0.0.1:18081/p2p/topics
```

Remediation:

1. Stop affected nodes.
2. Set exactly one shared rehearsal chain id across all operators.
3. Clean data directories that were initialized with the wrong chain id.
4. Restart A first, then restart peers in documented order.
5. Capture corrected `/status`, `/p2p/status`, and `/sync/status` evidence.

## Scenario: block announced but not fetched

Symptoms:

- A height increases, peer counts are non-zero, but other nodes remain behind.
- `/p2p/propagation` shows traffic, but `/sync/status.pending_block_requests` does not drain.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18081/p2p/propagation
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18081/sync/missing
```

Remediation:

1. Verify the lagging node is connected to a peer that has the block.
2. Check topic and chain-id alignment.
3. Inspect pending block requests and node logs for failed request/data handling.
4. Restart the lagging node to force tip discovery and catch-up.
5. If the origin node does not have the block, confirm the external miner submitted through the expected node RPC endpoint.

## Scenario: restart/rejoin does not converge

Symptoms:

- A restarted node reports healthy RPC but no peers.
- Peers reconnect but height remains behind for multiple block intervals.
- `/sync/status.catchup_stage` remains degraded or recovering without progress.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/health
curl -fsS http://127.0.0.1:18081/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18081/sync/missing
```

Remediation:

1. Compare pre-restart and post-restart peer ids, listen addresses, and bootnode settings.
2. Confirm the data directory was not accidentally cleaned unless the test intended a fresh rejoin.
3. Verify at least one connected peer is at or near the current network height.
4. Restart only one node at a time to isolate rejoin behavior.
5. Record the final convergence time as hardening evidence.

## Scenario: missing parent stuck

Symptoms:

- `/sync/status.pending_missing_parents` remains non-zero.
- `/sync/status.orphan_count` remains non-zero.
- `/sync/missing` shows the same hash across multiple polling intervals.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/sync/missing
curl -fsS http://127.0.0.1:18081/orphans
curl -fsS http://127.0.0.1:18081/p2p/status
```

Remediation:

1. Confirm at least one connected peer has the missing parent.
2. Reconnect to the node that mined or stored the parent.
3. Restart the lagging node to re-run targeted requests.
4. If the orphan came from a test injection, clean only the affected test data and replay from a healthy peer.

## Scenario: duplicate storm or noisy relay

Symptoms:

- Duplicate suppression or relay-loop counters rise rapidly.
- Heights do not converge or peer message rate limiting increases.
- Multiple operators may have started duplicate miners or relayers.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/p2p/propagation
curl -fsS http://127.0.0.1:18080/sync/status
```

Remediation:

1. Stop unintended duplicate submit loops or manual relayers.
2. Confirm the number of external miners intended for the scenario.
3. Verify shared chain id and topics across all nodes.
4. Temporarily reduce topology to A+B, verify convergence, then add nodes back one at a time.
5. Preserve counters and logs as diagnostics review evidence.

## Scenario: wrong firewall ports

Symptoms:

- RPC works over SSH or localhost, but peers never connect.
- Operators can curl RPC status, but `connected_peers` is empty.

Facts:

- RPC ports are HTTP ports.
- P2P ports are libp2p TCP ports.
- Bootnodes must point to P2P ports, not RPC ports.

Remediation on Ubuntu with UFW, adjusted for the selected topology:

```bash
sudo ufw allow from <PEER_IP> to any port <P2P_PORT> proto tcp
sudo ufw status verbose
```

Do not expose RPC publicly unless it is explicitly protected by host firewalling, VPN, or another access-control layer.

## Recovery success criteria

A recovery is complete when:

- Every node reports `mode=libp2p-real` and expected connected peers.
- Every node reports the same chain id.
- Best height converges across the rehearsal topology.
- Pending block requests, missing parents, and orphan counts drain after catch-up.
- Restarted nodes rejoin without manual state edits.
- Operators capture before/after endpoint bundles and explain the root cause or unresolved risk.
