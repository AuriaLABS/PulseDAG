# v2.4.0 private burn-in operator profile

## Scope

This profile is the only supported identity for the valueless v2.4.0 private burn-in tracked in #789. It does not authorize a public testnet or a release tag.

## Frozen private identity

Every active node in the v2.4.0 burn-in must use:

```text
PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0
PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0
PULSEDAG_CONSENSUS_MODE=legacy
```

`legacy` means the supported single-parent/tip selection policy. It does not select the previous PoW: genesis and target validation use the v2.4.0 canonical compact-target constants.

The chain ID is operationally significant. It namespaces block, transaction and sync topics; accompanies network messages and selected-tip inventories; and excludes incompatible peers from synchronization.

## Phase A — isolated single node

Start from `configs/single-node/single-node.env.example` after replacing only operator-specific persistent paths. Required properties:

- a new empty RocksDB directory that has never hosted v2.3.0;
- a new evidence directory;
- P2P disabled, no bootnodes and no mDNS;
- loopback RPC;
- external standalone miner;
- fixed sanitized configuration archived before the clock starts.

The 24-hour clock starts only after #789 records the exact candidate SHA, UTC start time, rendered sanitized configuration, clean database initialization, process/container inventory and binary/image digests.

## Phase B — restart, snapshot, prune and restore

Keep the same chain ID, candidate SHA and fixed configuration. Planned restart/prune/restore operations must be timestamped. Any invalidating reset or configuration drift restarts the clock.

## Phase C — real P2P second node

Use `configs/private-testnet/seed.env.example` and `configs/private-testnet/node.env.example` as the base for new processes. Both nodes must use the exact v2.4.0 chain ID, separate persistent database paths and separate persistent P2P identity files. Record full `/p2p/<peer-id>` bootnode addresses.

A node reporting another chain ID, a chain-mismatch inventory rejection or a v2.3.0 identity is a stop condition. Do not merge evidence across chain IDs or candidate SHAs.

## Prohibited reuse

Do not reuse:

- `private-testnet-v2.3.0` or `pulsedag-private-v2.3.0`;
- a v2.3.0 RocksDB directory, snapshot or restore bundle;
- a v2.3.0 rendered environment file;
- previous candidate logs, metrics or screenshots;
- unrestricted endpoints, credentials, wallet seeds or private keys in evidence.

## Authorization boundary

This profile permits only a private, valueless burn-in. Public ingress, `public_testnet_ready=true`, Day 0, the 30-day public-testnet clock, contracts, production custody and mainnet claims remain prohibited.
