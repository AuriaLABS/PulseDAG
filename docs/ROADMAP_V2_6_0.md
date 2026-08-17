# ROADMAP v2.6.0 — Programmability, Smart Contracts and Verifiable Applications

Status: **APPROVED FUTURE ROADMAP**

Approval date: 2026-08-17 UTC

## Purpose

v2.6.0 introduces PulseDAG programmability after the v2.5.0 scale, GPU-mining and public-testnet acceptance program has completed. The design direction is inspired by the modern Kaspa programmability model: keep the L1 DAG/UTXO consensus core small and deterministic, add UTXO-native covenant/script capabilities for bounded on-chain rules, and support more complex application execution through explicitly versioned verifiable-program / based-application mechanisms rather than turning every full node into an unbounded global application VM.

This is an architectural direction, not a compatibility claim with Kaspa and not an authorization to copy unstable external APIs or implementations. PulseDAG specifications, transaction formats, proof domains, state commitments and activation rules remain independently versioned and reviewed.

v2.6.0 implementation must not begin until the accepted stable-public-testnet gate required by the project policy has completed. The intended predecessor evidence is the v2.5.0 public-testnet 30-day acceptance gate.

## Non-negotiable principles

- UTXO/DAG consensus remains the authoritative ordering and settlement layer.
- Smart-contract semantics are explicitly versioned and activated; no silent mutation of existing transaction or UTXO semantics.
- Simple programmable spending rules should use bounded UTXO-native covenant/script mechanisms.
- Complex applications must not force every L1 node to execute unlimited arbitrary workloads.
- All consensus-visible execution and proof verification is deterministic and resource-bounded.
- Cross-network replay is prevented by chain/network/domain binding.
- Contract state, token supply and application commitments must be replayable and auditable.
- CPU, memory, state growth and proof-verification cost are explicitly metered/bounded.
- Mining remains external to the node and continues to receive deterministic transaction/contract fees through the normal block-template path.
- **Pool servers, pool protocols, share accounting, vardiff, worker management and payout systems are external third-party projects and are not part of PulseDAG v2.6.0.**
- v2.6.0 does not authorize mainnet launch.

## Dependency spine

```text
v2.5.0 final GO + accepted 30-day public-testnet gate
                         |
                         v
                      Task 51
                         |
              +----------+-----------+
              |                      |
              v                      v
           Tasks 52-55            Tasks 57-59
        Covenants/VM core      Verifiable apps/proofs
              |                      |
              +----------+-----------+
                         v
                      Task 56
                         |
                         v
                    Tasks 60-64
                         |
                         v
                      Task 65
                         |
                         v
                      Task 66
                         |
                         v
                      Task 67
                         |
                         v
                      Task 68
                         |
                         v
                      Task 69
```

## Tasks

### Task 51 — v2.6.0 programmability activation contract

Freeze the complete programmability scope before implementation lands.

Required outcomes:

- exact accepted v2.5.0 baseline and evidence identity;
- proof that the prerequisite accepted public-testnet stability period is complete;
- transaction/script/contract versioning rules;
- activation height/condition and mixed-version behavior;
- chain/network/domain separation rules;
- storage, snapshot and pruning compatibility boundaries;
- rollback/downgrade boundary;
- execution/resource accounting model;
- contract-state commitment model;
- proof-system versioning policy;
- evidence invalidation rules;
- explicit statement that no downstream task may invent conflicting activation semantics.

### Task 52 — UTXO Covenants v1

Introduce bounded UTXO-native programmable spending conditions.

Initial capability targets may include:

- advanced timelocks;
- vault policies;
- programmable multisignature conditions;
- escrow;
- atomic-swap primitives;
- payment-channel primitives;
- controlled asset issuance/spending constraints;
- successor-output/state-continuation rules.

Covenants must be deterministic, statically bounded enough for consensus validation, and unable to access non-deterministic host resources.

### Task 53 — Contract Transaction v3

Define an explicitly versioned programmable transaction format without mutating Transaction Protocol v2 in place.

Required fields/semantics include as appropriate:

- covenant/contract inputs and outputs;
- execution/state commitments;
- application/contract namespace and version identity;
- payload commitment;
- declared compute/resource budgets;
- proof commitments where used;
- stable transaction/submission identity;
- canonical serialization/signing/txid derivation;
- chain/network/domain binding;
- deterministic rejection and replay rules.

### Task 54 — PulseScript

Create a PulseDAG-native source language/toolchain for bounded programmable spending and contract logic.

Required properties:

