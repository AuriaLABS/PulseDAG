# PulseDAG v3.0.0 roadmap and gates

Status: **AUTHORITATIVE Q4 2026 LAUNCH TARGET**

This document is the definitive PulseDAG v3.0.0 roadmap. It absorbs the approved technical scope previously described in the v2.5.0 scale/resilience roadmap and the v2.6.0 programmability roadmap, while replacing their obsolete standalone-public-testnet sequencing with the current coordinated launch model.

## Launch decision

PulseDAG targets **v3.0.0** as the definitive public-launch release in **Q4 2026 (October-December 2026)**.

The launch model is:

- launch **mainnet and a parallel public testnet in the same coordinated release window**;
- do **not** launch a standalone public testnet first;
- do **not** require a 30-day public-testnet acceptance clock before mainnet;
- complete the scale/resilience/GPU-mining requirements formerly planned for v2.5.0 as part of v3.0.0;
- complete the programmability/smart-contract requirements formerly planned for v2.6.0 as part of v3.0.0;
- keep private/dev/rehearsal networks as pre-launch engineering and regression evidence;
- freeze independent mainnet and testnet network identities/genesis/configuration while tying implementation provenance to one exact v3.0.0 candidate;
- authorize launch only through issue #781 after every mandatory v3.0.0 gate passes.

No exact date inside Q4 is authorized here. The final UTC launch window is recorded only after readiness review.

## Version and scope policy

- v2.4.x is the current development/validation base and historical evidence source.
- The approved **v2.5.0 technical scope is incorporated into v3.0.0** as the scale, public-network resilience and production-GPU-mining workstream.
- The approved **v2.6.0 technical scope is incorporated into v3.0.0** as the programmability, smart-contract, verifiable-application and programmable-economics workstream.
- v2.5.0 and v2.6.0 therefore remain useful requirement documents and implementation work packages, but their old independent public-launch/GO sequencing does not control the definitive launch.
- The definitive public launch identity is **v3.0.0**.
- Existing v2.4.x tags, binaries, artifacts and evidence must never be relabeled as v3.0.0.
- `VERSION`, Cargo versions and the v3.0.0 tag are frozen only on the exact final candidate.
- Evidence from incompatible SHAs, dependency graphs, protocol activation contracts, signing domains, chain identities or genesis configurations must not be combined.

## Program authority

- #781 — sole final launch-control record for coordinated mainnet + parallel-testnet launch.
- #794 — integrated v3.0.0 implementation, release, infrastructure and rehearsal completion program.
- #803 — dependency/security launch gate.
- #819 — production wallet/custody readiness gate.
- #789 — v2.4 operational evidence and regression input; not v3.0.0 launch authorization.
- `ROADMAP_V2_5_0.md` — source requirements for the v3 scale/resilience/GPU workstream.
- `ROADMAP_V2_6_0.md` — source requirements for the v3 programmability/smart-contract workstream.

# Mandatory v3.0.0 workstreams

## A. Core release, protocol and storage freeze

- freeze the final consensus, transaction, storage, snapshot, replay, sync, mining, wallet and programmability contracts;
- define every consensus-affecting change and activation rule explicitly;
- define storage/schema/snapshot migration and rollback boundaries;
- prevent silent reinterpretation of persisted or consensus data;
- define evidence invalidation rules after candidate-changing fixes;
- preserve deterministic consensus as the primary invariant.

## B. Scale, P2P and adversarial resilience — incorporated from v2.5.0

### P2P v3 and eclipse resistance

Mandatory outcomes:

- deterministic/observable peer scoring;
- inbound/outbound quotas and peer diversity;
- eclipse-resistance policy and failure-domain awareness;
- bounded CPU/RAM/disk/bandwidth cost per peer/activity class;
- handshake/inventory/orphan/request flood protection;
- bounded peer churn and deterministic penalty recovery;
- quality-aware sync-source selection;
- no unbounded resource amplification by one peer.

### Compact DAG relay

Mandatory outcomes:

- header/DAG-metadata-first announcements;
- compact-block reconstruction;
- transaction/inventory deduplication;
- parent-aware and parallel-parent relay;
- body-on-demand retrieval and bounded reconstruction state;
- safe full-block fallback;
- propagation/reconstruction/bandwidth metrics;
- identical canonical validation regardless of relay form.

### Fast sync, pruning v2 and state bootstrap

Mandatory outcomes:

- versioned snapshot manifests and state commitments;
- chunked/resumable/parallel snapshot transfer where safe;
- cryptographic snapshot verification;
- pruning-aware DAG frontier/bootstrap;
- clean-node catch-up and restore/rejoin across supported storage states;
- snapshot poisoning/incompatibility rejection;
- post-bootstrap verification/replay checks;
- bounded long-term storage profiles.

