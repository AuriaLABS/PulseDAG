# ROADMAP v2.5.0 — Network Scale, Production GPU Mining and Adversarial Resilience

Status: **APPROVED FUTURE ROADMAP**

Approval date: 2026-08-17 UTC

## Purpose

v2.5.0 is the scale-and-resilience release after the v2.4.0 protocol/consensus expansion. Its purpose is to prove that PulseDAG can operate as a high-cadence public network under sustained load, hostile network conditions, large DAG history, real GPU mining and rolling upgrades without weakening deterministic consensus or operator safety.

This approval does not authorize a v2.5.0 version bump, tag, release, protocol activation or public-network launch. v2.4.0 remains the required predecessor and its release/activation and launch-control gates remain authoritative until completed.

## Non-negotiable principles

- Consensus determinism remains the primary invariant.
- The miner remains an external application; mining logic is not embedded into the node.
- Production GPU mining must support both **NVIDIA** and **AMD/ATI** as first-class release targets.
- NVIDIA-only or AMD-only completion is insufficient for `GO_V2_5_0`.
- CPU reference PoW remains the canonical correctness oracle for GPU implementations.
- Ordinary multi-node safety remains fail-closed.
- High-cadence operation is promoted only from measured evidence, never from an assumed target interval.
- Storage, snapshot, pruning and sync changes require explicit compatibility/version boundaries.
- Rolling upgrades must not require intentionally stopping the full network.
- No pool server, pool accounting or payout logic is added to the node or official miner.
- No smart-contract implementation is included in v2.5.0.

## Dependency spine

```text
v2.4.0 final technical gate
        |
        v
Task 32
  |-------------------------------|
  v                               v
Tasks 33-36                   Tasks 37-40
Network/state                 Mining/GPU
  |                               |
  +---------------+---------------+
                  v
               Task 41
                  |
                  v
               Task 42
                  |
                  v
               Task 43
                  |
                  v
            Tasks 44-46
                  |
                  v
               Task 47
                  |
                  v
               Task 48
                  |
                  v
               Task 49
                  |
                  v
               Task 50
```

## Tasks

### Task 32 — v2.5.0 entry gate and protocol freeze

Freeze the v2.5.0 scope and compatibility contract before downstream implementation.

Required outcomes:

- exact accepted v2.4.0 baseline SHA/protocol identity;
- complete v2.5.0 feature inventory;
- capability/version negotiation rules;
- storage and snapshot compatibility boundaries;
- activation and rollback/downgrade rules;
- evidence invalidation rules;
- explicit definition of which changes are consensus-affecting;
- no silent reinterpretation of v2.4.0 persisted or consensus data.

### Task 33 — P2P v3 and eclipse resistance

Harden the peer layer for hostile public operation.

Required outcomes include:

- deterministic/observable peer scoring;
- inbound/outbound quotas;
- peer diversity and failure-domain awareness;
- eclipse-resistance policy;
- CPU, RAM, disk and bandwidth budgets per peer/activity class;
- connection, handshake, inventory, orphan and request flood protection;
- temporary penalties with deterministic recovery;
- bounded peer churn;
- quality-aware sync-source selection;
- no peer can create unbounded resource amplification.

### Task 34 — Compact DAG relay

Reduce propagation cost while preserving canonical validation.

Required outcomes:

- header/DAG-metadata-first announcements;
- compact block reconstruction;
- transaction and inventory deduplication;
- parent-aware and parallel-parent relay;
- body-on-demand retrieval;
- bounded reconstruction state;
- safe full-block fallback;
- propagation, reconstruction and bandwidth metrics;
- identical validation semantics regardless of relay form.

### Task 35 — Fast sync, pruning v2 and state bootstrap

Make mature-network node bootstrap practical without introducing trusted state.

Required outcomes:

- versioned snapshot manifests and state commitments;
- chunked, resumable and parallel snapshot transfer where safe;
- cryptographic snapshot verification;
- pruning-aware DAG frontier/bootstrap;
- clean-node catch-up;
- restore and rejoin across supported storage states;
- snapshot poisoning/incompatibility rejection;
- post-bootstrap verification/replay checks;
- bounded long-term storage profiles.

### Task 36 — Deterministic mempool v3 and fee market

Introduce a bounded public-network mempool and fee policy.

Required outcomes:

- deterministic fee-rate policy and fee estimation;
- hard memory/resource limits;
- deterministic eviction and expiry;
- replacement/RBF semantics consistent with Transaction Protocol v2;
- ancestor/descendant and package limits;
- package-aware acceptance/relay where enabled;
- conflict-set handling;
- anti-spam resource pricing;
- restart reconstruction and DAG-reordering reconciliation;
- order-independent final mempool state for equivalent inputs.

