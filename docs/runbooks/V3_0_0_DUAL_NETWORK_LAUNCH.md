# v3.0.0 coordinated mainnet + parallel-testnet launch runbook

Status: **PRE-GO / Q4 2026 TARGET / NOT LAUNCHED**

Authority: issue #781 is the sole final launch-control record. This runbook implements the integrated sequencing from `docs/ROADMAP_V3_0_0.md`; it cannot set launch GO by itself.

## Integrated release path

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

The technical acceptance criteria from `ROADMAP_V2_5_0.md` and `ROADMAP_V2_6_0.md` are mandatory inputs to the v3.0.0 GO decision.

## Launch model

PulseDAG v3.0.0 is launched through one coordinated release program with two independent public networks:

- **mainnet** — production network and production-value boundary;
- **parallel public testnet** — permanent public validation network for upgrades, contracts/proofs and applications.

There is no standalone public-testnet launch required before mainnet and no pre-mainnet 30-day public-testnet acceptance clock.

## Hard preconditions

Before any official v3 mainnet/testnet endpoint is advertised as launched, all of the following must be attached to #781/#794 on one exact integrated v3.0.0 candidate:

1. exact v3.0.0 source SHA/tree and release identity;
2. node, CPU miner, NVIDIA miner, AMD/ATI miner and wallet package hashes/manifests/provenance for supported targets;
3. all mandatory incorporated v2.5 scale/P2P/sync/mempool/mining/GPU/high-cadence/replay/chaos gates PASS;
4. all mandatory incorporated v2.6 covenant/contract/VM/application/asset/economic/security/replay gates PASS;
5. >=1,000,000-block deterministic DAG replay PASS;
6. >=1,000,000 programmable-operation deterministic replay PASS;
7. >=168 contiguous hours on one unchanged integrated v3 candidate PASS;
8. 30 accepted days of programmability-enabled exact-candidate pre-launch evidence complete;
9. no unresolved Sev-1 consensus, state, storage, replay, sync, mining, GPU, wallet, contract, proof-system or operator-safety defect;
10. #803 dependency/security gate resolved for the exact v3 public/mainnet candidate;
11. #819 production wallet/custody launch scope completed on the exact candidate;
12. final >=25-node / >=16-miner adversarial rehearsal PASS with NVIDIA/AMD coverage where supported;
13. restart/snapshot/prune/restore/rejoin and clean-bootstrap PASS;
14. independent mainnet and testnet chain IDs, network profiles, genesis hashes and consensus/activation/config digests frozen;
15. independent mainnet and testnet bootnodes/peer IDs/multiaddrs frozen;
16. DNS/TLS/status/RPC/event endpoint ownership and firewall policy recorded separately for both networks;
17. observability, evidence export and incident bundle collection verified on all launch roles;
18. primary and backup operators plus UTC launch/on-call/rollback window recorded;
19. explicit `GO_V3_DUAL_LAUNCH` recorded in #781.

## Identity-separation requirements

Mainnet and testnet must be impossible to confuse accidentally.

- different chain IDs;
- different network profiles;
- different genesis blocks/hashes;
- domain-separated wallet/signing identity;
- contract/application/proof domains bound to the intended network;
- independent persistent bootnode peer identities;
- separate DNS/status/RPC/event endpoints;
- no default cross-network bootstrapping;
- network mismatch fails closed in node, miner, wallet and application flows.

The final identities remain `TBD` until freeze. Do not invent production chain IDs, genesis values, peer IDs or DNS names in committed templates.

## Release freeze

Freeze one exact integrated v3.0.0 source candidate before final launch rehearsal. Record:

- exact source SHA and tree;
- VERSION and Cargo workspace/package versions;
- protocol/transaction activation contract;
- contract/VM/proof-system versions;
- storage/schema/snapshot compatibility identity;
- production monetary/economic policy digest;
- Linux/Windows/macOS package identities as supported;
- CPU/NVIDIA/AMD miner package identities as supported;
- extracted binary hashes;
- container/image digests where used;
- SBOM/provenance/attestation where available;
- mainnet config/genesis digests;
- parallel-testnet config/genesis digests.

Any release-affecting code, dependency, protocol, storage, GPU, wallet-signing, contract, VM, proof or activation change after freeze invalidates affected evidence and requires explicit rebaseline.

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
- backup/snapshot/recovery policy enabled and tested;
- contract/state/proof execution remains bounded by the frozen v3 resource model.

### Observer / public RPC

