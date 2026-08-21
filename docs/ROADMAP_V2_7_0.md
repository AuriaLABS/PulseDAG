# ROADMAP v2.7.0 — Programmability and Deterministic Contract Execution

Status: planning

## Purpose

PulseDAG v2.7.0 is the proposed programmability release. Its objective is to introduce a deterministic, resource-bounded contract execution layer that integrates with PulseDAG transaction semantics, canonical DAG ordering, storage, mining, RPC and multi-node recovery without weakening historical protocol behavior.

This roadmap is intentionally a planning document only. It does not authorize public or mainnet contract activation.

## Entry conditions

Work on the v2.7.0 activation path must respect the upstream release and public-testnet gates already defined by the project. In particular, smart contracts remain disabled until the required public-testnet evidence and a separate approval exist.

Before any v2.7.0 activation candidate is treated as final:

- the underlying consensus, transaction, storage, mining and P2P foundations must be stable;
- historical transaction/signing behavior must remain reproducible;
- contract execution must be disabled by default until explicitly activated;
- all activation-affecting protocol identities and version numbers must be frozen;
- evidence from different candidate SHAs must not be combined.

## Design principles

1. Determinism before throughput.
2. Explicit versioning rather than in-place mutation of historical protocol rules.
3. Resource accounting in consensus, not wall-clock timeouts.
4. Canonical DAG order is the only execution order that may affect committed contract state.
5. Node operation remains keyless; private-key custody stays outside the node.
6. Restart, restore, pruning and rejoin must reproduce the same committed state.
7. Contract activation is separate from implementation completion.
8. v2.7.0 should remain intentionally bounded; advanced ecosystem features belong to later releases.

---

# Milestones

## Milestone 1 — Protocol scope and activation contract

Freeze the exact v2.7.0 protocol surface before implementing execution.

Required decisions:

- contract protocol/version identifier;
- activation identity and activation profile;
- pre-activation behavior;
- disabled-by-default behavior;
- mixed-version node behavior;
- transaction/signing compatibility boundary;
- rollback and downgrade rules;
- storage/snapshot compatibility identity;
- candidate invalidation rules for activation-affecting changes.

Acceptance:

A single versioned activation contract exists and all later milestones implement against it without changing historical semantics in place.

## Milestone 2 — Deterministic execution model v1

Define the first contract execution semantics.

Freeze:

- contract identity/address model;
- contract code identity;
- readable and writable state;
- call semantics;
- contract-to-contract calls if supported;
- atomicity and revert behavior;
- event/log behavior;
- execution context exposed to contracts;
- maximum call depth;
- forbidden nondeterministic inputs.

Contracts must not depend on local filesystem, network, wall clock, uncontrolled randomness, thread scheduling or host-specific behavior.

Acceptance:

Identical code, input, state and canonical execution context always produce byte-identical output, state transition and status.

## Milestone 3 — Contract transaction format v1

Introduce canonical versioned transaction semantics for programmability.

Required transaction capabilities:

- deploy contract;
- call contract;
- transfer value to/from a contract where supported;
- canonical contract address derivation;
- explicit chain/network signing domain;
- canonical serialization;
- deterministic txid and submission identity;
- receipt linkage;
- event linkage;
- stable rejection classes.

Compatibility requirements:

- do not mutate historical transaction formats in place;
- wrong-network/wrong-domain transactions fail closed;
- pre-activation contract transactions are rejected deterministically;
- historical replay remains reproducible.

Acceptance:

Golden vectors prove canonical encoding, signing-domain separation, txid derivation and deterministic rejection behavior.

## Milestone 4 — Deterministic gas and fee schedule

Make execution resource accounting a consensus rule.

Define:

- gas schedule version;
- instruction/opcode costs;
- memory costs;
- storage read/write costs;
- code deployment costs;
- calldata costs;
- event/log costs;
- transaction gas limit;
- block gas limit;
- maximum contract code size;
- maximum state/value size;
- maximum event payload;
- interaction with transaction fees.

Gas exhaustion must terminate execution deterministically and must not rely on wall-clock timeouts.

Acceptance:

The same execution consumes exactly the same gas and produces exactly the same fee/result classification on every supported node.

## Milestone 5 — Contract state model and commitments

Add deterministic committed contract state.

Required work:

- canonical contract state representation;
- deterministic state key/value encoding;
- contract code commitment;
- contract state commitment;
- receipt commitment where required;
- event commitment/indexing boundary;
- integration with canonical PulseDAG state digest;
- explicit versioning for state schema.

The execution relationship must be reproducible as:

`canonical DAG order -> canonical transaction order -> execution -> receipts -> contract state -> canonical state digest`

