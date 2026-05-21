# Topology Manifest v2.2.18 (Private Testnet)

This document defines the topology manifest used for private-testnet rehearsals in `v2.2.18`.

## Files

- `configs/private-testnet/v2_2_18/topology.example.json`
- `configs/private-testnet/v2_2_18/topology.local-3n-1m.json`
- `configs/private-testnet/v2_2_18/topology.rc-5n-4m.json`

## Manifest fields

Each manifest includes:

- `run_id`: unique rehearsal execution identifier.
- `chain_id`: network/chain identifier for the private testnet.
- `nodes`: list of nodes participating in the rehearsal.
  - `rpc.port` and `rpc.bind` describe RPC endpoint settings.
  - `rpc.public` defaults to `false` to avoid public RPC exposure.
  - `p2p.port` and `p2p.bind` define peer-to-peer connectivity.
  - `bootstrap_peers` lists initial peer seeds.
  - `data_directory` identifies per-node data storage.
- `miners`: miner processes used for the rehearsal.
  - `target_node`: node where the miner submits work.
  - `backend`: mining backend (`cpu` or `gpu`), with CPU used by default.
- `expected_duration`: expected test duration.
- `perturbation_schedule`: timed fault/recovery events to inject.
- `evidence_directory`: output path for logs, snapshots, and artifacts.

## Scenarios

### local: `topology.local-3n-1m.json`

- 3 nodes.
- 1 CPU miner.
- Loopback-only RPC binding (`127.0.0.1`) on all nodes.

### RC: `topology.rc-5n-4m.json`

- 5 nodes.
- 4 external CPU miners.
- Loopback-only RPC binding (`127.0.0.1`) on all nodes.

## Non-goals / constraints

- No mining pool logic.
- No GPU requirement.
- No public RPC exposure by default.
- No consensus or mining protocol changes.

## Validation

JSON files should be checked for syntax validity, for example:

```bash
jq empty configs/private-testnet/v2_2_18/topology.example.json
jq empty configs/private-testnet/v2_2_18/topology.local-3n-1m.json
jq empty configs/private-testnet/v2_2_18/topology.rc-5n-4m.json
```

If scripts are added later, keep script names in this document aligned with the implementation.
