# Private testnet profile (v2.2.8 pre-testnet)

This document defines **pre-testnet** node profiles for controlled multi-node internal testing on v2.2.8.

> Important: this is **not** the official private testnet release. The official private testnet target remains **v2.3.0**.

## Profiles

`pulsedagd` now supports profile selection via env (`PULSEDAG_CONFIG_PROFILE`) and CLI (`--network`).

- `--network local`
  - `network_profile=local`
  - `chain_id=pulsedag-localnet`
  - default RPC bind `127.0.0.1:8180`
  - default P2P bind `/ip4/127.0.0.1/tcp/31333`
- `--network private`
  - `network_profile=private`
  - `chain_id=pulsedag-private-v2-2-8-pre`
  - default RPC bind `0.0.0.0:8280`
  - default P2P bind `/ip4/0.0.0.0/tcp/32333`

## CLI overrides

- `--p2p-listen <multiaddr>`
- `--rpc-listen <host:port>`
- `--bootnode <multiaddr>` (repeatable)
- `--peer <multiaddr>` (alias of `--bootnode`, repeatable)

Environment variables remain valid and are applied after CLI profile selection so operators can enforce deployment-specific overrides.

## Network identity and peer compatibility

At startup, node logs include:

- version,
- network profile,
- chain id,
- p2p/rpc bind,
- data directory,
- detected genesis hash.

The P2P layer already validates `chain_id` in inbound messages and drops chain-mismatched traffic, preventing incompatible network mixing in gossip/sync paths.
