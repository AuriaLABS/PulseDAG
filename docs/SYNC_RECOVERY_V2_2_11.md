# Sync Recovery and Troubleshooting v2.2.11

This guide diagnoses v2.2.11 P2P completion rehearsals using real `libp2p-real` nodes. It is intended for local hosts or external Ubuntu servers. It does not claim public mainnet readiness or v2.3.0 readiness.

## First-response checklist

Run these on every node before deep diagnosis:

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/sync/status
curl -fsS http://127.0.0.1:18080/sync/missing
```

For nodes B/C, replace port `18080` with `18081`/`18082` or the server-local RPC bind.

Useful fields:

- `/p2p/status`: `mode`, `peer_id`, `listening`, `connected_peers`, `topics`, `inbound_chain_mismatch_dropped`, duplicate counters, peer recovery fields.
- `/sync/status`: `chain_id`, `best_height`, `selected_tip`, `orphan_count`, `pending_block_requests`, `pending_missing_parents`, `catchup_stage`, `lag_band`, `recovery_reason`, `last_rejected_peer_block_reason`.
- `/sync/missing`: pending request and missing parent hashes.

## Scenario: `peer_count = 0`

Symptoms:

- `/p2p/status` has an empty `connected_peers` array.
- `/sync/status.readiness_reasons` includes `no connected peers`.
- B/C do not catch up even though A is mining.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/p2p/status
ss -ltnp | rg '18181|18182|18183'
sudo ufw status verbose
```

Remediation:

1. Confirm all nodes use `PULSEDAG_P2P_ENABLED=true` and `PULSEDAG_P2P_MODE=libp2p-real`.
2. Confirm B/C bootnode points at A's reachable P2P multiaddr, not A's RPC address:
   `--bootnode /ip4/<NODE_A_PUBLIC_OR_PRIVATE_IP>/tcp/18181`.
3. Open the P2P TCP ports between nodes. For the rehearsal defaults: `18181`, `18182`, and `18183`.
4. Keep RPC private unless explicitly secured. Opening `18080` is not a substitute for opening P2P.
5. Restart B/C after correcting the bootnode or firewall.

## Scenario: chain-id mismatch

Symptoms:

- Peers may connect, but propagation is ignored.
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

1. Stop all nodes.
2. Set exactly one shared chain id, for example `PULSEDAG_REHEARSAL_CHAIN_ID=pulsedag-rehearsal-v2-2-11` when using scripts or `PULSEDAG_CHAIN_ID=...` when launching manually.
3. Clean data directories if a node was initialized with the wrong chain id.
4. Restart A, then B/C.

## Scenario: block announced but not fetched

Symptoms:

- A height increases, B/C peer count is non-zero, but B/C heights remain unchanged.
- `/p2p/propagation` shows block traffic but `/sync/status.pending_block_requests` remains zero or does not drain.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18081/p2p/propagation
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18081/sync/missing
```

Remediation:

1. Verify B/C are connected to a peer that actually has the block.
2. Check that B/C topics match A's topics.
3. Inspect `pending_block_requests`; if it grows and never drains, check peer logs for failed `GetBlock`/`BlockData` handling.
4. Restart the lagging node to force `GetTips`/`Tips` catch-up.
5. If the hash is missing on A, ensure the external miner submits to A's RPC and A accepted the block.

## Scenario: `BlockData` rejected with `invalid_pow`

Symptoms:

- `/sync/status.last_rejected_peer_block_reason` contains `invalid_pow` or another PoW validation reason.
- The receiving node does not apply the block.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18081/pow/policy
curl -fsS http://127.0.0.1:18080/pow/policy
```

Remediation:

1. Confirm all nodes run the same binaries and profile.
2. Confirm all nodes share the same chain id and data lineage.
3. Ensure blocks originate from `pulsedag-miner` through `POST /mining/submit`; do not hand-edit block payloads.
4. Stop any miner using stale templates or a different node profile.
5. Clean and restart only the rejected test data if the rehearsal intentionally injected invalid blocks.