### Deterministic mempool v3 and fee market

Mandatory outcomes:

- deterministic fee-rate policy and fee estimation;
- hard memory/resource limits;
- deterministic eviction/expiry;
- replacement/RBF semantics consistent with the frozen transaction protocol;
- ancestor/descendant/package limits;
- conflict-set handling and anti-spam resource pricing;
- restart reconstruction and DAG-reordering reconciliation;
- order-independent final mempool state for equivalent inputs.

## C. Mining Protocol v3 and production GPU mining — incorporated from v2.5.0

The miner remains an external application. No pool accounting/payout logic is embedded into the node or canonical standalone miner.

Mandatory outcomes:

- versioned mining jobs and template sequence IDs;
- bounded push/equivalent new-work notification;
- template invalidation and target updates;
- selected-tip and parallel-parent awareness;
- stable submission identity and deterministic submit reconciliation;
- explicit accepted/rejected/stale/unknown-finality states;
- multiple miners per node and bounded backpressure;
- end-to-end template-to-submit telemetry.

### GPU release targets

Both major GPU families are mandatory for the v3 production-mining claim:

- NVIDIA CUDA backend;
- AMD/ATI production backend on the supported AMD compute stack;
- CPU reference PoW remains the correctness oracle;
- CPU ↔ NVIDIA golden vectors;
- CPU ↔ AMD golden vectors;
- NVIDIA ↔ AMD same-input deterministic vectors;
- nonce/target boundary tests;
- single-GPU and multi-GPU correctness;
- Linux support for both families;
- Windows support where the selected production runtime/driver stack is supported;
- no GPU-specific consensus-validation shortcut in the node.

Final v3 evidence must record the equivalent of:

- `GPU_MINING_NVIDIA_PASS=true`;
- `GPU_MINING_AMD_PASS=true`.

### GPU runtime and hardening

- device discovery and explicit selection;
- homogeneous/heterogeneous multi-GPU scheduling;
- mixed AMD/NVIDIA operation where supported;
- deterministic nonce-space partitioning without intentional duplicate work;
- isolated workers, watchdog and device-failure recovery;
- work redistribution and reconnect/job refresh;
- per-device and aggregate metrics;
- temperature/power/clocks/utilization telemetry where safely available;
- architecture-specific profiling and reproducible benchmark mode;
- correctness/performance regression baselines;
- malformed-work, overflow, endianness and nonce-boundary tests;
- long-duration GPU soak and recovery tests.

## D. High-cadence operating envelope — incorporated from v2.5.0

Validate approximately:

- 1 second;
- 500 ms;
- 250 ms.

These are validation points, not automatic public defaults.

Measure propagation, orphan/merge-set pressure, DAG width, CPU/block, canonical-state apply latency, database amplification, template freshness, submit latency, stale rate, sync/finality behavior and miner fairness.

The final v3 public cadence is selected from evidence and may be more conservative than the fastest passing point.

## E. Deterministic replay, rolling upgrades, public API and chaos — incorporated from v2.5.0

### Million-block deterministic DAG replay

At least 1,000,000 valid DAG blocks must reproduce byte-identical selected parents, selected-tip/chain metadata, blue/red classification, canonical DAG order, transaction outcomes, UTXO/state and final state digest across varied valid arrival permutations, restarts, snapshots, pruning, forks and parallel blocks.

### Rolling upgrades/live activation

- capability/version negotiation;
- canary/staged rollout mechanics;
- explicit activation point where required;
- storage/snapshot migration;
- safe pre-activation rollback;
- fail-closed post-activation downgrade when semantics cannot revert;
- version-distribution/compatibility observability;
- no full-network shutdown as a normal upgrade procedure.

### Public RPC/API v3

- stable versioned API/schema contract;
- machine-readable API contract where practical;
- deterministic error codes;
- request IDs, pagination and payload/query bounds;
- rate-limit metadata;
- event streaming for blocks, selected-tip/DAG changes, transaction/mempool state, sync state and mining-job invalidation;
- strict public vs operator/admin separation.

### Automated chaos matrix

Automate and retain evidence for node/miner process loss, GPU loss, disk pressure/latency, packet loss, latency, peer churn, seed loss, partitions, bounded clock skew, delayed-parent/orphan storms, RPC load, snapshot recovery and prune/rejoin.

Every scenario emits a reviewable timeline, metrics, convergence/state digests, incident record and PASS/FAIL.

## F. Reproducible supply chain and large-scale rehearsal — incorporated from v2.5.0

Mandatory outcomes:

