# PulseDAG v2.3.0 release-candidate proposal

## Decision status

`PENDING_MAINTAINER_DECISION`

This proposal does not authorize a version bump, release tag, artifact publication, public-testnet launch, or the start/backdating of the 30-day public-testnet clock.

## Proposal identity

- Base tag: `v2.2.20`.
- Base commit: `14a1c38249830ee6912d8e70d6d223126cf7f63b`.
- Task 13 activation baseline: `928c25b81ed13b539d9d2b5930609cc97430b9a3`.
- Activation baseline distance from `v2.2.20`: 82 commits ahead, zero behind.
- Exact proposal candidate: the head SHA recorded by the Task 13 proposal workflow.
- Current repository version while this proposal is pending: `VERSION=v2.2.20`, Cargo `2.2.20`.

## Accepted operational prerequisite

Task 12 is complete with independently reviewed protected evidence:

- candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32`;
- workflow run `29773225491`;
- artifact `v2_3_0_task12_netns_22fa09b19da2893fa73b91b198b26675bd1e6e32_29773225491_1`;
- artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`;
- 56 unique controller checksum entries verified;
- all nine mandatory phases `PASS`, `failure=null`;
- 55/55 endpoint snapshots independently verified;
- `version_bump_authorized=false`, `public_testnet_ready=false`, and public-testnet clock not started.

## Change inventory since v2.2.20

### Runtime behavior

The only changed Rust runtime crate file is `crates/pulsedag-p2p/src/lib.rs`.

- Real-network connected-peer surfaces now derive from active libp2p transport sessions.
- Indirect gossipsub authors remain observable but cannot be reported as connected or selected for direct sync requests.
- Connection-established and connection-closed events control connected and selected-sync surfaces.
- Existing active-peer recovery, ranking, cooldown, hysteresis, topology diversity, and connection-budget behavior remains covered by the complete P2P library suite.

No consensus-mode, block-validation, storage-format, chain-state, miner-protocol, smart-contract, or Cargo dependency change is included.

### Private-testnet bootstrap and lifecycle

- Role-specific seed and ordinary-node environment templates.
- Complete libp2p bootnode multiaddrs with `/p2p/<peer-id>`.
- Fail-closed chain identity, persistent path, P2P, RPC, pruning, and readiness preflight.
- Idempotent install, start, stop, status, upgrade, rollback, PID identity, immutable release, and health-gated automatic rollback helpers.

### Observability and operations

- Versioned metrics inventory, Prometheus example, Grafana dashboard, and alert rules.
- Private-testnet operations, incident response, security/capacity, recovery, and five-node rehearsal runbooks.
- Redacted incident evidence collector and runtime metrics exporter.

### Protected rehearsal and evidence

- Strict five-node controller with exact-candidate validation and loopback RPC collection.
- Isolated Linux namespace implementation with one seed, four ordinary nodes, external miner, restart, bounded partition, restoration, convergence, and checksummed evidence.
- Live workflow reporting and independent Task 12 evidence review.

### Repository quality

- Contribution and repository standards.
- English code-comment validation.
- Secret/generated-output/broken-link and cleanup-candidate checks.
- Dedicated repository-hygiene CI.
- Historical v2.2 working material removed from active paths while immutable tags remain the historical source.

## Compatibility statement

### Storage and chain state

- No storage-format migration.
- No chain-ID migration for existing v2.2 networks.
- v2.3.0 private-testnet profiles intentionally use `pulsedag-private-v2.3.0`.
- Persistent identity and RocksDB paths remain external runtime state and must survive binary upgrade or rollback.

### P2P operator behavior

- Ordinary-node bootnodes must include the seed peer ID.
- `/status` and `/p2p/status` peer counts represent direct real-network transport sessions in `libp2p-real` mode.
- RPC remains loopback-only in the supported private-testnet profile.

### Mining

- Mining remains a standalone external application.
- Node and miner release assets remain separate archives.
- No embedded pool or node-mining logic is introduced.

## Upgrade and rollback

The supported upgrade path uses the Task 09 lifecycle controller:

1. install the candidate as a new immutable release;
2. retain the current release as `previous`;
3. stop the managed process safely;
4. activate and health-check the candidate;
5. automatically restore the previous release if health does not recover;
6. preserve identity, RocksDB, snapshots, and operator evidence throughout.

Rollback triggers include health failure, replay or storage inconsistency, chain mismatch, persistent P2P non-convergence, artifact checksum failure, or any SEV-1 security/consensus/sync/storage incident.

## Release asset plan

The existing release workflow builds separate `pulsedagd` and `pulsedag-miner` archives for:

- `x86_64-unknown-linux-gnu`;
- `x86_64-pc-windows-msvc`;
- `x86_64-apple-darwin`.

Every archive requires a per-asset checksum, JSON manifest, build-provenance attestation, unpack-and-smoke verification, consolidated `SHA256SUMS.txt`, and release provenance summary.

Candidate packaging must include `docs/INSTALL_BINARIES_V2_3_0.md`; the v2.2.19 installation guide must not be packaged as the v2.3.0 guide.

## Required exact-candidate gates

Before an approval may authorize a versioned candidate:

- proposal validator and repository hygiene;
- full workspace tests and Clippy;
- RPC and release validation;
- complete P2P library suite and real-swarm validation;
- private-testnet bootstrap and lifecycle contracts;
- observability and runbook contracts;
- multi-host rehearsal controller contract;
- node and standalone-miner release build and smoke tests;
- no unresolved SEV-1 blocker.

After a version change, every gate must rerun on the exact versioned candidate. The previously accepted Task 12 artifact remains historical operational evidence and is not rewritten or backdated.

## Known limitations and excluded scope

- Private-testnet release proposal only.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock remains not started.
- Smart contracts remain disabled and out of scope.
- No ARM release target is currently produced by `release-binaries.yml`.
- No storage migration is proposed.

## Decision options

A maintainer must record exactly one outcome in `V2_3_0_RELEASE_DECISION.md`:

- `APPROVE_RELEASE_CANDIDATE` — authorizes a separate versioned candidate PR only;
- `REQUEST_CHANGES` — keeps versions unchanged until listed changes pass;
- `NO_GO` — stops Task 13 and records the blocking reason.

Until that file records explicit approval, `VERSION` and Cargo remain `2.2.20` and no release tag or publication is allowed.
