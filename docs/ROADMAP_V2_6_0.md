# ROADMAP v2.6.0 — Programmability, Smart Contracts and Verifiable Applications Workstream for v3.0.0

Status: **APPROVED MANDATORY V3.0.0 WORKSTREAM**

Original approval date: 2026-08-17 UTC
Integration rebaseline: 2026-08-31 UTC

## Purpose

v2.6.0 is the mandatory programmability engineering milestone on the path to PulseDAG v3.0.0. Its technical scope is incorporated into the authoritative `ROADMAP_V3_0_0.md` acceptance matrix.

The intended path is:

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

The design keeps the L1 DAG/UTXO consensus core deterministic and bounded, adds UTXO-native programmable rules, and supports more complex verifiable applications through explicitly versioned mechanisms rather than forcing unbounded global application execution into every full node.

This document no longer requires completion of a standalone public-testnet 30-day clock before programmability work begins. The v2.5 technical workstream remains the predecessor milestone, while public mainnet and parallel testnet launch together only after the integrated v3.0.0 GO decision.

## Non-negotiable principles

- UTXO/DAG consensus remains the authoritative ordering and settlement layer.
- Smart-contract semantics are explicitly versioned/activated; no silent mutation of existing transaction/UTXO semantics.
- Simple programmable spending rules use bounded UTXO-native covenant/script mechanisms.
- Complex applications must not force every L1 node to execute unlimited arbitrary workloads.
- All consensus-visible execution and proof verification is deterministic and resource bounded.
- Cross-network replay is prevented by chain/network/application/domain binding.
- Contract state, token supply and application commitments are replayable/auditable.
- CPU, memory, state growth and proof-verification cost are explicitly metered/bounded.
- Mining remains external to the node and receives deterministic ordinary + programmable fees through the normal template path.
- Pool servers/protocols/share accounting/vardiff/worker management/payouts remain external third-party projects.
- This workstream does not by itself authorize public launch; #781 is the sole final launch authority.

## Dependency spine

```text
v2.5 workstream PASS
        |
        v
     Task 51
        |
   +----+-----+
   v          v
Tasks 52-55  Tasks 57-59
   |          |
   +----+-----+
        v
     Task 56
        |
   Tasks 60-64
        |
     Task 65
        |
     Task 66
        |
     Task 67
        |
     Task 68
        |
     Task 69
        |
        v
 v3.0.0 final integration/GO
```

## Tasks

### Task 51 — Programmability activation contract

Freeze the complete programmability scope before implementation lands.

Required outcomes:

- exact accepted v2.5 workstream baseline/evidence identity;
- transaction/script/contract versioning rules;
- activation condition and mixed-version behavior;
- chain/network/application/domain separation;
- storage/snapshot/pruning compatibility boundaries;
- rollback/downgrade boundary;
- execution/resource accounting model;
- contract/application state-commitment model;
- proof-system versioning policy;
- evidence invalidation rules;
- no downstream task may invent conflicting activation semantics.

### Task 52 — UTXO Covenants v1

Initial accepted capabilities may include:

- advanced timelocks;
- vault policies;
- programmable multisignature;
- escrow;
- atomic-swap primitives;
- payment-channel primitives;
- controlled asset issuance/spending constraints;
- successor-output/state-continuation rules.

Covenants must be deterministic, statically/resource bounded and unable to access non-deterministic host resources.

### Task 53 — Contract Transaction v3

Define a versioned programmable transaction format without silently mutating Transaction Protocol v2.

Required fields/semantics as applicable:

- covenant/contract inputs and outputs;
- execution/state commitments;
- application/contract namespace and version;
- payload commitment;
- declared compute/resource budgets;
- proof commitments;
- stable transaction/submission identity;
- canonical serialization/signing/txid derivation;
- chain/network/domain binding;
- deterministic rejection/replay rules.

### Task 54 — PulseScript

Required properties:

- deterministic semantics;
- static typing;
- no floating-point consensus behavior;
- explicit integer/overflow semantics;
- bounded/analyzable resource constructs;
- reproducible compiler output;
- versioned compiler/bytecode/script format;
- ABI/interface description;
- source maps/debug metadata outside consensus where appropriate;
- golden compilation vectors.