- reproducible or independently verifiable release builds;
- SBOM and artifact provenance;
- signed/attested release manifests where supported;
- dependency/security audit gates;
- least-privilege workflows;
- exact source-SHA-linked node/miner/wallet artifacts;
- clean-machine package smoke tests;
- recorded binary and network-config digests.

Final private/release-candidate rehearsal target:

- at least 25 real nodes;
- at least 16 external miners;
- NVIDIA, AMD/ATI, multi-GPU and supported mixed-GPU scenarios;
- at least three independent failure domains;
- multiple regions/providers where practical;
- injected partitions, node/miner/GPU churn, seed loss, stale templates, target changes, orphan storms, RPC load, snapshot/bootstrap/pruning, offline/rejoin and rolling-upgrade periods;
- no unexplained canonical divergence after recovery.

## G. Exact-candidate pre-launch burn-in — adapted from v2.5.0

Run at least **168 contiguous hours** on one unchanged v3.0.0 release candidate before final GO.

Continuously record block production, DAG width, selected-tip changes, orphan/merge-set pressure, CPU, RAM, disk, bandwidth, peer state, mempool, state digest, miner/GPU telemetry, template/submit latency, stale rate, snapshot/pruning activity and incidents.

A candidate-changing fix invalidates affected burn-in evidence and requires explicit rebaseline.

This 168-hour release-candidate burn-in replaces the old v2.5 standalone-public-testnet canary/30-day prerequisite for the initial mainnet launch. The permanent parallel public testnet begins with the coordinated v3 launch.

## H. Programmability activation contract — incorporated from v2.6.0

Programmability is part of the v3.0.0 scope and must be frozen before launch.

Mandatory outcomes:

- transaction/script/contract versioning rules;
- explicit activation semantics and mixed-version behavior;
- chain/network/domain separation;
- storage/snapshot/pruning compatibility;
- rollback/downgrade boundary;
- deterministic execution/resource accounting model;
- contract/application state-commitment model;
- proof-system versioning policy;
- evidence invalidation rules;
- no downstream component may invent conflicting activation semantics.

## I. UTXO covenants and Contract Transaction v3 — incorporated from v2.6.0

### UTXO Covenants v1

Provide bounded deterministic programmable spending conditions for approved capabilities such as advanced timelocks, vault policies, programmable multisignature, escrow, atomic-swap/payment-channel primitives, controlled asset rules and successor-output/state-continuation rules.

Covenants must be deterministic, resource bounded and unable to access non-deterministic host resources.

### Contract Transaction v3

Freeze a versioned programmable transaction format with, as applicable:

- covenant/contract inputs and outputs;
- execution/state commitments;
- application/contract namespace and version;
- payload commitment;
- declared resource budgets;
- proof commitments;
- stable submission identity;
- canonical serialization/signing/txid derivation;
- chain/network/domain binding;
- deterministic rejection/replay rules.

## J. PulseScript and deterministic contract VM — incorporated from v2.6.0

### PulseScript

- deterministic semantics;
- static typing;
- no floating-point consensus behavior;
- explicit integer/overflow semantics;
- bounded/analyzable resource constructs;
- reproducible compiler output;
- versioned compiler/bytecode/script format;
- ABI/interface description;
- golden compilation vectors.

### Deterministic Contract VM

- deterministic arithmetic/memory/stack behavior;
- instruction/compute metering;
- memory, stack, recursion/call-depth and state-access limits;
- deterministic failure codes;
- no filesystem, external networking, wall-clock, host randomness or hardware-dependent consensus semantics;
- reproducible execution across supported architectures;
- differential vectors across implementations/platforms.

## K. Parallel DAG contract execution — incorporated from v2.6.0

- deterministic read/write set or equivalent conflict declaration;
- parallel execution only for non-conflicting transitions;
- canonical conflict resolution through accepted DAG order;
- deterministic rollback/re-execution when ordering changes in the non-final region;
- identical final application state regardless of arrival order or local thread scheduling;
- bounded scheduler complexity.

## L. Based Applications, PulseProgs and ZK verification — incorporated from v2.6.0

### Based Applications

- versioned application identity;
- canonical L1 operation ordering;
- state-transition commitments;
- settlement/finality semantics;
- explicit data-availability assumptions;
- fraud/validity-proof policy as applicable;
- deterministic rejection of malformed/incompatible commitments;
- no hidden trusted sequencer requirement unless explicitly declared.

### PulseProgs / Verifiable Programs

Separate and version program identity, state commitments, scheduling, runtime, proof/verification interface, storage/indexing and rollback/canonical-state transitions.

### ZK verification layer

