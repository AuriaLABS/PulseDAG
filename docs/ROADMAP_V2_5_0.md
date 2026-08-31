# ROADMAP v2.5.0 — Scale, Production GPU Mining and Adversarial Resilience Workstream for v3.0.0

Status: **APPROVED MANDATORY V3.0.0 WORKSTREAM**

Original approval date: 2026-08-17 UTC
Integration rebaseline: 2026-08-31 UTC

## Purpose

v2.5.0 is the mandatory scale-and-resilience engineering milestone on the path to PulseDAG v3.0.0. Its technical scope is incorporated into the authoritative `ROADMAP_V3_0_0.md` acceptance matrix.

The intended path is:

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

This workstream proves that PulseDAG can operate under sustained load, hostile network conditions, large DAG history, real NVIDIA/AMD GPU mining and rolling upgrades without weakening deterministic consensus or operator safety.

This document no longer defines a standalone public launch. The old v2.5 public-testnet canary/30-day acceptance prerequisite is superseded by the integrated v3.0.0 pre-launch evidence program and the coordinated mainnet + parallel-testnet launch in Q4 2026.

## Non-negotiable principles

- Consensus determinism remains the primary invariant.
- The miner remains an external application; mining logic is not embedded into the node.
- Production GPU mining supports both **NVIDIA** and **AMD/ATI** as first-class v3 release targets.
- NVIDIA-only or AMD-only completion is insufficient for the integrated v3 production-mining gate.
- CPU reference PoW remains the canonical correctness oracle for GPU implementations.
- Ordinary multi-node safety remains fail-closed.
- High-cadence operation is promoted only from measured evidence.
- Storage, snapshot, pruning and sync changes require explicit compatibility/version boundaries.
- Rolling upgrades must not require intentionally stopping the full network.
- No pool server, share accounting, vardiff or payout logic is added to the node or official miner.
- This workstream does not by itself authorize mainnet/testnet launch; #781 is the sole final authority.

## Dependency spine

```text
v2.4.x accepted implementation baseline
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
                  |
                  v
           v2.6 workstream
                  |
                  v
           v3.0.0 final GO
```

## Tasks

### Task 32 — v2.5 workstream entry gate and protocol freeze

Freeze the scale/resilience compatibility contract before downstream implementation.

Required outcomes:

- exact accepted v2.4.x baseline SHA/protocol identity;
- complete v2.5 workstream feature inventory;
- capability/version negotiation rules;
- storage/snapshot compatibility boundaries;
- activation and rollback/downgrade rules;
- evidence invalidation rules;
- explicit definition of consensus-affecting changes;
- no silent reinterpretation of persisted/consensus data.

### Task 33 — P2P v3 and eclipse resistance

Required outcomes:

- deterministic/observable peer scoring;
- inbound/outbound quotas;
- peer diversity and failure-domain awareness;
- eclipse-resistance policy;
- CPU/RAM/disk/bandwidth budgets per peer/activity class;
- connection/handshake/inventory/orphan/request flood protection;
- temporary penalties with deterministic recovery;
- bounded peer churn;
- quality-aware sync-source selection;
- no unbounded resource amplification by one peer.

### Task 34 — Compact DAG relay

Required outcomes:

- header/DAG-metadata-first announcements;
- compact block reconstruction;
- transaction/inventory deduplication;
- parent-aware and parallel-parent relay;
- body-on-demand retrieval;
- bounded reconstruction state;
- safe full-block fallback;
- propagation/reconstruction/bandwidth metrics;
- identical canonical validation regardless of relay form.

### Task 35 — Fast sync, pruning v2 and state bootstrap

Required outcomes:

- versioned snapshot manifests/state commitments;
- chunked/resumable/parallel snapshot transfer where safe;
- cryptographic snapshot verification;
- pruning-aware DAG frontier/bootstrap;
- clean-node catch-up;
- restore/rejoin across supported storage states;
- snapshot poisoning/incompatibility rejection;
- post-bootstrap verification/replay checks;
- bounded long-term storage profiles.

### Task 36 — Deterministic mempool v3 and fee market

Required outcomes:

- deterministic fee-rate policy/estimation;
- hard memory/resource limits;
- deterministic eviction/expiry;
- replacement/RBF semantics consistent with the frozen transaction protocol;
- ancestor/descendant/package limits;
- package-aware acceptance/relay where enabled;
- conflict-set handling;
- anti-spam resource pricing;
- restart reconstruction and DAG-reordering reconciliation;
- order-independent final mempool state for equivalent inputs.

