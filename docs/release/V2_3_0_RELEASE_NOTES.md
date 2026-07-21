# PulseDAG v2.3.0 release notes

> Release-candidate notes. Publication remains blocked until the exact versioned candidate passes every required gate and a final private-testnet release decision is recorded.

## Overview

v2.3.0 prepares PulseDAG for repeatable private-testnet operations. The release focuses on real-network bootstrap, safe node lifecycle management, observability, operator response, protected five-node rehearsal evidence, and accurate direct-peer accounting.

## Highlights

### Repeatable private-testnet bootstrap

- Seed and ordinary-node configuration profiles.
- Fail-closed checks for chain identity, persistent node identity, `libp2p-real`, bootnodes, RPC exposure, pruning, and readiness guardrails.
- Complete bootnode multiaddrs containing the seed `/p2p/<peer-id>`.
- Kademlia enabled and mDNS disabled for supported multi-host operation.

### Safe lifecycle and rollback

- Idempotent install, start, stop, status, restart, upgrade, and rollback operations.
- Immutable release directories and binary checksums.
- PID-reuse protection and structured lifecycle state.
- Health-gated upgrades with automatic restoration of the previous release.
- Persistent identity and RocksDB state preserved across binary changes.

### Accurate real-network peer semantics

- Connected-peer counts in `libp2p-real` mode require active transport sessions.
- Indirect gossipsub authors no longer appear as directly connected peers or direct sync candidates.
- Direct connection establishment and closure control connected and selected-sync surfaces.
- Complete P2P library and real-swarm regression coverage protects ranking, recovery, hysteresis, diversity, and connection budgets.

### Observability baseline

- Versioned metrics inventory.
- Prometheus scrape configuration example.
- Grafana dashboard and alert rules.
- Coverage for health, P2P, sync, mining, mempool, snapshot, prune, storage, and incident signals.

### Operator and incident runbooks

- Bootstrap and external-miner attachment.
- Upgrade and rollback.
- Backup, restore, snapshot, partition, and rejoin recovery.
- High orphan or missing-parent response.
- Disk pressure, RPC abuse, identity rotation, evidence custody, and incident severity ownership.

### Protected five-node rehearsal

- One seed and four ordinary nodes using genuinely isolated Linux network namespaces.
- External standalone mining before and after fault injection.
- Ordinary-node restart and rejoin.
- Bounded target-only P2P isolation with loopback RPC and process availability retained.
- Exact restoration, five-node convergence, healthy endpoint surfaces, zero replay gap, and checksummed evidence.
- Accepted Task 12 `GO` for candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32` in workflow run `29773225491`.

### Repository professionalization

- Contribution and repository standards.
- English code-comment validation.
- Secret, generated-output, broken-link, and cleanup checks.
- Dedicated repository-hygiene CI.
- Active documentation reorganized around supported operations; immutable tags retain historical release material.

## Compatibility and operator action

### Bootnodes

Ordinary private-testnet nodes must use complete libp2p addresses:

```text
/ip4/<seed-address>/tcp/<port>/p2p/<seed-peer-id>
```

Short addresses without a peer ID are rejected by the supported preflight and lifecycle path.

### Peer-count interpretation

In `libp2p-real` mode, peer counts describe direct transport sessions. Relayed message authors may remain visible in diagnostic accounting but are not connected peers.

### Storage

No storage-format migration is included. Preserve persistent identity, RocksDB, and snapshot directories during upgrade or rollback.

### Mining

Mining remains external. Install and operate `pulsedag-miner` separately from `pulsedagd`.

## Release assets

The exact candidate must produce separate node and standalone-miner archives for:

- Linux x86_64;
- Windows x86_64;
- macOS x86_64.

Each archive requires a checksum, build manifest, provenance attestation, and successful unpack-and-smoke evidence. Follow `docs/INSTALL_BINARIES_V2_3_0.md` only after the final v2.3.0 release is published.

## Known limitations

- Private-testnet scope only.
- No ARM release artifacts in the current release matrix.
- Smart contracts remain disabled and out of scope.
- Public-testnet readiness is not claimed.
- The 30-day public-testnet clock has not started.

## Security and integrity

Do not bypass checksum, manifest, provenance, preflight, health, replay, storage-consistency, or release gates. Any SEV-1 consensus, sync, storage, credential, identity, or release incident is a no-go.

## Candidate decision state

- Maintainer proposal decision: `APPROVE_RELEASE_CANDIDATE`.
- Version bump: authorized and applied to the candidate.
- Final private-testnet release decision: `PENDING_FINAL_CANDIDATE_EVIDENCE`.
- Tag and publication: not authorized yet.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
