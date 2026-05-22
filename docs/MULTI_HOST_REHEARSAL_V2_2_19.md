# MULTI-HOST REHEARSAL v2.2.19

## Inputs
- `configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json`
- `configs/private-testnet/v2_2_19/bootnodes.example.toml`

## Steps
1. Validate topology:
   ```bash
   scripts/v2_2_19_validate_topology.sh configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json
   ```
2. Render per-host env files:
   ```bash
   scripts/v2_2_19_render_multi_host_env.sh configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json
   ```
3. Provision hosts with operator + firewall runbooks.
4. Start bootnode(s), then follower nodes, then miners.
5. Collect evidence:
   ```bash
   scripts/v2_2_19_collect_remote_rehearsal_evidence.sh configs/private-testnet/v2_2_19/topology.multi-host-5n-4m.example.json
   ```

## Expected evidence
- Per-node `/status` and `/p2p/status` payloads.
- Journald tails for node/miner services.
- Peer connectivity snapshots.
- Height progression timeline.

## Safe stop sequence
1. Stop miners on all hosts.
2. Stop non-bootnode full nodes.
3. Stop bootnodes last.