- deterministic semantics;
- static typing;
- no floating-point consensus behavior;
- explicit integer/overflow semantics;
- bounded/analysable resource constructs;
- reproducible compiler output;
- versioned compiler/bytecode/script format;
- ABI/interface description;
- source maps/debug metadata outside consensus where appropriate;
- golden compilation vectors.

### Task 55 — Deterministic Contract VM

Provide a bounded deterministic execution environment for contract logic that exceeds simple covenant primitives but remains appropriate for L1 verification.

Requirements:

- deterministic integer-only consensus arithmetic unless another exact representation is formally specified;
- deterministic memory/stack behavior;
- instruction/compute metering;
- memory, stack, recursion/call-depth and state-access limits;
- deterministic failure codes;
- no filesystem, external networking, wall-clock time, host randomness or hardware-dependent semantics;
- reproducible execution across supported architectures;
- differential test vectors across implementations/platforms.

### Task 56 — Parallel contract execution on the DAG

Use PulseDAG canonical ordering and explicit state-access declarations to execute independent programmable operations in parallel where correctness permits.

Required outcomes:

- deterministic read/write set model or equivalent conflict declaration;
- parallel execution only for non-conflicting state transitions;
- canonical conflict resolution through the accepted DAG order;
- deterministic rollback/re-execution when selected ordering changes inside the non-final region;
- identical final contract/application state regardless of arrival order or local thread scheduling;
- bounded scheduler complexity.

### Task 57 — Based Applications

Define an application model in which PulseDAG L1 provides ordering, data/commitment availability as specified, and final settlement while complex application execution may occur outside the critical L1 execution path.

Required outcomes:

- versioned application namespace/identity;
- canonical operation ordering from L1;
- state-transition commitment rules;
- settlement/finality semantics;
- fraud/validity-proof policy as applicable;
- explicit data-availability assumptions;
- deterministic rejection of malformed or incompatible application commitments;
- no hidden trusted sequencer requirement unless explicitly declared by an application profile.

### Task 58 — PulseProgs / Verifiable Programs

Create the PulseDAG-native framework for independently versioned verifiable programs.

The architecture should separate at minimum:

- program identity/versioning;
- state commitments;
- scheduling;
- transaction/application runtime;
- proof/verification interface;
- storage/indexing boundaries;
- rollback and canonical-state transitions.

PulseProgs may take conceptual inspiration from external verifiable-program architectures, but PulseDAG must not bind consensus to unstable third-party APIs or implementation details.

### Task 59 — ZK verification layer

Introduce a versioned proof-verification abstraction for application/state-transition proofs.

Required outcomes:

- proof-system/version identifiers;
- verification-key commitments/versioning;
- proof-size and verification-cost limits;
- state-root/transition commitments;
- chain/network/application domain separation;
- replay protection;
- golden proof/verification vectors;
- fail-closed handling of unknown proof systems or versions;
- ability to add future proof systems through explicit activation rather than silent substitution.

### Task 60 — Native assets and token standards

Define consensus-backed programmable asset standards rather than indexer-only conventions.

The roadmap should support at minimum standardized profiles for:

- fungible assets;
- unique/non-fungible assets;
- multi-asset collections.

Required semantics include deterministic mint/burn/transfer, supply caps where declared, ownership/spending rules, metadata commitments, application events and replayable total-supply accounting.

Names such as `PDT-20`, `PDT-721` and `PDT-1155` may be reserved as working conventions but must not imply Ethereum/EVM compatibility unless separately specified and tested.

### Task 61 — Contract state, events, indexing and RPC

Expose bounded stable APIs for programmable state without forcing consensus nodes to maintain unlimited application indexes.

Required outcomes:

- contract/application identity lookup;
- current state/commitment queries;
- execution/history references;
- event streaming;
- asset/contract transaction status;
- bounded pagination and resource limits;
- indexer-friendly canonical event/state-change feeds;
- strict separation between consensus-required state and optional heavy indexes.

### Task 62 — Contract resource and fee economy

Define deterministic pricing/limits for programmable workloads.

The accounting model must be able to charge separately for relevant resource classes such as:

- transaction bytes;
- compute/instructions;
- memory where consensus-visible;
- state reads/writes/growth;
- proof verification;
- other explicitly specified scarce resources.

The design must not blindly copy a single-variable gas model if separate bounded resource dimensions provide safer economics.

### Task 63 — Contract security model

Treat programmable execution as a new high-risk consensus surface.

Mandatory validation areas include:

- malformed bytecode/script;
- integer/serialization edge cases;
- replay/domain attacks;
- reentrancy/call-graph hazards where applicable;
- state-conflict and ordering attacks;
- resource exhaustion;
- storage/state-growth attacks;
- oversized/malformed proof attacks;
- recursive/cross-contract amplification;
- DAG reorder/non-final rollback behavior;
- compiler/runtime differential fuzzing;
- property-based state-transition testing;
- adversarial contract corpus and regression suite.

### Task 64 — Mining and programmable-fee integration

Integrate programmable transaction fees into the existing external-miner/template pipeline without adding pool logic.

The node must deterministically compute and expose the block economics for:

```text
block subsidy
+ ordinary transaction fees
+ programmable compute/state fees
+ proof-verification fees where applicable
```

Required outcomes include deterministic template accounting, miner-visible fee totals, submit validation, fee replay and no divergence between node, wallet/application tooling and canonical state.

### Task 65 — Monetary policy final candidate

Produce the candidate monetary/economic policy that later mainnet-preparation releases can freeze.

Record and test:

- maximum/target supply model as applicable;
- emission curve;
- block reward/reduction schedule;
- coinbase maturity;
- base transaction fee rules;
- programmable resource fees;
- burn/recycling behavior only if explicitly approved;
- exact total-supply calculation at arbitrary height/accepted state;
- no hidden or implementation-dependent issuance path.

This task produces a candidate policy; final mainnet freeze remains a later roadmap responsibility.

### Task 66 — Smart-contract testnet

Run a dedicated programmability validation program before v2.6 promotion.

Workloads must include:

- covenants;
- PulseScript contracts;
- VM execution;
- tokens/assets;
- parallel independent contracts;
- intentional state conflicts;
- invalid/malformed contracts;
- verifiable programs / based applications;
- ZK proof verification where implemented;
- large state and state-growth pressure;
- transaction spam/resource exhaustion attempts;
- AMD/NVIDIA production mining;
- snapshots, pruning, restart/rejoin and rolling upgrades.

### Task 67 — Programmability million-transaction replay

Replay at least 1,000,000 programmable transactions/operations under varied valid arrival and scheduling permutations.

The same canonical input history must produce byte-identical:

- canonical DAG/order;
- contract execution outcomes;
- covenant outputs;
- asset supply/ownership state;
- application/state commitments;
- proof-verification outcomes;
- global canonical state digest.

### Task 68 — 30-day programmability burn-in

Operate the accepted v2.6 candidate for at least 30 accepted days with programmability enabled and one exact evidence-controlled candidate identity.

Exercise sustained normal transactions, contracts, tokens, verifiable applications/proofs, AMD/NVIDIA mining, pruning, snapshots, restarts, node rejoin, rolling upgrades and controlled failure scenarios.

No unexplained consensus/application-state divergence or unresolved release-blocking Sev-1 issue may remain.

### Task 69 — v2.6.0 final decision

Record the exact candidate SHA, protocol/transaction/VM/proof/storage versions and complete evidence bundle.

Mandatory final gates:

- activation contract PASS;
- UTXO Covenants v1 PASS;
- Contract Transaction v3 PASS;
- PulseScript PASS;
- deterministic VM PASS;
- parallel DAG execution PASS;
- Based Applications model PASS;
- PulseProgs/verifiable-program framework PASS;
- ZK verification layer PASS where in accepted scope;
- native asset/token standards PASS;
- contract state/events/RPC PASS;
- resource/fee economy PASS;
- contract security/fuzzing PASS;
- mining/programmable-fee integration PASS;
- monetary-policy candidate PASS;
- smart-contract testnet PASS;
- >=1,000,000 programmable-operation replay PASS;
- 30-day programmability burn-in PASS;
- zero unresolved release-blocking Sev-1 issues.

Select exactly one:

- `GO_V2_6_0`;
- `DELAY_V2_6_0`;
- `NO_GO_V2_6_0`.

## Completion criteria

v2.6.0 is complete only when Tasks 51-69 are merged or formally dispositioned under this roadmap, all mandatory deterministic/security/economic evidence is current for one exact final candidate, and Task 69 records exactly one final decision.

## Explicitly out of scope

- pool server software;
- pool protocol implementation owned by PulseDAG;
- share accounting;
- vardiff;
- worker management/authentication;
- pool payouts or custody;
- embedding pool services into the node or official miner;
- implicit EVM compatibility;
- copying unstable third-party programmability APIs into consensus;
- unbounded arbitrary host execution from contracts;
- mainnet genesis or mainnet launch.

Third parties remain free to build mining pools or other services against PulseDAG's documented public node/miner interfaces, but those projects are outside the PulseDAG core roadmap.