### Task 37 — Mining Protocol v3

Upgrade the external node/miner contract for high-cadence production mining.

Required outcomes:

- versioned mining jobs and template sequence IDs;
- push or equivalent bounded notification of new work;
- template invalidation and target updates;
- selected-tip and parallel-parent awareness;
- stable submission identity;
- explicit accepted/rejected/stale/unknown-finality states;
- deterministic submit reconciliation;
- multiple miners per node;
- bounded backpressure;
- end-to-end template-to-submit telemetry.

### Task 38 — Production GPU mining: NVIDIA + AMD/ATI

Deliver canonical production GPU PoW for both major GPU families.

Mandatory release targets:

- NVIDIA CUDA backend;
- AMD/ATI production backend using the supported AMD compute stack for target platforms;
- canonical kHeavyHash-compatible PoW semantics identical to CPU validation;
- CPU ↔ NVIDIA golden vectors;
- CPU ↔ AMD golden vectors;
- NVIDIA ↔ AMD same-input deterministic vectors;
- nonce and target boundary tests;
- single-GPU and multi-GPU correctness;
- Linux support for both families;
- Windows support for both families where the selected production runtime/driver stack is supported;
- no GPU-specific consensus validation shortcut in the node.

`GPU_MINING_NVIDIA_PASS=true` and `GPU_MINING_AMD_PASS=true` are both mandatory for final v2.5.0 GO.

### Task 39 — GPU runtime, multi-GPU and device management

Turn GPU PoW into an operator-grade miner.

Required outcomes:

- device discovery and explicit selection;
- homogeneous and heterogeneous multi-GPU scheduling;
- mixed AMD/NVIDIA host operation where the OS/runtime combination supports it;
- deterministic nonce-space partitioning with zero intentional duplicate work;
- isolated workers per device;
- watchdog and device reset/failure recovery;
- automatic work redistribution;
- reconnect and job refresh after node/miner disruption;
- per-device configuration;
- per-device and aggregate hashrate, accepted/rejected/stale, error and uptime metrics;
- temperature, power, clocks and utilization telemetry where exposed safely by the vendor runtime.

### Task 40 — GPU kernel performance and hardening

Prove that the GPU implementation is both correct and production-worthy.

Required outcomes:

- architecture-specific kernel profiling for NVIDIA and AMD;
- occupancy/register/memory-access analysis;
- batching and work-size tuning;
- bounded job-change latency;
- low host-CPU overhead;
- reproducible benchmark mode;
- correctness regression CI;
- performance regression baselines;
- endianness, overflow, nonce-boundary and malformed-work tests;
- long-duration GPU soak tests;
- device-error and recovery tests;
- no performance optimization may change canonical PoW results.

### Task 41 — High-cadence protocol v2

Promote high-cadence work from an experiment into a measured protocol operating envelope.

Initial validation points must include approximately:

- 1 second;
- 500 ms;
- 250 ms.

These are test points, not automatically public defaults.

Measure at minimum propagation delay, orphan/merge-set pressure, DAG width, CPU/block, canonical-state apply latency, database amplification, template freshness, submit latency, stale rate, sync behavior, finality behavior and miner fairness.

The accepted public cadence must be selected from evidence and may be more conservative than the fastest passing test point.

### Task 42 — Million-block deterministic DAG replay

Validate at least 1,000,000 DAG blocks under deterministic replay and reconstruction.

The same valid DAG dataset must converge to byte-identical:

- selected parents;
- selected tip/chain metadata;
- blue/red classification;
- canonical DAG order;
- transaction outcomes;
- UTXO/state;
- state digest.

Validation must cover different block/peer/orphan arrival permutations, restarts, snapshots, pruning, forks and parallel blocks.

### Task 43 — Rolling upgrade and live activation

Prove that ordinary protocol/software upgrades do not require stopping the full network.

Required outcomes:

- supported v2.4/v2.5 mixed-version window;
- capability negotiation;
- canary and staged rollout;
- explicit activation point where required;
- storage/snapshot migration;
- pre-activation rollback;
- post-activation downgrade fail-closed where semantics cannot safely revert;
- version-distribution and compatibility observability;
- no network-wide shutdown as a normal upgrade procedure.

### Task 44 — Public RPC/API v3 and event streaming

Provide a stable bounded API surface for wallets, explorers and services.

Required outcomes:

- API versioning and schema contract;
- OpenAPI or equivalent machine-readable contract;
- deterministic error codes;
- request IDs, pagination and payload/query bounds;
- rate-limit metadata;
- event streaming for new blocks, selected-tip/DAG updates, transaction state, mempool state, sync state and mining-job invalidation;
- strict separation of public and administrative/operator surfaces.