- `public_safe` exposure only;
- admin/operator surfaces disabled;
- TLS/reverse-proxy ownership recorded where used;
- explicit request-body, rate-limit and CORS policy;
- bounded transaction, contract, asset, event and proof-query surfaces;
- no credentials, seeds, mnemonics or private keys in static configuration or evidence.

### External miner

- exact packaged v3.0.0 CPU/GPU miner for the supported target;
- connects only to the intended network endpoint;
- network identity mismatch fails closed;
- deterministic template economics include ordinary and programmable fees;
- standalone miner remains free of pool payout/share-accounting/vardiff logic.

### Wallet / relay

- exact packaged v3.0.0 wallet;
- encrypted local custody only;
- no raw private keys/seeds/passwords over public RPC;
- relay accepts only the supported signed-transaction contract;
- wallet verifies chain/network identity before sign/broadcast;
- wallet supports the frozen v3 transaction, asset and contract semantics in launch scope.

## Final rehearsal matrix

Run against the frozen integrated v3 candidate and production-like identities/configuration without exposing the official networks prematurely:

- >=25-node / >=16-miner multi-node/multi-miner convergence;
- CPU/NVIDIA/AMD correctness and mixed supported GPU scenarios;
- competing/parallel-parent pressure and deterministic state/order checks;
- million-block deterministic replay spot/reproduction checks;
- programmable-operation deterministic replay spot/reproduction checks;
- seed and ordinary-node restart;
- miner/GPU worker restart/disconnect/rejoin;
- bounded node isolation/rejoin;
- delayed-parent/missing-history recovery;
- snapshot export/verify/restore;
- compact prune and post-prune recovery;
- clean-node bootstrap/catch-up;
- rolling-upgrade/live-activation rehearsal;
- wallet create/restore/sign/broadcast/reconcile;
- asset mint/burn/transfer flows in accepted scope;
- covenant/PulseScript/VM/contract execution and state/event reconciliation;
- based/verifiable-application and proof verification where included;
- public-safe RPC abuse/body/rate-limit/CORS checks;
- dependency/security and secret-scanning closeout;
- resource/disk/RocksDB/P2P/RPC/event/contract/GPU latency and saturation monitoring.

## GO review

#781 must record exactly one decision:

- `GO_V3_DUAL_LAUNCH`
- `DELAY_V3_DUAL_LAUNCH`
- `NO_GO_V3_DUAL_LAUNCH`

A GO is valid only for the exact frozen integrated candidate, artifact set, protocol/contract/economic identity and the two recorded network identities.

## Coordinated launch sequence

After GO:

1. verify release/artifact/protocol/contract/config/genesis hashes on every host;
2. start mainnet seed nodes and verify persistent identities;
3. start parallel-testnet seed nodes and verify independent identities;
4. start ordinary/observer nodes for both networks;
5. verify peer/signing/application-domain separation and no cross-network compatibility;
6. start approved CPU/NVIDIA/AMD miners for each network as applicable;
7. verify independent block production, selected-tip/state convergence and submit flow;
8. bring up official public RPC/status/event endpoints;
9. verify wallet/relay transfer flows against intended network identities;
10. verify assets, covenants, PulseScript/VM contracts and state/event surfaces;
11. verify based/verifiable applications and proof-verification surfaces where included;
12. record first accepted mainnet block/height and UTC timestamp;
13. record first accepted parallel-testnet block/height and UTC timestamp;
14. publish exact binaries/checksums, network identities, bootnodes/endpoints, wallet/application tooling, known limitations, operator/user instructions and incident/security routes.

The two recorded timestamps may differ operationally by minutes; they belong to one coordinated release window and neither requires a prior public-testnet acceptance clock.

## Hard-stop / rollback conditions

Delay or stop rollout on:

- consensus/state/order or contract/application-state divergence;
- chain/genesis/config/artifact/protocol/VM/proof digest mismatch;
- accidental mainnet/testnet peer, signing or application-domain compatibility;
- unexplained data loss or restore failure;
- persistent sync/rejoin failure;
- unresolved submit/finality incoherence;
- CPU/GPU PoW result disagreement;
- deterministic contract/proof replay disagreement;
- reachable security blocker without approved disposition;
- wallet custody/signing/network-domain defect;
- unbounded contract/resource behavior outside frozen policy;
- loss of monitoring, operator control or rollback capability.

Rollback actions and incident timelines must be recorded against the exact affected network and release identity.

## Post-launch

- heightened monitoring for the first 24 hours and first week;
- retain parallel testnet as the permanent public upgrade, contract/proof and application validation network;
- future consensus/network/contract/proof changes rehearse on parallel testnet before separately authorized mainnet activation;
- no silent protocol, VM, proof-system, fee or economic-policy mutation after launch.