Where included in the frozen v3 scope:

- proof-system/version identifiers;
- verification-key commitments/versioning;
- proof-size and verification-cost limits;
- state-root/transition commitments;
- chain/network/application domain separation;
- replay protection;
- golden proof/verification vectors;
- fail-closed handling of unknown proof systems/versions.

## M. Native assets, contract state/events and programmable fee economy — incorporated from v2.6.0

### Native assets/token standards

Support deterministic consensus-backed profiles for fungible assets, unique/non-fungible assets and multi-asset collections, including deterministic mint/burn/transfer, declared supply caps, ownership/spending rules, metadata commitments, application events and replayable total-supply accounting.

No compatibility with external token/VM ecosystems is implied unless separately specified and tested.

### Contract state/events/RPC

- bounded state/commitment queries;
- execution/history references;
- event streaming;
- asset/contract transaction status;
- bounded pagination/resource limits;
- indexer-friendly canonical feeds;
- separation of consensus-required state from optional heavy indexes.

### Resource and fee economy

Meter and bound relevant resource classes independently where appropriate, including transaction bytes, compute, memory, state reads/writes/growth and proof verification.

## N. Contract security and programmable mining economics — incorporated from v2.6.0

Mandatory adversarial coverage:

- malformed bytecode/script;
- integer/serialization edge cases;
- replay/domain attacks;
- reentrancy/call-graph hazards where applicable;
- state-conflict/order attacks;
- resource exhaustion and state-growth attacks;
- malformed/oversized proof attacks;
- recursive/cross-contract amplification;
- DAG reorder/non-final rollback behavior;
- compiler/runtime differential fuzzing;
- property-based state-transition testing;
- adversarial contract corpus/regressions.

The external-miner template path must deterministically account for:

- block subsidy;
- ordinary transaction fees;
- programmable compute/state fees;
- proof-verification fees where applicable.

Pool membership, share accounting, vardiff, worker management and payouts remain external third-party infrastructure.

## O. Monetary/economic policy freeze — incorporated from v2.6.0

Before final v3 launch, freeze and test the approved production monetary/economic policy:

- maximum/target supply model as applicable;
- emission curve;
- block reward/reduction schedule;
- coinbase maturity;
- base transaction fee rules;
- programmable resource fees;
- burn/recycling behavior only if explicitly approved;
- exact total-supply calculation at arbitrary accepted state;
- no hidden or implementation-dependent issuance path.

## P. Programmability validation and replay — incorporated from v2.6.0

The pre-launch validation program must exercise:

- covenants;
- PulseScript;
- deterministic VM execution;
- tokens/assets;
- parallel independent contracts;
- intentional state conflicts;
- invalid/malformed contracts;
- verifiable programs/based applications;
- ZK proof verification where included;
- large state/state-growth pressure;
- spam/resource-exhaustion attempts;
- AMD/NVIDIA production mining;
- snapshots, pruning, restart/rejoin and rolling upgrades.

At least **1,000,000 programmable transactions/operations** must replay to byte-identical canonical DAG/order, contract outcomes, covenant outputs, asset supply/ownership state, application/state commitments, proof-verification outcomes and final canonical state digest across varied valid arrival/scheduling permutations.

## Q. Programmability burn-in — adapted from v2.6.0

The v3 release program retains the strong long-duration programmability requirement from the v2.6 roadmap, but it is executed as pre-launch exact-candidate validation rather than as a prerequisite standalone public-testnet phase.

Before final GO, accumulate **30 accepted days of programmability-enabled exact-candidate burn-in evidence** across controlled private/release-candidate infrastructure. The evidence window may include continuous or explicitly policy-approved accumulated periods, but invalidated time may not be silently counted.

Exercise sustained normal transactions, contracts, tokens, verifiable applications/proofs, AMD/NVIDIA mining, pruning, snapshots, restarts, node rejoin, rolling upgrades and controlled failure scenarios.

No unexplained consensus/application-state divergence or unresolved release-blocking Sev-1 issue may remain.

## R. Production wallet/custody boundary

Complete #819 on the same exact v3 candidate:

- encrypted deterministic custody;
- backup verification and clean-machine recovery;
- bounded lock/unlock and secret lifecycle;
- deterministic safe transaction construction;
- fee/UTXO/pending/reconciliation policy;
- mainnet/testnet network-domain separation;
- offline signing/watch-only flow;
- signed-transaction-only public relay;
- packaged create/restore/sign/send/reconcile validation;
- no raw private-key/mnemonic/password custody over public node RPC.

Wallet/application tooling must understand the frozen programmable transaction and asset/contract semantics included in v3.

## S. Dependency, workflow and release security

