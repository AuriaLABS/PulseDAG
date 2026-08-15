# ROADMAP v2.4.0 — Runtime Resilience, Protocol v2 and Deterministic DAG Consensus

Date: 2026-08-15 UTC

## Starting point

The v2.3.0 private-testnet line established repeatable multi-host bootstrap, lifecycle tooling, observability, incident runbooks, and protected rehearsal evidence.

The first v2.4.0 program then focused on operator modes, route-contract enforcement, control-plane resilience, submission finality, target-based retargeting, public-safe RPC, wallet hardening, pruning-aware sync, dependency security, and public-testnet release readiness.

The v2.4.0 scope is now intentionally extended before final release/activation to include two protocol-level changes that were previously deferred:

1. a versioned transaction/signing v2 contract with cryptographic chain/network binding and explicit replay/replacement/submission semantics; and
2. the deterministic GHOSTDAG-style consensus path already described by `GHOSTDAG_SELECTION_DESIGN_SPEC.md` and `GHOSTDAG_SELECTION_MULTI_PR_PLAN.md`, including selected-parent selection, bounded blue/red classification, canonical DAG ordering, state application, finality/pruning, P2P sync and mining integration.

This scope expansion means no earlier v2.4.0 candidate SHA, burn-in artifact, launch rehearsal, release identity or GO decision can be treated as final if it predates required Tasks 22–30. Evidence must be regenerated on the final exact candidate whenever the activation contract says the old evidence is invalidated.

v2.4.0 remains a no-smart-contract release. Public-testnet launch remains a separate explicit decision controlled by issue `#781`.

## Guardrails

- `VERSION`, Cargo package versions and release identity remain unchanged until the final Task 31 decision authorizes the v2.4.0 release/activation candidate.
- No `v2.4.0` tag or release artifact may be published from roadmap/implementation work without explicit maintainer approval.
- `public_testnet_ready=false` remains mandatory until the separate launch-control gate authorizes public launch.
- The 30-day public-testnet clock must not start or be backdated before the actual authorized public launch.
- Smart contracts remain disabled and require a separate later approval after the accepted public-testnet policy is satisfied.
- Multi-node safety remains fail-closed by default.
- Any single-node mode must be explicit and impossible to activate accidentally.
- Consensus, transaction, signing, header and storage-format changes require explicit versioning/activation rules; no canonical semantics may be silently mutated in place.
- Historical blocks, transactions, signatures and snapshots must remain deterministically decodable/replayable under their original version rules.
- Mining remains an external application; no embedded pool logic is introduced.
- High cadence remains disabled by default until Tasks 24–28 are complete and Task 29 is intentionally enabled for controlled experimentation.
- Code comments, developer documentation, commits, and pull-request descriptions remain English-only.
- Credentials, private keys, wallet seeds, local runtime state, generated burn-in output, and operator-specific configuration must not be committed.

## Dependency spine

```text
Tasks 14–20 runtime/release foundation
        |
        v
Task 21 pre-protocol readiness checkpoint
        |
        v
Task 22 activation contract
   |             |
   v             v
Task 23        Task 24
Tx/signing v2  DAG data model
   |             |
   |             v
   |           Task 25
   |             |
   |             v
   |           Task 26
   |             |
   |             v
   +---------> Task 27
         \       /
          v     v
           Task 28
              |
              v
           Task 29
              |
              v
           Task 30
              |
              v
           Task 31
```

## Active v2.4.0 work sequence

### Task 14 — Explicit single-node operator profile

Status: **ACTIVE** in issue `#784`.

Add a first-class single-node profile for local development, deterministic burn-in, and operator validation without weakening ordinary multi-node isolation safeguards.

Required outcomes include explicit opt-in, loopback-only RPC by default, safe zero-peer behavior, contradictory-configuration rejection, clear startup identity, and a deterministic transition back to ordinary private multi-node operation.

### Task 15 — Topology-aware mining-template availability

Status: **PLANNED**. Tracks issue `#783`.

Split zero-peer template behavior by explicit topology: intentional single-node operation may mine; ordinary nodes remain fail-closed when unexpectedly isolated. Degraded sync, orphan recovery and missing-parent recovery must block template production in every profile.

### Task 16 — RPC route and metrics-inventory contract

Status: **PLANNED**. Tracks issue `#783`.

Prevent route drift between the RPC router and observability inventory, enforce endpoint/schema compatibility in CI, and keep exporter health/error behavior explicit and bounded.

### Task 17 — Liveness and submission finality under sustained mining load

Status: **PLANNED**.

Keep `/health`, `/status`, `/p2p/status`, exporter collection and `/mining/submit` bounded under sustained external mining, including explicit unknown-finality reconciliation rather than false definitive rejection.

### Task 18 — Canonical target retarget and mining cadence safety

Status: **ACTIVE — CONSENSUS RELEASE BLOCKER**. Tracks issue `#786`.