### Task 37 — Mining Protocol v3

Required outcomes:

- versioned mining jobs/template sequence IDs;
- bounded push/equivalent new-work notification;
- template invalidation and target updates;
- selected-tip/parallel-parent awareness;
- stable submission identity;
- explicit accepted/rejected/stale/unknown-finality states;
- deterministic submit reconciliation;
- multiple miners per node;
- bounded backpressure;
- end-to-end template-to-submit telemetry.

### Task 38 — Production GPU mining: NVIDIA + AMD/ATI

Mandatory v3 targets:

- NVIDIA CUDA backend;
- AMD/ATI production backend using the supported AMD compute stack;
- canonical kHeavyHash-compatible PoW semantics identical to CPU validation;
- CPU ↔ NVIDIA golden vectors;
- CPU ↔ AMD golden vectors;
- NVIDIA ↔ AMD same-input deterministic vectors;
- nonce/target boundary tests;
- single-GPU and multi-GPU correctness;
- Linux support for both families;
- Windows support where the selected runtime/driver stack is supported;
- no GPU-specific consensus-validation shortcut in the node.

Final integrated evidence must record:

- `GPU_MINING_NVIDIA_PASS=true`;
- `GPU_MINING_AMD_PASS=true`.

### Task 39 — GPU runtime, multi-GPU and device management

Required outcomes:

- device discovery and explicit selection;
- homogeneous/heterogeneous multi-GPU scheduling;
- mixed AMD/NVIDIA host operation where supported;
- deterministic nonce-space partitioning with zero intentional duplicate work;
- isolated workers per device;
- watchdog/device reset/failure recovery;
- automatic work redistribution;
- reconnect/job refresh after disruption;
- per-device configuration;
- per-device/aggregate hashrate, accepted/rejected/stale, error and uptime metrics;
- temperature/power/clocks/utilization telemetry where safely exposed.

### Task 40 — GPU kernel performance and hardening

Required outcomes:

- architecture-specific NVIDIA/AMD profiling;
- occupancy/register/memory-access analysis;
- batching/work-size tuning;
- bounded job-change latency;
- low host-CPU overhead;
- reproducible benchmark mode;
- correctness/performance regression baselines;
- endianness/overflow/nonce-boundary/malformed-work tests;
- long-duration GPU soak;
- device-error/recovery tests;
- no optimization may change canonical PoW results.

### Task 41 — High-cadence protocol operating envelope

Initial validation points include approximately:

- 1 second;
- 500 ms;
- 250 ms.

These are test points, not automatic production defaults.

Measure propagation delay, orphan/merge-set pressure, DAG width, CPU/block, canonical-state apply latency, database amplification, template freshness, submit latency, stale rate, sync/finality behavior and miner fairness.

The accepted production cadence is selected from evidence and may be more conservative than the fastest passing point.

### Task 42 — Million-block deterministic DAG replay

Validate at least 1,000,000 DAG blocks under deterministic replay/reconstruction.

Equivalent valid histories must converge to byte-identical:

- selected parents;
- selected-tip/chain metadata;
- blue/red classification;
- canonical DAG order;
- transaction outcomes;
- UTXO/state;
- state digest.

Cover varied block/peer/orphan arrival permutations, restarts, snapshots, pruning, forks and parallel blocks.

### Task 43 — Rolling upgrade and live activation

Required outcomes:

- capability/version negotiation;
- canary/staged rollout mechanics;
- explicit activation point where required;
- storage/snapshot migration;
- pre-activation rollback;
- post-activation downgrade fail-closed where semantics cannot safely revert;
- version-distribution/compatibility observability;
- no full-network shutdown as a normal upgrade procedure.

### Task 44 — Public RPC/API v3 and event streaming

Required outcomes:

- API versioning/schema contract;
- machine-readable API contract where practical;
- deterministic error codes;
- request IDs, pagination and payload/query bounds;
- rate-limit metadata;
- event streaming for blocks, selected-tip/DAG updates, transaction/mempool state, sync state and mining-job invalidation;
- strict public/admin/operator separation.

### Task 45 — Observability v3 and automated chaos testing

Automated scenarios include node/miner process loss, GPU-worker/device loss, disk pressure/latency, packet loss, latency, peer churn, seed loss, partitions, bounded clock skew, orphan/delayed-parent storms, RPC load, snapshot recovery and prune/rejoin.

