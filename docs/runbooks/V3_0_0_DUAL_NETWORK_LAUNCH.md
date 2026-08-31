# v3.0.0 coordinated mainnet + parallel-testnet launch runbook

Status: **PRE-GO / Q4 2026 TARGET / NOT LAUNCHED**

Authority: issue #781 is the sole final launch-control record. This runbook implements the integrated sequencing from `docs/ROADMAP_V3_0_0.md`; it cannot set launch GO by itself.

## Integrated release path

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

The technical acceptance criteria from `ROADMAP_V2_5_0.md` and `ROADMAP_V2_6_0.md` are mandatory inputs to the v3.0.0 GO decision.

The production economic/network authority is additionally defined by:

- `docs/MONETARY_POLICY_V3_0_0.md`;
- `docs/GENESIS_V3_0_0.md`;
- `docs/NETWORK_PARAMETERS_V3_0_0.md`;
- `docs/runbooks/V3_0_0_GENESIS_CEREMONY.md`;
- `docs/V3_0_0_LAUNCH_MANIFEST.md`.

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
14. `docs/MONETARY_POLICY_V3_0_0.md` has an explicitly approved production policy; no development constant or genesis allocation is implicitly promoted;
15. the canonical DAG reward/emission index, subsidy schedule, coinbase maturity, fee disposition, burn rule and terminal/max-supply rule are frozen and covered by deterministic vectors;
16. mainnet genesis issuance/allocation exactly matches the approved monetary-policy manifest; no placeholder destination such as `genesis-treasury` remains;
17. the production genesis uses an exact frozen timestamp and deterministic inputs rather than runtime `current_ts()` or operator-local defaults;
18. the independent genesis ceremony in `V3_0_0_GENESIS_CEREMONY.md` is complete and clean reproductions are byte-identical;
19. independent mainnet and testnet chain IDs, network/signing domains, network profiles, genesis hashes and consensus/activation/config digests are frozen;
20. independent mainnet and testnet bootnodes/peer IDs/multiaddrs are frozen;
21. DNS/TLS/status/RPC/event endpoint ownership and firewall policy are recorded separately for both networks;
22. `docs/V3_0_0_LAUNCH_MANIFEST.md` has `Launch state: **FROZEN**`, contains no launch-required `TBD`, and all mandatory network-separation assertions are `PASS`;
23. `scripts/validate_v3_0_0_network_freeze.py` reports `launch_ready=true` on the exact candidate;
24. observability, evidence export and incident bundle collection are verified on all launch roles;
25. primary and backup operators plus UTC launch/on-call/rollback window are recorded;
26. explicit `GO_V3_DUAL_LAUNCH` is recorded in #781 for those exact artifacts, policy, genesis identities and configurations.

## Identity-separation requirements

Mainnet and testnet must be impossible to confuse accidentally.

- different chain IDs;
- different network profiles;
- different network/signing domains;
- different genesis blocks/hashes;
- domain-separated wallet/signing identity;
- contract/application/proof domains bound to the intended network;
- independent persistent bootnode peer identities;
- separate DNS/status/RPC/event endpoints;
- no default cross-network bootstrapping;
- network mismatch fails closed in node, miner, wallet and application flows.

The final identities remain `TBD` until freeze. Do not invent production chain IDs, genesis values, peer IDs, addresses or DNS names in committed templates.

## Monetary and genesis freeze

The current v2.4-derived implementation is only a development baseline. Its existing genesis supply/allocation, subsidy constants and runtime timestamp behavior are not automatically authorized for mainnet.

Before final freeze:

- approve or replace every monetary baseline in `MONETARY_POLICY_V3_0_0.md`;
- implement a canonical DAG reward/emission index with unambiguous consensus semantics;
- implement and test the approved coinbase-maturity rule;
- produce independent total-supply accounting and transition vectors;
- remove production dependence on the `genesis-treasury` placeholder;
- replace runtime genesis timestamp selection with the exact ceremony timestamp;
- generate mainnet and testnet genesis independently from immutable input manifests;
- reproduce each genesis byte-for-byte in independent clean environments;
- bind all resulting digests into `V3_0_0_LAUNCH_MANIFEST.md`.

A one-atomic-unit issuance mismatch, unreproducible genesis, hidden allocation or cross-network identity collision is a hard stop.

## Release freeze

Freeze one exact integrated v3.0.0 source candidate before final launch rehearsal. Record in `docs/V3_0_0_LAUNCH_MANIFEST.md`:

- exact source SHA and tree;
- VERSION and Cargo workspace/package versions;
- protocol/transaction activation contract;
- contract/VM/proof-system versions;
- storage/schema/snapshot compatibility identity;
- production monetary/economic policy digest and emission-vector digest;
- genesis allocation manifest and supply-accounting digests;
- Linux/Windows/macOS package identities as supported;
- CPU/NVIDIA/AMD miner package identities as supported;
- extracted binary hashes;
- container/image digests where used;
- SBOM/provenance/attestation where available;
- mainnet canonical genesis/config/network digests;
- parallel-testnet canonical genesis/config/network digests;
- bootnode/DNS/public-endpoint manifests;
- wallet/network-domain manifest;
- security/rehearsal/burn-in evidence references.

Any release-affecting code, dependency, protocol, storage, GPU, wallet-signing, monetary, genesis, contract, VM, proof or activation change after freeze invalidates affected evidence and requires explicit rebaseline. Any changed genesis input creates a new network identity.