Complete #803 on the exact v3 candidate:

- no unresolved reachable security blocker without explicit reviewed mainnet/public disposition;
- exact compiler-artifact dependency reachability on supported targets;
- warning/advisory ownership and drift control;
- workflow least privilege;
- secret scanning;
- artifact provenance/SBOM/attestation where supported;
- wallet/contract/proof dependencies included in the same security matrix.

## T. Freeze one exact v3.0.0 release candidate

Record:

- exact source SHA/tree;
- `VERSION`/Cargo/release metadata;
- protocol/transaction/contract/VM/proof/storage versions;
- reproducible node/miner/wallet artifacts and digests;
- GPU backend/package identities;
- activation contract;
- storage compatibility boundary;
- supply/economic policy digest;
- evidence bundle identity.

## U. Freeze two independent public network identities

### Mainnet

- chain ID;
- network profile;
- genesis;
- consensus/activation/config digest;
- bootnodes/peer IDs;
- DNS/RPC/status endpoints.

### Parallel public testnet

- independent chain ID;
- independent network profile;
- independent genesis;
- independent consensus/activation/config digest as appropriate;
- independent bootnodes/peer IDs;
- separate DNS/RPC/status endpoints.

The networks must not share genesis, chain ID, wallet/signing domain, bootnode identity or accidental peer compatibility.

## V. Production launch readiness review

Verify:

- infrastructure, backups, NTP, firewall, TLS/DNS and monitoring;
- primary and backup operators;
- launch/on-call/rollback window;
- status/incident communication path;
- public-safe RPC and private admin/operator surfaces;
- final exact-candidate evidence for scale, GPU, replay, programmability, wallet and security;
- rollback/hard-stop plan.

## W. Single coordinated decision in #781

Record exactly one:

- `GO_V3_DUAL_LAUNCH`;
- `DELAY_V3_DUAL_LAUNCH`;
- `NO_GO_V3_DUAL_LAUNCH`.

GO applies only to the exact frozen v3.0.0 artifacts, protocol/programmability identity and two frozen network identities.

## X. Coordinated mainnet + parallel-testnet launch

After GO:

1. verify exact artifacts/config/genesis on every host;
2. start and verify independent seed meshes;
3. start ordinary/observer nodes for both networks;
4. verify network identity separation and no cross-network compatibility;
5. start approved CPU/NVIDIA/AMD miners as applicable;
6. verify independent block production, selected-tip/state convergence and submit reconciliation;
7. bring up public RPC/status/event surfaces;
8. verify wallet transaction, asset and contract flows against the correct network identity;
9. verify contract execution/state/event/proof surfaces included in v3;
10. record first accepted mainnet block/height + UTC timestamp;
11. record first accepted parallel-testnet block/height + UTC timestamp;
12. publish binaries/checksums, network identities, bootnodes/endpoints, wallet/application tooling, known limitations and incident/security routes.

The two first-block timestamps may differ by operational minutes; they belong to one coordinated release window.

## Y. Post-launch stabilization

- enhanced first-24h and first-week monitoring;
- retain parallel testnet permanently for upgrade and application validation;
- incident/rollback/recovery recording by network;
- future consensus/contract/proof-system changes rehearse on testnet and require separately versioned activation decisions;
- no silent activation or semantic mutation after launch.

# Removed legacy sequencing

The following do **not** control the v3.0.0 launch:

- the 1 September 2026 standalone-public-testnet target;
- `GO_PUBLIC_TESTNET` as final project authorization;
- a 30-day public-testnet clock before mainnet;
- the v2.5 Task 49 standalone public-testnet canary/30-day prerequisite;
- the v2.6 dependency on completion of that standalone-public-testnet period before programmability work begins.

The **technical** v2.5 and v2.6 requirements remain incorporated above. Only their obsolete pre-mainnet public-testnet sequencing is removed.

Legacy runtime/config fields such as `public_testnet_ready` and `thirty_day_public_testnet_clock_started` may remain temporarily for v2.4.x compatibility/historical validation, but they are not v3.0.0 launch authority.

# Completion rule

v3.0.0 is complete only when:

- all incorporated scale/resilience/GPU requirements from v2.5 are complete or formally dispositioned without weakening the mandatory v3 launch contract;
- all incorporated programmability/smart-contract/economic requirements from v2.6 are complete or formally dispositioned within the frozen v3 scope;
- the exact-candidate replay, 168-hour release burn-in and 30-day programmability evidence requirements pass;
- #803 and #819 pass for the exact candidate;
- independent mainnet/testnet identities are frozen;
- #781 records the final decision and, after GO, actual launch boundaries/timestamps for both networks.
