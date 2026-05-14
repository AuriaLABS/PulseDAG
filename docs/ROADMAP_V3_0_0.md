# PulseDAG v3.0.0 roadmap and gates

v3.0.0 is the stable network target for the long-lived PulseDAG core. It must be earned through evidence from the v2.2.x hardening line, the v2.3.0 private-testnet readiness decision, private testnet operation, stable testnet burn-in, and release-candidate rehearsals.

## Target definition

v3.0.0 should represent a stable node core with documented consensus behavior, storage/replay recovery, P2P operation, external miner integration, operator runbooks, release artifacts, and upgrade/rollback policy. It is not a vehicle for unrelated feature expansion.

## Required gate sequence

1. v2.2.14 foundation hardening documents and rehearses storage, replay, snapshot, mining-template, three-node, and burn-in evidence expectations.
2. v2.3.0 makes the private-testnet readiness decision only after its required gates are satisfied.
3. Private testnet operation completes the 14-day burn-in gate with no unresolved Sev-1 consensus, storage, replay, sync, or mining-template incidents.
4. Stable testnet operation completes at least 30 days before smart contract implementation begins.
5. v3 release candidates prove deterministic replay, snapshot restore, multi-node convergence, miner submit behavior, upgrade/rollback, monitoring, and operator runbook readiness.
6. v3.0.0 is tagged only after release evidence shows every hard gate is satisfied or an explicit non-blocking waiver is recorded.

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

The stable network target for v3.0.0 requires more than a private testnet boot. It requires sustained stable-testnet behavior, incident discipline, documented recovery paths, and release-candidate artifacts that can be operated by maintainers without hidden local assumptions.

A stable testnet must run for at least 30 days before smart contract implementation begins. The 30-day period should include normal block production, node restarts, snapshot/restore drills, miner submit validation, monitoring review, and incident review.

## Smart contract gate

Smart contracts are post-stable-testnet work. Before the 30-day stable testnet gate completes, the project must not add:

- Smart contract VM/runtime execution.
- Contract deployment transactions.
- Contract state transition logic.
- Gas accounting or contract fee-market rules.
- Contract RPC/API surfaces.
- Contract-specific consensus rules.

Design notes may be written, but implementation must wait until the stable core has proven itself.

## Miner and pool gate

The miner remains external for v3.0.0. The node provides the mining template and submit validation surface; the miner performs work and returns submissions. This boundary keeps the node consensus surface smaller and keeps mining-device concerns out of node consensus code.

Pool logic is not allowed in the miner. Share accounting, payout policy, pool membership, pool authentication, and pool operator services belong in separate pool infrastructure if they are ever built.

## Explicitly out of scope for v3.0.0

- Smart contracts before the 30-day stable testnet gate.
- Embedding a miner inside the node.
- Adding pool logic to the miner.
- Public claims that exceed the documented consensus and network evidence.
- Feature work that bypasses release gates or weakens operator recovery.
- Compatibility claims with other networks unless backed by explicit specifications and tests.

## Promotion rule

A release may be promoted to v3.0.0 only when the gate evidence is complete, current, and reviewable. If evidence is missing, stale, or contradicted by unresolved incidents, the release remains a candidate and must not be called the stable network target.
