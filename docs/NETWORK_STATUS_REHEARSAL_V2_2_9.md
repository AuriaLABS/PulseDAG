# Network Status Rehearsal v2.2.9

This document summarizes the operator-facing RPC endpoints for rehearsal nodes.

## `/status`
Use `/status` for a compact control-plane view. It includes:
- version
- chain_id
- best_height, block_count, selected_tip
- p2p_enabled, p2p_mode, peer_count
- storage_backend
- uptime_secs
- orphan_count
- mempool_size

## `/runtime`
Use `/runtime` for cumulative runtime counters and health rollups. Relevant counters include:
- `pulsedag_blocks_accepted_total`
- `pulsedag_blocks_rejected_total`
- `pulsedag_invalid_pow_total`
- `pulsedag_mining_templates_total`
- `pulsedag_mining_submits_total`
- `pulsedag_p2p_blocks_received_total`
- `pulsedag_sync_missing_parents_total`
- `queued_orphan_blocks`, `adopted_orphan_blocks`
- `external_mining_submit_total`
- `peer` and P2P health fields

## `/metrics`
`/metrics` exposes a lightweight JSON metrics snapshot and now includes core runtime counters.

Limitations:
- Counters are process-local and reset on node restart.
- Peer/orphan counts are instantaneous snapshots.
- Not all P2P announce/request breakdowns are tracked as dedicated counters yet.

## `/p2p/status`
`/p2p/status` provides P2P runtime details including:
- runtime mode
- listen addresses
- connected peers and count
- peer lifecycle/recovery details
- semantics indicating whether peers represent real network connectivity