Acceptance:

Independent nodes processing the same canonical history produce byte-identical contract state and canonical state digests.

## Milestone 6 — Contract runtime / VM v1

Implement the first bounded execution engine.

Runtime requirements:

- sandboxed execution;
- deterministic host API;
- no direct filesystem access;
- no direct networking;
- no wall-clock dependency;
- no uncontrolled entropy;
- bounded memory;
- bounded call depth;
- deterministic arithmetic behavior;
- versioned runtime identity;
- deterministic trap/revert/error classes;
- gas enforced throughout execution.

The implementation may choose an execution technology only after the deterministic and operational requirements above are satisfied.

Acceptance:

The runtime passes deterministic execution vectors across supported platforms and rejects unsupported/nondeterministic behavior consistently.

## Milestone 7 — GHOSTDAG and canonical-order integration

Bind contract execution to PulseDAG canonical ordering.

Required behavior:

- block arrival order must never define contract state;
- selected-parent / DAG-order semantics determine execution order;
- side tips cannot commit conflicting transient state prematurely;
- reordering/reclassification must converge to the same canonical execution result;
- duplicate/conflicting contract effects resolve deterministically;
- reorg/finality behavior must be explicitly defined for contract state.

Test matrices must vary:

- peer order;
- parent arrival order;
- side-tip arrival;
- delayed parents;
- restart timing;
- multi-miner block production.

Acceptance:

All nodes converge to identical transaction order, receipts, contract state and canonical state digest after quiescence.

## Milestone 8 — Mempool and mining integration

Integrate contracts into admission, selection and templates.

Mempool requirements:

- version-aware contract transaction admission;
- signature/domain validation;
- pre-activation rejection;
- gas-limit checks;
- bounded code/calldata checks;
- duplicate/conflict rules;
- stable machine-readable rejection classes;
- deterministic interaction with existing transaction conflicts.

Mining requirements:

- contract-aware template construction;
- block gas limit enforcement;
- selected-tip context binding;
- deterministic inclusion order;
- stale-template handling;
- no hidden node-side signing or private-key custody.

Acceptance:

Node, mempool and miner agree on contract transaction identity, eligibility, ordering, resource accounting and final inclusion outcome.

## Milestone 9 — Durable storage, restart, snapshot and pruning

Make contract execution recoverable and reproducible.

Required work:

- persist contract code and state;
- persist consensus-relevant receipts/metadata;
- persist schema/version identity;
- atomic state transition persistence;
- crash-safe writes;
- restart parity;
- snapshot export/verify/restore;
- pruning rules that retain all consensus-required material;
- clean-node catch-up;
- offline/rejoin recovery;
- downgrade boundaries.

Acceptance:

Restart, snapshot restore, compact pruning and clean catch-up reproduce the same contract and canonical state digests without manual database repair.

## Milestone 10 — RPC, developer API and SDK baseline

Expose a narrow public developer surface without exposing node administration.

Initial RPC capabilities should include:

- deploy submission;
- contract call submission;
- read-only call/simulation;
- gas estimation;
- contract metadata lookup;
- contract state query;
- transaction receipt query;
- event query;
- protocol/runtime version query.

Security requirements:

- strict body limits;
- rate limits on public surfaces;
- stable error codes;
- no private keys, seeds or wallet passwords accepted by node RPC;
- simulation results clearly identified as non-final;
- public and operator control planes remain separate.

SDK priority:

1. Rust;
2. TypeScript/JavaScript.

Acceptance:

A developer can build, simulate, submit and reconcile a contract transaction using documented versioned APIs without requiring administrator RPC access.

## Milestone 11 — Security and adversarial execution suite

Build a dedicated contract threat and abuse matrix.

Mandatory classes:

- infinite-loop attempts;
- gas exhaustion;
- excessive memory allocation;
- recursion/call-depth abuse;
- storage amplification;
- malformed bytecode/program payloads;
- oversized calldata;
- event/log spam;
- integer and serialization edge cases;
- invalid state transitions;
- duplicate/conflicting submissions;
- denial-of-service pressure against simulation and submission APIs;
- parser/runtime fuzzing where practical;
- reentrancy analysis if the execution model permits nested mutable calls;
- dependency/RustSec review for runtime and cryptographic dependencies.

Acceptance:

No adversarial input can cause nondeterministic consensus output, unbounded consensus resource consumption, silent state corruption or unsafe cross-network execution.

## Milestone 12 — Deterministic replay and adversarial multi-node validation

Prove the complete v2.7.0 execution stack on one exact candidate SHA.

Replay matrix:

- identical DAG under multiple arrival permutations;
- peer-order variation;
- parent-order variation;
- restart at different points;
- transaction-order pressure;
- historical pre-contract replay;
- contract activation boundary replay.

Recovery matrix:

- process restart;
- snapshot restore;
- pruning + restore;
- clean catch-up;
- offline/rejoin;
- mixed-version/capability rehearsal.

Multi-node matrix:

- 5 nodes / 1 miner;
- 5 nodes / 2 miners;
- multi-miner stress profile;
- bounded network partitions;
- delayed/reordered parents;
- transaction conflicts;
- contract-call pressure;
- RPC abuse pressure.

Compare at minimum:

- selected tip;
- canonical DAG-order digest;
- transaction outcome digest;
- receipt digest;
- event digest where consensus-relevant;
- contract state root/digest;
- canonical state digest;
- gas/resource accounting totals.

Acceptance:

Every mandatory matrix converges deterministically on one exact SHA with no unexplained state divergence, replay mismatch or recovery failure.

## Milestone 13 — Private programmability devnet burn-in

Run a real private network with contract execution intentionally enabled under an explicit non-public profile.

Required evidence:

- exact candidate SHA;
- exact runtime/protocol versions;
- clean database/network initialization;
- multiple independent nodes;
- multiple external miners where relevant;
- representative contract deployment/call workload;
- sustained transaction load;
- planned restart;
- node isolation/rejoin;
- snapshot/restore;
- pruning drill;
- resource/latency/storage metrics;
- contract state digest comparisons;
- incident log;
- final PASS/FAIL decision.

Any consensus or contract-state divergence invalidates the run.

Acceptance:

The private devnet runs the complete contract stack under realistic failures and recovers with identical committed state across nodes.

## Milestone 14 — v2.7.0 release and activation decision

Freeze one final technical candidate and record the release decision.

Mandatory evidence:

- exact source SHA;
- clean provenance;
- VERSION/Cargo/release identity;
- contract protocol version;
- runtime/VM version;
- gas schedule version;
- transaction format version;
- storage/state schema version;
- chain/network/genesis/config identity;
- node/miner/wallet/SDK artifact digests where applicable;
- workflow run IDs and evidence digests;
- deterministic replay PASS;
- multi-node PASS;
- private devnet burn-in PASS;
- no unresolved Sev-1 consensus, execution, storage, replay, security or operator-safety issue.

Record exactly one technical decision:

- `GO_V2_7_0_RELEASE`
- `DELAY_V2_7_0_RELEASE`
- `NO_GO_V2_7_0_RELEASE`

A technical release GO does not automatically authorize public/mainnet contract activation. Activation remains a separate explicit decision governed by upstream network readiness and operational evidence.

---

# Dependency spine

Primary dependency order:

`M1 -> M2 -> M3 -> M4 -> M5 -> M6 -> M7 -> M8 -> M9 -> M10 -> M11 -> M12 -> M13 -> M14`

After M6 is stable, parts of M9, M10 and M11 may proceed in parallel, but M12 must validate the integrated final implementation on one exact SHA.

# Completion criteria for v2.7.0

v2.7.0 is technically complete only when:

1. the activation contract and all protocol/runtime versions are frozen;
2. deterministic contract execution is implemented and resource-bounded;
3. contract state is committed through canonical DAG execution order;
4. mempool, mining, storage and RPC are integrated;
5. restart, snapshot, pruning, restore and rejoin preserve exact state;
6. adversarial and security validation passes;
7. deterministic multi-node replay passes on one exact candidate SHA;
8. private programmability devnet burn-in passes;
9. no unresolved Sev-1 blocker remains;
10. a final v2.7.0 technical release decision is recorded.

The core end-to-end invariant is:

`same canonical DAG history -> same transaction order -> same execution -> same gas -> same receipts -> same contract state -> same canonical state digest`

# Explicitly out of scope for v2.7.0

To keep the release bounded, the following should be tracked separately unless the roadmap is deliberately revised and all affected evidence restarted:

- mainnet launch authorization;
- public contract activation authorization;
- bridges;
- full EVM compatibility;
- native DeFi protocols;
- protocol-level NFT features;
- on-chain governance;
- zero-knowledge proof systems;
- rollups;
- sharding;
- complex proxy/upgrade frameworks;
- hardware-wallet integration;
- HSM/secure-enclave custody.

# Planning rule

If a change modifies contract execution semantics, canonical ordering, gas accounting, transaction identity, committed state, activation identity or storage compatibility after candidate validation has started, the affected exact-SHA validation evidence must be regenerated rather than combined with older evidence.
