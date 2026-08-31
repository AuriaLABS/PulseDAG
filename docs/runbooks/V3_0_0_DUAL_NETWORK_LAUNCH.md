# v3.0.0 coordinated mainnet + parallel-testnet launch runbook

Status: **PRE-GO / Q4 2026 TARGET / NOT LAUNCHED**

Authority: issue #781 is the sole final launch-control record. This runbook implements the sequencing from `docs/ROADMAP_V3_0_0.md`; it cannot set launch GO by itself.

## Launch model

PulseDAG v3.0.0 is launched through one coordinated release program with two independent public networks:

- **mainnet** — production network and production-value boundary;
- **parallel public testnet** — permanent public validation network for upgrades and rehearsals.

There is no standalone public-testnet launch required before mainnet and no 30-day public-testnet acceptance clock as a mainnet prerequisite.

## Hard preconditions

Before any official v3 mainnet/testnet endpoint is advertised as launched, all of the following must be attached to #781/#794 on one exact v3.0.0 candidate:

1. exact v3.0.0 source SHA/tree and release identity;
2. node/miner/wallet package hashes, manifests and provenance;
3. no unresolved Sev-1 consensus, state, storage, replay, sync, mining or operator-safety defect;
4. #803 dependency/security gate resolved for mainnet/public exposure;
5. #819 production wallet/custody launch scope completed or explicitly excluded from launch with no misleading custody claim;
6. final adversarial multi-node/multi-miner rehearsal PASS;
7. restart/snapshot/prune/restore/rejoin and clean-bootstrap PASS;
8. independent mainnet and testnet chain IDs, network profiles, genesis hashes and consensus/config digests frozen;
9. independent mainnet and testnet bootnodes/peer IDs/multiaddrs frozen;
10. DNS/TLS/status endpoint ownership and firewall policy recorded separately for both networks;
11. observability, evidence export and incident bundle collection verified on all launch roles;
12. primary and backup operators plus UTC launch/on-call/rollback window recorded;
13. explicit `GO_V3_DUAL_LAUNCH` recorded in #781.

## Identity-separation requirements

Mainnet and testnet must be impossible to confuse accidentally.

- different chain IDs;
- different network profiles;
- different genesis blocks/hashes;
- domain-separated wallet/signing identity where the protocol requires it;
- independent persistent bootnode peer identities;
- separate DNS/status/RPC endpoints;
- no default cross-network bootstrapping;
- network mismatch must fail closed in node, miner and wallet flows.

The final identities remain `TBD` until freeze. Do not invent production chain IDs, genesis values, peer IDs or DNS names in committed templates.

## Release freeze

Freeze one exact v3.0.0 source candidate before final launch rehearsal. Record:

- exact source SHA and tree;
- VERSION and Cargo workspace/package versions;
- protocol/activation contract version;
- Linux/Windows/macOS package identities as supported;
- extracted binary hashes;
- container/image digests where used;
- SBOM/provenance where available;
- mainnet config/genesis digests;
- parallel-testnet config/genesis digests.

Any release-affecting code, dependency, protocol, storage, wallet-signing or activation change after freeze invalidates affected evidence and requires an explicit rebaseline.

## Infrastructure roles

### Seed / bootnode

- persistent P2P identity outside release directories;
- public P2P only;
- RPC/admin remains loopback or private management;
- no wallet custody on seed nodes;
- record actual live `/p2p/<peer-id>` multiaddr.

### Ordinary node

- persistent identity and RocksDB volume;
- bootstraps only from the frozen network-specific bootnode set;
- admin disabled on public-facing process;
- backup/snapshot/recovery policy enabled and tested.

### Observer / public RPC

- `public_safe` exposure only;
- admin/operator surfaces disabled;
- TLS/reverse-proxy ownership recorded where used;
- explicit request-body, rate-limit and CORS policy;
- no credentials, seeds, mnemonics or private keys in static configuration or evidence.

### External miner

- exact packaged v3.0.0 miner;
- connects only to the intended network endpoint;
- network identity mismatch fails closed;
- standalone miner remains free of pool payout/share-accounting logic.

### Wallet / relay

- exact packaged v3.0.0 wallet when an official wallet is included;
- encrypted local custody only;
- no raw private keys/seeds/passwords over public RPC;
- relay accepts only the supported signed-transaction contract;
- wallet verifies chain/network identity before sign/broadcast.

## Final rehearsal matrix

Run against the frozen v3 candidate and production-like identities/configuration without exposing the official networks prematurely:

- multi-node and multi-miner convergence;
- competing/parallel parent pressure and deterministic state/order checks;
- seed and ordinary-node restart;
- miner restart/disconnect/rejoin;
- bounded node isolation/rejoin;
- delayed-parent/missing-history recovery;
- snapshot export/verify/restore;
- compact prune and post-prune recovery;
- clean-node bootstrap/catch-up;
- wallet create/restore/sign/broadcast/reconcile when included;
- public-safe RPC abuse/body/rate-limit/CORS checks;
- dependency/security and secret-scanning closeout;
- resource/disk/RocksDB/P2P/RPC latency monitoring.

## GO review

#781 must record exactly one decision:

- `GO_V3_DUAL_LAUNCH`
- `DELAY_V3_DUAL_LAUNCH`
- `NO_GO_V3_DUAL_LAUNCH`

A GO is valid only for the exact frozen candidate, artifact set and the two recorded network identities.

## Coordinated launch sequence

After GO:

1. verify release/artifact/config/genesis hashes on every host;
2. start mainnet seed nodes and verify their persistent identities;
3. start parallel-testnet seed nodes and verify independent identities;
4. start ordinary/observer nodes for both networks;
5. verify peer-mesh separation and no cross-network compatibility;
6. start the approved miners for each network;
7. verify independent block production, selected-tip/state convergence and submit flow;
8. bring up official public RPC/status endpoints;
9. verify wallet/relay flows against the intended network identities;
10. record first accepted mainnet block/height and UTC timestamp;
11. record first accepted parallel-testnet block/height and UTC timestamp;
12. publish exact binaries/checksums, network identities, bootnodes/endpoints, known limitations, operator/user instructions and incident/security routes.

The two recorded timestamps may differ operationally by minutes; they belong to one coordinated release window and neither requires a prior 30-day acceptance clock.

## Hard-stop / rollback conditions

Delay or stop rollout on:

- consensus/state/order divergence;
- chain/genesis/config/artifact digest mismatch;
- accidental mainnet/testnet peer or signing-domain compatibility;
- unexplained data loss or restore failure;
- persistent sync/rejoin failure;
- unresolved submit-finality incoherence;
- reachable security blocker without approved disposition;
- wallet custody/signing defect that violates the approved launch scope;
- loss of monitoring, operator control or rollback capability.

Rollback actions and incident timelines must be recorded against the exact affected network and release identity.

## Post-launch

- heightened monitoring for the first 24 hours and first week;
- retain the parallel testnet as the permanent public upgrade-validation network;
- no automatic smart-contract or high-cadence activation unless explicitly included in the frozen v3.0.0 launch decision;
- future consensus/network upgrades rehearse on the parallel testnet before a separately authorized mainnet activation.