## Infrastructure roles

### Seed / bootnode

- persistent P2P identity outside release directories;
- public P2P only;
- RPC/admin remains loopback or private management;
- no wallet custody on seed nodes;
- record actual live `/p2p/<peer-id>` multiaddr;
- run only against the exact frozen network/genesis/config identity.

### Ordinary node

- persistent identity and RocksDB volume;
- bootstraps only from the frozen network-specific bootnode set;
- admin disabled on public-facing process;
- backup/snapshot/recovery policy enabled and tested;
- contract/state/proof execution remains bounded by the frozen v3 resource model;
- reports/exports exact chain ID, genesis and config identity for operator verification.

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
- deterministic template economics include the approved subsidy plus eligible ordinary/program/proof fees;
- reward output obeys the frozen coinbase and maturity contract;
- standalone miner remains free of pool payout/share-accounting/vardiff logic.

### Wallet / relay

- exact packaged v3.0.0 wallet;
- encrypted local custody only;
- no raw private keys/seeds/passwords over public RPC;
- relay accepts only the supported signed-transaction contract;
- wallet verifies chain/network identity before sign/broadcast;
- wallet displays/derives the correct frozen network address/domain identity;
- wallet supports the frozen v3 transaction, asset and contract semantics in launch scope.

## Final rehearsal matrix

Run against the frozen integrated v3 candidate and production-like identities/configuration without exposing the official networks prematurely:

- >=25-node / >=16-miner multi-node/multi-miner convergence;
- CPU/NVIDIA/AMD correctness and mixed supported GPU scenarios;
- competing/parallel-parent pressure and deterministic state/order checks;
- million-block deterministic replay spot/reproduction checks;
- programmable-operation deterministic replay spot/reproduction checks;
- monetary supply/reward/fee/maturity vector verification;
- genesis/config/network identity verification on every role;
- seed and ordinary-node restart;
- miner/GPU worker restart/disconnect/rejoin;
- bounded node isolation/rejoin;
- delayed-parent/missing-history recovery;
- snapshot export/verify/restore;
- compact prune and post-prune recovery;
- clean-node bootstrap/catch-up;
- rolling-upgrade/live-activation rehearsal;
- wallet create/restore/sign/broadcast/reconcile;
- cross-network node/miner/wallet/application rejection tests;
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

A GO is valid only for the exact frozen integrated candidate, artifact set, monetary policy, genesis/config identities, protocol/contract identity and the two recorded network identities. `PRE_FREEZE` or `launch_ready=false` is an automatic no-GO condition.

## Coordinated launch sequence

After GO:

1. verify release/artifact/protocol/contract/monetary/config/genesis hashes on every host against `V3_0_0_LAUNCH_MANIFEST.md`;
2. verify the launch manifest is still `FROZEN` and the freeze validator reports `launch_ready=true`;
3. start mainnet seed nodes and verify persistent identities;
4. start parallel-testnet seed nodes and verify independent identities;
5. start ordinary/observer nodes for both networks;
6. verify peer/signing/application-domain separation and no cross-network compatibility;
7. start approved CPU/NVIDIA/AMD miners for each network as applicable;
8. verify independent block production, reward/fee accounting, selected-tip/state convergence and submit flow;
9. bring up official public RPC/status/event endpoints;
10. verify wallet/relay transfer flows against intended network identities;
11. verify assets, covenants, PulseScript/VM contracts and state/event surfaces;
12. verify based/verifiable applications and proof-verification surfaces where included;
13. record first accepted mainnet block/height-or-score and UTC timestamp;
14. record first accepted parallel-testnet block/height-or-score and UTC timestamp;
15. publish exact binaries/checksums, monetary policy, genesis/network identities, bootnodes/endpoints, wallet/application tooling, known limitations, operator/user instructions and incident/security routes.

The two recorded timestamps may differ operationally by minutes; they belong to one coordinated release window and neither requires a prior public-testnet acceptance clock.

## Hard-stop / rollback conditions

Delay or stop rollout on:

- consensus/state/order or contract/application-state divergence;
- chain/genesis/config/artifact/protocol/VM/proof/monetary digest mismatch;
- genesis or reward issuance mismatch;
- placeholder/undocumented genesis allocation;
- accidental mainnet/testnet peer, signing or application-domain compatibility;
- unexplained data loss or restore failure;
- persistent sync/rejoin failure;
- unresolved submit/finality incoherence;
- CPU/GPU PoW result disagreement;
- deterministic contract/proof replay disagreement;
- reachable security blocker without approved disposition;
- wallet custody/signing/network-domain defect;
- unbounded contract/resource behavior outside frozen policy;
- `launch_ready=false`, a non-`FROZEN` launch manifest or any required `TBD` reappearing;
- loss of monitoring, operator control or rollback capability.

Rollback actions and incident timelines must be recorded against the exact affected network and release identity.

## Post-launch

- heightened monitoring for the first 24 hours and first week;
- continuously reconcile issued supply/reward/fee accounting against the frozen economic policy;
- retain parallel testnet as the permanent public upgrade, contract/proof and application validation network;
- future consensus/network/contract/proof changes rehearse on parallel testnet before separately authorized mainnet activation;
- no silent protocol, VM, proof-system, fee, genesis or economic-policy mutation after launch.