### Task 55 — Deterministic Contract VM

Requirements:

- deterministic integer-only consensus arithmetic unless another exact representation is formally frozen;
- deterministic memory/stack behavior;
- instruction/compute metering;
- memory, stack, recursion/call-depth and state-access limits;
- deterministic failure codes;
- no filesystem, external networking, wall-clock, host randomness or hardware-dependent consensus semantics;
- reproducible execution across supported architectures;
- differential vectors across implementations/platforms.

### Task 56 — Parallel contract execution on the DAG

Required outcomes:

- deterministic read/write set or equivalent conflict declaration;
- parallel execution only for non-conflicting transitions;
- canonical conflict resolution through accepted DAG order;
- deterministic rollback/re-execution when selected ordering changes inside the non-final region;
- identical final contract/application state regardless of arrival order/local thread scheduling;
- bounded scheduler complexity.

### Task 57 — Based Applications

Required outcomes:

- versioned application namespace/identity;
- canonical operation ordering from L1;
- state-transition commitment rules;
- settlement/finality semantics;
- fraud/validity-proof policy as applicable;
- explicit data-availability assumptions;
- deterministic rejection of malformed/incompatible commitments;
- no hidden trusted sequencer requirement unless explicitly declared by an application profile.

### Task 58 — PulseProgs / Verifiable Programs

Separate and version at minimum:

- program identity/versioning;
- state commitments;
- scheduling;
- transaction/application runtime;
- proof/verification interface;
- storage/indexing boundaries;
- rollback/canonical-state transitions.

PulseDAG must not bind consensus to unstable third-party APIs or implementation details.

### Task 59 — ZK verification layer

Where included in the frozen v3 scope:

- proof-system/version identifiers;
- verification-key commitments/versioning;
- proof-size/verification-cost limits;
- state-root/transition commitments;
- chain/network/application domain separation;
- replay protection;
- golden proof/verification vectors;
- fail-closed unknown proof systems/versions;
- future proof systems added only through explicit activation.

### Task 60 — Native assets and token standards

Support consensus-backed programmable asset profiles for:

- fungible assets;
- unique/non-fungible assets;
- multi-asset collections.

Required semantics include deterministic mint/burn/transfer, declared supply caps, ownership/spending rules, metadata commitments, application events and replayable total-supply accounting.

Names such as `PDT-20`, `PDT-721` and `PDT-1155` may be working conventions but do not imply external VM compatibility.

### Task 61 — Contract state, events, indexing and RPC

Required outcomes:

- contract/application identity lookup;
- current state/commitment queries;
- execution/history references;
- event streaming;
- asset/contract transaction status;
- bounded pagination/resource limits;
- indexer-friendly canonical event/state-change feeds;
- strict separation between consensus-required state and optional heavy indexes.

### Task 62 — Contract resource and fee economy

Define deterministic pricing/limits for relevant resource classes such as:

- transaction bytes;
- compute/instructions;
- memory where consensus visible;
- state reads/writes/growth;
- proof verification;
- other explicitly specified scarce resources.

Do not blindly collapse all limits into one variable if separate resource dimensions provide safer deterministic economics.

### Task 63 — Contract security model

Mandatory validation includes:

- malformed bytecode/script;
- integer/serialization edge cases;
- replay/domain attacks;
- reentrancy/call-graph hazards where applicable;
- state-conflict/order attacks;
- resource exhaustion;
- storage/state-growth attacks;
- oversized/malformed proof attacks;
- recursive/cross-contract amplification;
- DAG reorder/non-final rollback behavior;
- compiler/runtime differential fuzzing;
- property-based state-transition testing;
- adversarial contract corpus/regressions.

### Task 64 — Mining and programmable-fee integration

The node must deterministically compute/expose template economics for:

```text
block subsidy
+ ordinary transaction fees
+ programmable compute/state fees
+ proof-verification fees where applicable
```

Required outcomes include deterministic accounting, miner-visible fee totals, submit validation and no divergence between node, wallet/application tooling and canonical state.

No pool logic is added.

### Task 65 — Production monetary/economic policy candidate

Produce/freeze the economic policy required by final v3 integration:

- maximum/target supply model as applicable;
- emission curve;
- block reward/reduction schedule;
- coinbase maturity;
- base transaction fee rules;
- programmable resource fees;
- burn/recycling behavior only if explicitly approved;
- exact total-supply calculation at arbitrary height/accepted state;
- no hidden/implementation-dependent issuance path.

The final production freeze is recorded with the exact v3 candidate in #794/#781.

### Task 66 — Integrated programmability validation program

**Rebaselined from the old dedicated smart-contract public-testnet step.**

Before final v3 GO, run controlled private/release-candidate validation that exercises:

- covenants;
- PulseScript contracts;
- VM execution;
- tokens/assets;
- parallel independent contracts;
- intentional state conflicts;
- invalid/malformed contracts;
- verifiable programs / based applications;
- ZK proof verification where included;
- large state/state-growth pressure;
- transaction spam/resource exhaustion;
- AMD/NVIDIA production mining;
- snapshots, pruning, restart/rejoin and rolling upgrades.

The permanent parallel public testnet launches with mainnet after final v3 GO and remains the public validation environment after launch.

### Task 67 — Programmability million-transaction replay

Replay at least 1,000,000 programmable transactions/operations under varied valid arrival/scheduling permutations.

Equivalent canonical histories must produce byte-identical:

- canonical DAG/order;
- contract execution outcomes;
- covenant outputs;
- asset supply/ownership state;
- application/state commitments;
- proof-verification outcomes;
- global canonical state digest.

### Task 68 — 30-day programmability exact-candidate burn-in

Retain the original strong long-duration requirement, but execute it as **pre-launch exact-candidate evidence**, not as a standalone public-testnet prerequisite.

Before final v3 GO, accumulate at least **30 accepted days** with programmability enabled on controlled private/release-candidate infrastructure tied to one exact candidate identity.

Exercise sustained normal transactions, contracts, tokens, verifiable applications/proofs, AMD/NVIDIA mining, pruning, snapshots, restarts, node rejoin, rolling upgrades and controlled failure scenarios.

Invalidated time may not be silently counted. No unexplained consensus/application-state divergence or unresolved workstream-blocking Sev-1 issue may remain.

### Task 69 — v2.6 workstream completion decision for v3

Record the exact candidate SHA, transaction/contract/VM/proof/storage/economic identities and complete evidence bundle.

Mandatory gates:

- activation contract PASS;
- UTXO Covenants v1 PASS;
- Contract Transaction v3 PASS;
- PulseScript PASS;
- deterministic VM PASS;
- parallel DAG execution PASS;
- Based Applications PASS;
- PulseProgs/verifiable-program framework PASS;
- ZK verification PASS where included in frozen scope;
- native asset/token standards PASS;
- contract state/events/RPC PASS;
- resource/fee economy PASS;
- contract security/fuzzing PASS;
- mining/programmable-fee integration PASS;
- monetary/economic policy PASS;
- integrated programmability validation PASS;
- >=1,000,000 programmable-operation replay PASS;
- 30-day programmability exact-candidate burn-in PASS;
- zero unresolved workstream-blocking Sev-1 issues.

Select exactly one milestone result:

- `V2_6_WORKSTREAM_PASS`;
- `V2_6_WORKSTREAM_DELAY`;
- `V2_6_WORKSTREAM_FAIL`.

This result feeds the final integrated v3.0.0 acceptance matrix. It is **not** `GO_V3_DUAL_LAUNCH` and does not authorize a public network launch.

## Completion criteria

The v2.6 workstream is complete only when Tasks 51-69 are merged or formally dispositioned under the integrated v3 dependency contract, all deterministic/security/economic evidence is current for one exact candidate, and Task 69 records one coherent workstream result.

## Explicitly out of scope

- pool server software/protocol implementation owned by PulseDAG;
- share accounting/vardiff/worker management/payout custody;
- embedding pool services into node/official miner;
- implicit EVM compatibility;
- copying unstable third-party programmability APIs into consensus;
- unbounded arbitrary host execution from contracts;
- final mainnet/testnet genesis freeze;
- final public launch authorization.

Third parties may build services against documented public interfaces, but those projects remain outside the PulseDAG core workstream.