## Scenario: `missing_parent` stuck

Symptoms:

- `/sync/status.pending_missing_parents` remains non-zero.
- `/sync/status.orphan_count` remains non-zero.
- `/sync/missing` shows the same hash for multiple polling intervals.

Checks:

```bash
curl -fsS http://127.0.0.1:18081/sync/missing
curl -fsS http://127.0.0.1:18081/orphans
curl -fsS http://127.0.0.1:18081/p2p/status
```

Remediation:

1. Confirm at least one connected peer has the missing parent.
2. Restart the lagging node to re-run tip discovery and targeted requests.
3. If no connected peer has the parent, reconnect to the node that mined or stored it.
4. If the orphan was produced by a test injection, clean the affected node data and replay from a healthy peer.

## Scenario: duplicate storm

Symptoms:

- `inbound_duplicates_suppressed`, `outbound_duplicates_suppressed`, `block_outbound_duplicates_suppressed`, `tx_outbound_duplicates_suppressed`, or `relay_loop_prevented` rise rapidly.
- Heights do not converge or peer message rate limiting increases.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/p2p/status
curl -fsS http://127.0.0.1:18080/p2p/propagation
curl -fsS http://127.0.0.1:18080/sync/status
```

Remediation:

1. Stop any duplicate manual relayers or repeated submit loops.
2. Confirm only one external miner loop is intended for the scenario.
3. Verify all nodes use the same chain id and topics; mismatch-induced retries can look like noisy traffic.
4. Temporarily reduce topology to A+B, verify convergence, then add C back.

## Scenario: B/C not catching up

Symptoms:

- Node A height grows.
- B/C have peers but `best_height` stays lower than A.
- `/sync/status.catchup_stage` is `recovering` or `degraded` for more than several target block intervals.

Checks:

```bash
curl -fsS http://127.0.0.1:18080/sync/status
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18082/sync/status
curl -fsS http://127.0.0.1:18081/p2p/status
curl -fsS http://127.0.0.1:18082/p2p/status
```

Remediation:

1. Check `selected_sync_peer`; B/C should select or be able to reach a peer near A's height.
2. Check `pending_block_requests` and `pending_missing_parents`; use the targeted scenarios above if either is stuck.
3. Restart one lagging node at a time to validate restart catch-up.
4. If data was initialized with an old chain id or incompatible genesis, stop, clean that node data directory, and restart with the shared rehearsal chain id.

## Scenario: wrong firewall ports

Symptoms:

- RPC works over SSH or localhost, but peers never connect.
- `curl http://<host>:18080/status` works from an operator machine, but `connected_peers` is empty.

Facts:

- RPC ports are HTTP ports (`18080`, `18081`, `18082` in the rehearsal scripts).
- P2P ports are libp2p TCP ports (`18181`, `18182`, `18183` in the rehearsal scripts).
- Bootnodes must point to P2P ports, not RPC ports.

Remediation on Ubuntu with UFW, adjust source ranges for your environment:

```bash
sudo ufw allow from <NODE_B_IP> to any port 18181 proto tcp
sudo ufw allow from <NODE_C_IP> to any port 18181 proto tcp
sudo ufw allow from <NODE_A_IP> to any port 18182 proto tcp
sudo ufw allow from <NODE_A_IP> to any port 18183 proto tcp
sudo ufw status verbose
```

Do not expose RPC publicly unless it is explicitly protected by host firewalling, VPN, or another access control layer.

## Recovery success criteria

A recovery is complete when:

- `GET /p2p/status` on every node shows `mode=libp2p-real` and at least one real connected peer.
- `GET /sync/status` on every node shows matching `chain_id`.
- `best_height` converges across A/B/C.
- `pending_block_requests=0`, `pending_missing_parents=0`, and `orphan_count=0` after catch-up.
- `catchup_stage` is `steady` and `p2p_ready_for_private_rehearsal=true` for the tested topology.