Replace integer-difficulty adjustment with deterministic canonical target adjustment, use selected-chain history, exclude genesis timestamp contamination, fix the easiest-target absorbing state, and make template/RPC/validation agree byte-for-byte on consensus target data.

Design contract: [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md).

### Task 19 — Reference operator packaging and recovery

Status: **PLANNED**.

Maintain a reference stack for node, external miner, exporter, Prometheus and Grafana, with idempotent lifecycle/recovery operations and no downstream protocol patches or committed secrets.

### Task 20 — v2.4.0 runtime burn-in and compatibility matrix

Status: **BLOCKED ON TASKS 14–19**.

Run the runtime/operational candidate through single-node burn-in, contention, restart, snapshot/recovery, exporter continuity, topology transition and existing multi-node fault-recovery regressions.

Task 20 evidence is an operational baseline only. Because Tasks 22–30 now add protocol/consensus changes, Task 20 evidence must not be presented as the final v2.4.0 release evidence unless Task 22 explicitly proves it remains valid for the exact final candidate.

### Task 21 — Pre-protocol release-readiness checkpoint

Status: **BLOCKED ON TASKS 14–20**.

Task 21 is no longer the final v2.4.0 release decision. It is a checkpoint proving that the original runtime/operator/security foundation is sufficiently stable to begin the protocol-expansion sequence.

The checkpoint must confirm:

- the original operator/runtime blockers are resolved or explicitly dispositioned;
- accepted mining submissions cannot be misreported as definitive rejections;
- liveness remains bounded in the accepted operating envelope;
- target retargeting is deterministic;
- security/dependency and public-safe RPC gates remain blocking;
- wallet key custody remains outside the node;
- existing storage/sync/recovery evidence is coherent;
- no release tag, version bump, public launch or final candidate freeze is authorized yet.

Final release/activation authority moves to Task 31.

### Task 22 — v2.4.0 protocol scope and activation contract

Status: **ACTIVE**. Tracks issue `#865`.

Freeze the exact v2.4.0 protocol scope before downstream consensus and transaction-format work lands.

Required decisions:

- complete change inventory for Tasks 23–30;
- compatibility boundaries with existing v1 transaction/signing and historical data;
- versioning rules for headers, transactions, signing domains, P2P capability surfaces and persisted metadata;
- activation/version-gating strategy for every consensus-affecting change;
- mixed-version behavior;
- storage/snapshot migration rules;
- rollback/downgrade boundary;
- candidate and evidence invalidation rules.

No downstream task may invent a conflicting activation contract independently.

### Task 23 — Transaction and Signing Protocol v2

Status: **ACTIVE**. Tracks issue `#863`.

Introduce an explicitly versioned transaction/signing v2 contract without mutating transaction/signing v1 in place.

Required outcomes:

- cryptographic binding to explicit chain/network identity;
- canonical v2 serialization, signing message and txid derivation;
- cross-network domain-separation golden vectors;
- deterministic replay rules across v1/v2;
- explicit replacement/RBF decision and semantics;
- stable submission identity distinct from wallet-plan identity and final txid;
- deterministic mempool and wallet reconciliation behavior;
- explicit P2P/RPC/storage/indexing/miner/wallet compatibility and rejection rules.

Task 23 must pass the complete consensus, replay, mempool, storage, P2P, RPC, miner and wallet validation matrix on one exact SHA.

### Task 24 — GHOSTDAG consensus data model

Status: **ACTIVE**. Tracks issue `#866`.

Introduce the versioned metadata/storage foundation required by selected-parent and deterministic DAG consensus without silently activating new semantics.

Required model:

- `selected_parent` and selected-chain metadata;
- versioned `blue_score` semantics with an explicit legacy boundary;
- deterministic `blue_work` representation;
- merge-set/classification metadata;
- canonical order metadata/digests where needed;
- snapshot/export/import support;
- deterministic legacy decode/recompute/migration behavior.

Loading a new binary must not silently reinterpret historical canonical fields.

### Task 25 — Deterministic selected-parent and blue/red classification

Status: **ACTIVE**. Tracks issue `#867`.

Implement deterministic selected-parent/selected-tip selection plus bounded merge-set blue/red classification.

The same valid block set must yield identical selected parent, selected tip, blue/red sets, blue score/work metadata and classification digest regardless of arrival order, peer order, orphan adoption order, restart timing or local iteration order.

Merge-set discovery and classification must be explicitly bounded against adversarial CPU, memory and database amplification.

### Task 26 — Deterministic DAG ordering, state and finality

Status: **ACTIVE**. Tracks issue `#868`.

Define the canonical total DAG order and make authoritative state application follow that order rather than arrival order.

Required outcomes:

- deterministic selected-chain + merge-set ordering;
- staged state application where arrival order cannot be authoritative;
- deterministic transaction conflict/duplicate resolution;
- rollback/rebuild semantics for selected-chain/order changes;
- canonical UTXO/state-root replay determinism;
- explicit finality boundary;
- pruning/snapshot rules that retain all data needed for the non-final region;
- fail-closed behavior when required historical parent state is unavailable.

