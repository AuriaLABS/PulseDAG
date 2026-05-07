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


## Negative-path diagnosis for v2.2.12

Use these checks only as targeted negative-path rehearsals. Do not hand-edit the normal happy-path smoke test, do not change consensus rules, and do not relax PoW validation to manufacture outcomes.

### Chain-id mismatch drops

Expected behavior: block announcements, block payloads, and block-data responses with a foreign `chain_id` are dropped before node acceptance. Operators should see no new accepted block and should see mismatch counters increase.

Evidence to capture:

- Unit check: `cargo test -p pulsedag-p2p v2_2_12_block_chain_id_mismatches_are_dropped_and_counted`.
- Live endpoints when rehearsed: `/p2p/status.inbound_chain_mismatch_dropped`, `/p2p/status.last_drop_reason`, and `/sync/status.chain_id_mismatch_drops`.
- Expected `last_drop_reason` examples include `chain_mismatch_block`, `chain_mismatch_block_announce`, or `chain_mismatch_block_data`.

Diagnosis:

- If mismatch counters stay flat, confirm the negative-path payload actually used a different chain id than `/status.chain_id`.
- If peer counts are zero, first solve connectivity; a mismatch rehearsal cannot prove inbound dropping if no inbound traffic arrives.
- If a wrong-chain payload reaches block acceptance, treat it as a release blocker.

### Duplicate block announcement and block-data suppression

Expected behavior: duplicate block announcements or duplicate block-data payloads are suppressed, not accepted repeatedly, and not repeatedly relayed.

Evidence to capture:

- Unit check: `cargo test -p pulsedag-p2p v2_2_12_duplicate_blockdata_is_delivered_once_and_counted`.
- Unit check: `cargo test -p pulsedag-p2p repeated_block_relay_storm_is_deduped_without_counter_inflation`.
- Live endpoints: `/p2p/status.duplicate_suppression_counters`, `/p2p/status.block_propagation_counters`, `/sync/status.duplicate_suppression_counters`, and `/sync/status.last_accepted_peer_block`.

Diagnosis:

- `inbound_duplicates_suppressed` should rise for repeated inbound duplicates.
- Outbound block publish counters should show first-seen relay only once for the same block hash; duplicate-suppression counters should absorb repeats.
- `accepted_p2p_blocks` and `blockdata_accepted` should not increase once per duplicate payload.

### Invalid peer block rejection

Expected behavior: invalid peer blocks are rejected by the normal block acceptance path. PoW failures remain `InvalidPow`; invalid transactions remain `InvalidTransaction`; missing parents are queued as orphan recovery pressure rather than accepted.

Evidence to capture:

- Unit check: `cargo test -p pulsedag-core rejects_block_with_invalid_pow`.
- Unit check: `cargo test -p pulsedag-core invalid_transaction_in_peer_block_returns_invalid_transaction_outcome`.
- Live endpoints when applicable: `/sync/status.last_rejected_peer_block_reason`, `/p2p/status.last_rejected_peer_block_reason`, and `/diagnostics.last_rejected_peer_block_reason`.
- Logs should include `peer_block_rejected` for validation failures or `peer_block_missing_parent` for orphan queuing.

Diagnosis:

- If `last_rejected_peer_block_reason` is empty, confirm an invalid peer block was actually received after node startup and after the runtime counters were reset.
- If `blockdata_invalid_pow` or `pulsedag_invalid_pow_total` rises, the peer supplied a payload failing the active PoW policy; do not lower difficulty or bypass validation to clear the test.
- If only missing-parent counters rise, use `/sync/missing` to identify missing parent hashes and allow normal catch-up to request them.

## Final diagnostic fields for v2.2.12 rehearsal

Operators should capture these fields from every node when comparing before/after sync recovery evidence. Fields are diagnostic only and do not change consensus rules.

### `/sync/status`

- `chain_id`, `p2p_enabled`, and `p2p_mode` identify the network namespace and whether sync has a P2P source.
- `selected_sync_peer` reports the chosen sync peer when the P2P selector has one available.
- `catchup_stage`, `lag_blocks`, `lag_band`, `catchup_progress_bps`, and `catchup_summary` describe the local catch-up phase and lag severity.
- `recovery_reason` explains why the node is recovering, degraded, or under bounded no-progress remediation.
- `pending_block_requests`, `pending_missing_parents`, and `orphan_count` expose outstanding block fetches, missing parent pressure, and queued orphans.
- `last_accepted_peer_block` and `last_rejected_peer_block_reason` show the most recent peer block outcome when available.
- `chain_id_mismatch_drops` mirrors P2P chain-id mismatch drops for quick cross-checks from the sync view.
- `duplicate_suppression_counters` mirrors P2P duplicate suppression totals for inbound messages, outbound messages, outbound transactions, and outbound blocks.
- `readiness_reasons` lists operator-actionable blockers; `p2p_ready_for_private_rehearsal` is true only when the readiness list is empty.

### `/p2p/status`

- `p2p_enabled`, `p2p_mode`, `mode`, `connected_peers_are_real_network`, and `connected_peers_semantics` describe whether peer counts represent real libp2p connections or simulated/internal observations.
- `peer_id`, `listening_addresses`, `connected_peers`, `peer_count`, `topics`, `mdns`, and `kademlia` describe local identity, reachability, and topic membership.
- `selected_sync_peer`, `sync_candidates`, and `sync_selection_sticky_until_unix` expose peer selection and anti-flap state.
- `pending_block_requests`, `pending_missing_parents`, `orphan_count`, `sync_state`, `last_accepted_peer_block`, and `last_rejected_peer_block_reason` mirror sync runtime pressure from the P2P view.
- `inbound_chain_mismatch_dropped` and `last_drop_reason` identify chain-id mismatch drops and the most recent inbound/outbound suppression reason.
- `duplicate_suppression_counters`, `inbound_duplicates_suppressed`, `outbound_duplicates_suppressed`, `tx_propagation_counters`, and `block_propagation_counters` expose duplicate suppression and propagation health.
- `peer_state_summary`, `recovery_activity_summary`, and `peer_recovery` describe lifecycle tiers, cooldown/backoff, reconnect attempts, suppression, recent failures, and recovery success state per peer.
- `connection_slot_budget`, `connected_slots_in_use`, `available_connection_slots`, `topology_*`, `degraded_mode`, and `connection_shaping_active` help diagnose topology pressure and connection shaping.
- `readiness_reasons` and `p2p_ready_for_private_rehearsal` provide the operator-facing rehearsal gate summary.

### `/sync/missing`

- `pending_block_requests`, `pending_missing_parents`, and `orphan_count` summarize outstanding recovery pressure.
- `orphans[].hash` and `orphans[].missing_parents` list child blocks blocked on missing parents.

## Recovery success criteria

A recovery is complete when:

- Every node reports `mode=libp2p-real` and expected connected peers.
- Every node reports the same chain id.
- Best height converges across the rehearsal topology.
- Pending block requests, missing parents, and orphan counts drain after catch-up.
- Restarted nodes rejoin without manual state edits.
- Operators capture before/after endpoint bundles and explain the root cause or unresolved risk.
