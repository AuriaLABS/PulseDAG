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

## Staged local convergence gates

Before treating the multi-host 5-node/4-miner rehearsal as a release signal, run the staged local gates so failures are easier to classify:

```bash
# Mandatory baseline readiness gate.
scripts/v2_2_19_private_5n_1m_rehearsal.sh

# Intermediate fork-pressure gate; set INTERMEDIATE_REQUIRED=0 only for an explicit warning-only run.
scripts/v2_2_19_private_5n_2m_rehearsal.sh

# Diagnostic stress gate; this must not be used to claim public readiness while orphan recovery is still limited.
scripts/v2_2_19_private_5n_4m_rehearsal.sh

# Orchestrated sequence: baseline required, intermediate required by default, stress diagnostic by default.
scripts/v2_2_19_staged_convergence_gates.sh
```

Each staged run stops miners, waits `QUIESCENCE_SECS`, resamples `/status`, `/readiness`, `/p2p/status`, and `/sync/status`, then writes `evidence-summary.md`, `p2p_convergence.json`, `quiescence-metrics.json`, `evidence.tar.gz`, and `evidence.tar.gz.sha256` for both PASS and FAIL outcomes. The summary includes per-node final height/tip, peer count, orphan count, pending missing-parent count, sync status, per-miner templates/submits/accepted/rejected, distinct final tips, worst lag from max height, lag improvement during quiescence, and failure classifications.

Failure classifications are: `HARNESS_TIMEOUT`, `RPC_UNAVAILABLE`, `P2P_NOT_CONNECTED`, `MINER_NO_TEMPLATE`, `MINER_NO_ACCEPTED_BLOCKS`, `SYNC_DIVERGED`, `MISSING_PARENT_BACKLOG`, `READINESS_SCHEMA_MISMATCH`, and `CLEANUP_HANG`. These gates do not change consensus rules and do not set `public_testnet_ready=true`.

## Expected evidence
- Per-node `/status` and `/p2p/status` payloads.
- Journald tails for node/miner services.
- Peer connectivity snapshots.
- Height progression timeline.

## Safe stop sequence
1. Stop miners on all hosts.
2. Stop non-bootnode full nodes.
3. Stop bootnodes last.