### Task 27 — GHOSTDAG-aware P2P sync

Status: **ACTIVE**. Tracks issue `#869`.

Extend sync to converge on the same selected chain, DAG frontier and missing-parent state under the activated consensus model.

Required outcomes:

- capability-negotiated selected-chain locators;
- DAG frontier exchange;
- deterministic common-ancestor discovery;
- pruning-aware peer compatibility;
- productive orphan/missing-parent recovery;
- automatic offline/rejoin convergence without process restart or DB repair;
- rolling/mixed-version behavior defined by Task 22;
- no unsafe selected-sync finalization from incomplete DAG knowledge.

### Task 28 — Mining, mempool and wallet protocol integration

Status: **ACTIVE**. Tracks issue `#870`.

Integrate selected-tip/DAG-order consensus and transaction/signing v2 across the external miner contract, template construction, mempool policy, public signed relay and dedicated wallet application.

Required outcomes:

- templates build on activated selected tip and safe parallel parents;
- deterministic duplicate/conflict filtering across selected-chain, merge-set and mempool context;
- v1/v2 mempool compatibility and stable rejection/replacement semantics;
- wallet v2 planning, chain-bound signing, identity verification, broadcast and reconciliation;
- offline signing and encrypted-keystore/keyless-node boundaries remain intact.

### Task 29 — Experimental high-cadence blocks

Status: **BLOCKED ON TASKS 24–28**. Tracks issue `#871`.

Introduce faster-block-cadence experimentation only after deterministic selection, classification, ordering, sync and mining integration are already proven.

Requirements:

- disabled by default;
- explicit dev/testnet experimental gate;
- no miner sleep as consensus clock;
- template freshness and submit-finality guarantees remain coherent;
- orphan/missing-parent recovery and RPC/control-plane liveness remain bounded;
- normal release/public defaults remain conservative until Task 30 evidence and Task 31 approval.

### Task 30 — Replay and adversarial multi-node validation

Status: **BLOCKED ON TASKS 23–29**. Tracks issue `#872`.

Validate the complete v2.4.0 protocol stack on one exact candidate SHA.

Mandatory matrix:

- identical DAG replay under different block, peer and orphan-arrival permutations;
- restart, snapshot/export/restore, compact prune and clean-node catch-up;
- offline/rejoin and pruning-aware peer selection;
- mixed-version/capability rehearsal under Task 22 rules;
- historical v1 and activated v2 transaction replay;
- replacement/RBF/submission-identity matrix where enabled;
- 5N/1M and 5N/2M convergence;
- high-pressure multi-miner/stress testing after Task 29 gates;
- delayed/reordered parents, orphan storms, peer churn and bounded partitions;
- public signed-relay abuse/resource-limit regression.

All nodes must converge on byte-identical selected-parent metadata, selected tip/chain/frontier, classification digest, DAG-order digest, transaction outcomes and canonical state digest.

### Task 31 — v2.4.0 release and activation decision

Status: **BLOCKED ON TASKS 22–30**. Tracks issue `#873`.

Task 31 is the final technical v2.4.0 release/activation gate.

Before a GO decision, record:

- exact final source SHA and provenance;
- complete VERSION/Cargo/release identity;
- consensus/header/transaction/signing activation versions;
- chain/network/genesis/config digests;
- node/miner/wallet artifact digests;
- storage/snapshot compatibility identity;
- exact validation workflow/evidence digests;
- migration and rollback/downgrade boundaries;
- final status of experimental high cadence.

Select exactly one:

- `GO_V2_4_0_RELEASE_AND_ACTIVATION`;
- `DELAY_V2_4_0_RELEASE_AND_ACTIVATION`;
- `NO_GO_V2_4_0_RELEASE_AND_ACTIVATION`.

A Task 31 GO is still not by itself a public-testnet launch. Public launch remains controlled by `#781`; any required burn-in/rehearsal evidence invalidated by Tasks 22–30 must be rerun on the final exact SHA before public GO.

## Completion criteria

v2.4.0 is technically complete only when:

1. Tasks 14–31 are merged or formally dispositioned under the roadmap dependency rules;
2. all required consensus, transaction, wallet, P2P, storage, replay, security and operational tests pass on the exact final candidate;
3. Task 30 records deterministic PASS evidence on that candidate;
4. no unresolved Sev-1 consensus, storage, replay, sync, mining, transaction, security or operator-safety blocker exists;
5. Task 31 records exactly one final release/activation decision;
6. any public-testnet launch still separately satisfies `#794` and receives explicit authorization in `#781`.

## Out of scope

- implicit or automatic public-testnet launch;
- starting or backdating the 30-day public-testnet clock without the actual authorized launch;
- smart-contract activation;
- production/mainnet custody claims;
- GPU mining enablement without a canonical verified kernel;
- pool protocols or embedded pool services;
- replacing kHeavyHash;
- silent in-place mutation of v1 transaction/signing/header semantics;
- weakening ordinary-node isolation safeguards for operator convenience.