Each scenario emits a reviewable timeline, metrics, state/convergence digests, incident record and PASS/FAIL.

### Task 46 — Reproducible supply chain and release security

Required outcomes:

- reproducible or independently verifiable builds;
- SBOM;
- artifact provenance;
- signed/attested release manifests where supported;
- dependency/security audit gates;
- least-privilege workflows;
- source-SHA-linked node/miner artifacts;
- supported Linux/Windows package validation;
- CPU/NVIDIA/AMD miner artifacts as applicable;
- clean-machine native smoke tests;
- recorded binary/network-config hashes.

### Task 47 — 25-node / 16-miner / multi-GPU adversarial rehearsal

Run on one exact candidate SHA with:

- at least 25 real nodes;
- at least 16 external miner processes;
- NVIDIA, AMD/ATI, multi-GPU and supported mixed-GPU scenarios;
- at least three independent failure domains;
- multiple regions/providers where practical.

Inject partitions, node/miner/GPU churn, seed loss, stale templates, target changes, orphan storms, RPC load, snapshot/bootstrap/pruning, offline/rejoin, rolling upgrade and high-cadence periods.

Final state must converge across surviving/recovered nodes with no unexplained canonical divergence.

### Task 48 — Seven-day exact-candidate burn-in

Run at least **168 contiguous hours** on one unchanged integrated v3 candidate.

A candidate-changing fix invalidates affected burn-in evidence unless the evidence contract explicitly proves otherwise.

Continuously record block production, DAG width, selected-tip changes, orphan/merge-set pressure, CPU, RAM, disk, bandwidth, peer state, mempool, state digest, miner/GPU telemetry, template/submit latency, stale rate, snapshot/pruning activity and incidents.

No unresolved Sev-1 consensus, storage, replay, sync, mining, security or operator-safety issue may remain.

### Task 49 — Integrated v3 pre-launch network acceptance

**Rebaselined from the old standalone public-testnet canary/30-day task.**

There is no required standalone public-testnet launch before v3 mainnet. Instead, validate the frozen release candidate on controlled private/release-candidate infrastructure with production-like topology and configuration, and preserve evidence needed by #794/#781.

Required coverage includes:

- staged/canary deployment mechanics;
- node joins/leaves;
- NVIDIA/AMD mining;
- rolling upgrades;
- snapshot/pruning/restore;
- clean sync/rejoin;
- incident drills;
- replay/state-digest verification;
- identity-separation rehearsal for future mainnet and parallel testnet.

The permanent parallel public testnet launches together with mainnet after `GO_V3_DUAL_LAUNCH`.

### Task 50 — v2.5 workstream completion decision for v3

Record one exact candidate/evidence bundle for the scale/resilience workstream.

Mandatory gates:

- P2P v3 PASS;
- compact DAG relay PASS;
- fast sync/pruning/bootstrap PASS;
- mempool v3/fee market PASS;
- Mining Protocol v3 PASS;
- NVIDIA GPU mining PASS;
- AMD/ATI GPU mining PASS;
- multi-GPU runtime PASS;
- GPU hardening PASS;
- accepted high-cadence operating envelope PASS;
- >=1,000,000-block deterministic replay PASS;
- rolling upgrade/live activation PASS;
- public RPC/API v3 PASS;
- automated chaos matrix PASS;
- supply-chain/release-security PASS;
- 25-node/16-miner rehearsal PASS;
- 168-hour exact-candidate burn-in PASS;
- integrated pre-launch network acceptance PASS;
- zero unresolved workstream-blocking Sev-1 issues.

Select exactly one milestone result:

- `V2_5_WORKSTREAM_PASS`;
- `V2_5_WORKSTREAM_DELAY`;
- `V2_5_WORKSTREAM_FAIL`.

This result feeds the v2.6 workstream and final v3 acceptance. It is **not** `GO_V3_DUAL_LAUNCH` and does not authorize a public network launch.

## Completion criteria

The v2.5 workstream is complete only when Tasks 32-50 are merged or formally dispositioned under the integrated v3 dependency contract and Task 50 records one coherent result bound to exact evidence.

## Explicitly out of scope

- pool servers/share accounting/vardiff/payout systems;
- embedding mining inside the node;
- consensus shortcuts for GPU mining;
- mainnet/testnet genesis freeze;
- final public launch authorization;
- silent protocol/storage migration;
- declaring the fastest tested cadence as production default without evidence.

Programmability is handled by the subsequent v2.6 workstream and is also mandatory for final v3.0.0 acceptance.