### Task 45 — Observability v3 and automated chaos testing

Make failure injection a repeatable validation surface rather than a manual exercise.

Automated scenarios must include node/miner process loss, GPU-worker/device loss, disk pressure/latency, packet loss, latency, peer churn, seed loss, partitions, clock skew within the modeled threat envelope, orphan/delayed-parent storms, RPC load, snapshot recovery and prune/rejoin.

Each scenario must emit a reviewable timeline, metrics, state/convergence digests, incident record and PASS/FAIL result.

### Task 46 — Reproducible supply chain and release security

Required outcomes:

- reproducible or independently verifiable release builds;
- SBOM;
- artifact provenance;
- signed release manifests;
- dependency/security audit gates;
- least-privilege workflows;
- source-SHA-linked node/miner artifacts;
- Linux/Windows package validation for supported targets;
- separate CPU, NVIDIA and AMD miner artifacts where applicable;
- clean-machine native smoke tests;
- recorded hashes for binaries and network configuration used in evidence.

### Task 47 — 25-node / 16-miner / multi-GPU adversarial rehearsal

Run a large private distributed rehearsal using one exact candidate SHA.

Target topology:

- at least 25 real nodes;
- at least 16 external miner processes;
- NVIDIA, AMD/ATI, multi-GPU and mixed supported scenarios;
- at least three independent failure domains;
- multiple regions/providers where practical.

Inject partitions, node/miner/GPU churn, seed loss, stale templates, target changes, orphan storms, RPC load, snapshot/bootstrap/pruning, node offline/rejoin, rolling upgrade and high-cadence periods.

The final state must converge across all surviving/recovered nodes with no unexplained canonical divergence.

### Task 48 — Seven-day exact-SHA burn-in

Run at least 168 contiguous hours on one unchanged candidate SHA.

A candidate-changing fix invalidates the burn-in unless the evidence contract explicitly proves otherwise.

Continuously record block production, DAG width, selected-tip changes, orphan/merge-set pressure, CPU, RAM, disk, bandwidth, peer state, mempool, state digest, miner/GPU telemetry, template/submit latency, stale rate, snapshot/pruning activity and incidents.

No unresolved Sev-1 consensus, storage, replay, sync, mining, security or operator-safety issue may remain.

### Task 49 — Public-testnet v2.5 canary and 30-day acceptance

Promote v2.5 through a staged public-testnet rollout only after the private release gates pass.

Required rollout pattern:

```text
single canary -> small cohort -> 25% -> 50% -> majority -> full accepted cohort
```

After accepted activation, complete at least 30 accepted public-testnet days covering normal operation, AMD/NVIDIA mining, node joins/leaves, rolling upgrades, snapshot/pruning/restore, clean sync, incident drills, replay checks and state-digest verification.

Hard-stop incidents must follow the accepted evidence/clock policy; invalidated time may not be silently counted.

### Task 50 — v2.5.0 final release decision

Record one exact final candidate identity and one coherent evidence bundle.

Mandatory final gates:

- P2P v3 PASS;
- compact DAG relay PASS;
- fast sync/pruning/bootstrap PASS;
- mempool v3/fee market PASS;
- Mining Protocol v3 PASS;
- NVIDIA GPU mining PASS;
- AMD/ATI GPU mining PASS;
- multi-GPU runtime PASS;
- GPU kernel correctness/hardening PASS;
- accepted high-cadence operating envelope PASS;
- >=1,000,000-block deterministic replay PASS;
- rolling upgrade/live activation PASS;
- public RPC/API v3 PASS;
- automated chaos matrix PASS;
- supply-chain/release-security PASS;
- 25-node/16-miner rehearsal PASS;
- 168-hour burn-in PASS;
- public canary and 30-day acceptance PASS;
- zero unresolved release-blocking Sev-1 issues.

Select exactly one:

- `GO_V2_5_0`;
- `DELAY_V2_5_0`;
- `NO_GO_V2_5_0`.

## Completion criteria

v2.5.0 is complete only when Tasks 32-50 are merged or formally dispositioned under this dependency contract, the exact final candidate has current evidence for every mandatory gate, and Task 50 records exactly one final decision.

## Explicitly out of scope

- smart-contract implementation or activation;
- pool servers, pool share accounting, vardiff, payout systems or pool operator services;
- embedding mining inside the node;
- consensus shortcuts for GPU mining;
- mainnet genesis or mainnet launch;
- silent protocol/storage migration;
- declaring the fastest tested block cadence as a public default without evidence.
