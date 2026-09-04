# PulseDAG v3.0.0 roadmap and gates

v3.0.0 is the definitive coordinated mainnet and parallel public-testnet release. It must be earned through the integrated v2.5.0 scale/resilience and v2.6.0 programmability workstreams, exact-candidate evidence, and the final launch decision.

## Target definition

v3.0.0 should represent a stable node core with documented consensus behavior, storage/replay recovery, P2P operation, external miner integration, operator runbooks, release artifacts, and upgrade/rollback policy. It is not a vehicle for unrelated feature expansion.

## Required gate sequence

1. v2.5.0 scale, resilience, GPU, mining, replay, upgrade, API, chaos, and burn-in gates complete.
2. v2.6.0 programmability, contract, VM, fee, proof, asset, security, and replay gates complete.
3. Wallet/custody, dependency/security, release, network-isolation, and production-operations gates complete on one exact candidate.
4. Mainnet and the parallel public testnet receive independently frozen identities and reproducible genesis/configuration.
5. v3.0.0 is tagged only after the exact-candidate evidence ledger and launch manifest support `GO_V3_DUAL_LAUNCH`.

## v3 hard gates

- `cargo fmt --check` and `cargo test --workspace` pass for release-candidate artifacts.
- Multi-node rehearsals demonstrate convergence after restart, rejoin, and delayed/lagging node recovery.
- Mining template retrieval and submit validation are stable through the external miner/node contract.
- Snapshot export/import and restore are repeatable and documented.
- Replay and order-independence checks show deterministic state reconstruction.
- Storage migration and rollback expectations are documented for operators.
- Observability exposes enough health, rejection, sync, and mining information to diagnose incidents.
- No unresolved Sev-1 consensus, storage, replay, sync, or mining-template issue is open.
- Release artifacts and operator runbooks are reproducible.

## v2.3.0 private testnet readiness

v2.3.0 is a readiness decision, not a public launch. It requires the evidence established by v2.2.14, including:

- Passing required Cargo checks.
- Successful three-node rehearsal.
- Mining template/submit validation.
- Snapshot export/import validation.
- Replay/order-independence validation.
- A 14-day burn-in plan and completed results before claiming readiness.
- Clear documentation of any known private-testnet limitations.

## Stable network target

The definitive network target requires more than a private testnet boot. It
requires the integrated v2.5/v2.6 evidence, incident discipline, documented
recovery paths, and exact release artifacts that can be operated without hidden
local assumptions.

Programmability evidence must include 30 accepted days on the exact integrated candidate before GO. This is an acceptance gate, not a prerequisite to begin programmability implementation and not a pre-mainnet public-testnet clock.

## Smart contract gate

Programmability is part of the integrated v3.0.0 acceptance matrix and must pass before GO. It includes:

- Smart contract VM/runtime execution.
- Contract deployment transactions.
- Contract state transition logic.
- Gas accounting or contract fee-market rules.
- Contract RPC/API surfaces.
- Contract-specific consensus rules.

- Covenants, Contract Transaction v3, PulseScript, bounded VM, parallel execution, applications, proofs, assets, state/events/RPC, and programmable fees.
- Contract security/fuzzing/adversarial evidence and at least 1,000,000 programmable-operation deterministic replay.

## Miner and pool gate

The miner remains external for v3.0.0. The node provides the mining template and submit validation surface; the miner performs work and returns submissions. This boundary keeps the node consensus surface smaller and keeps mining-device concerns out of node consensus code.

Pool logic is not allowed in the miner. Share accounting, payout policy, pool membership, pool authentication, and pool operator services belong in separate pool infrastructure if they are ever built.

## Explicitly out of scope for v3.0.0

- A standalone public-testnet launch before v3.0.0.
- Embedding a miner inside the node.
- Adding pool logic to the miner.
- Public claims that exceed the documented consensus and network evidence.
- Feature work that bypasses release gates or weakens operator recovery.
- Compatibility claims with other networks unless backed by explicit specifications and tests.

## Promotion rule

A release may be promoted to v3.0.0 only when the gate evidence is complete, current, and reviewable. If evidence is missing, stale, or contradicted by unresolved incidents, the release remains a candidate and must not be called the stable network target.
