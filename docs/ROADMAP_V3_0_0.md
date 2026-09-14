# PulseDAG v3.0.0 roadmap and gates

v3.0.0 is the stable network target for the long-lived PulseDAG core. It must be earned through evidence from the v2.2.x hardening line, the v2.3.0 private-testnet readiness decision, private testnet operation, stable testnet burn-in, and release-candidate rehearsals.

## Target definition

v3.0.0 should represent a stable node core with documented consensus behavior, storage/replay recovery, P2P operation, external miner integration, operator runbooks, release artifacts, and upgrade/rollback policy. It is not a vehicle for unrelated feature expansion.

Smart-contract implementation readiness and smart-contract protocol activation are separate decisions. Smart-contract code may be complete, tested, reviewed, and present in v3.0.0 artifacts, but smart-contract execution is intentionally inactive at the v3.0.0 mainnet launch. Activation requires a later protocol upgrade and its own dedicated activation gate.

## Required gate sequence

1. v2.2.14 foundation hardening documents and rehearses storage, replay, snapshot, mining-template, three-node, and burn-in evidence expectations.
2. v2.3.0 makes the private-testnet readiness decision only after its required gates are satisfied.
3. Private testnet operation completes the 14-day burn-in gate with no unresolved Sev-1 consensus, storage, replay, sync, or mining-template incidents.
4. Stable testnet operation establishes at least 30 days of core-maturity evidence before smart-contract work is relied upon for any future activation decision; this is not a v3.0.0 smart-contract activation gate.
5. v3 release candidates prove deterministic replay, snapshot restore, multi-node convergence, miner submit behavior, upgrade/rollback, monitoring, and operator runbook readiness.
6. v3 release candidates verify that smart-contract execution remains fail-closed and inactive under the v3.0.0 activation state, regardless of implementation completeness.
7. v3.0.0 is tagged only after release evidence shows every v3.0.0 hard gate is satisfied or an explicit non-blocking waiver is recorded.

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
- Smart-contract execution is demonstrably inactive at the v3.0.0 activation state and cannot be enabled accidentally by configuration, restart, replay, or ordinary operator action.

Smart-contract feature completeness is not a v3.0.0 launch gate. Only safe inactivity and non-interference with the active v3.0.0 protocol are v3.0.0 gates.

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

The roadmap retains at least 30 days of stable-testnet evidence as a core-maturity requirement. That period should include normal block production, node restarts, snapshot/restore drills, miner submit validation, monitoring review, and incident review. It supports later smart-contract activation confidence, but smart-contract implementation completeness is not required for v3.0.0 and smart-contract activation remains explicitly excluded from v3.0.0 mainnet.

## Smart contract implementation and activation policy

Smart-contract implementation work may be complete in the v3.0 codebase, but implementation readiness does not imply mainnet activation.

The stable-core maturity gate exists to prevent smart-contract work from weakening core validation. It must not be interpreted as permission to activate contracts in v3.0.0 or as a requirement that smart-contract implementation be complete before v3.0.0 ships.

Smart-contract capabilities that may exist in the codebase include:

- Smart contract VM/runtime execution.
- Contract deployment transactions.
- Contract state transition logic.
- Gas accounting or contract fee-market rules.
- Contract RPC/API surfaces.
- Contract-specific consensus rules.

These capabilities may be implemented, tested, reviewed, and packaged. For v3.0.0 mainnet, however, smart-contract execution remains intentionally disabled even if the implementation is production-ready.

The v3.0.0 release must therefore prove both of the following:

- **Implementation status may be READY:** code, tests, documentation, and review may be complete.
- **Activation status must be INACTIVE:** contract deployment/execution and contract-specific state transitions are not active protocol behavior on v3.0.0 mainnet.

A later smart-contract activation requires a dedicated protocol-upgrade decision with explicit activation parameters, compatibility and migration analysis, consensus/replay evidence, security review, operator documentation, rollback or recovery policy, and a separate go/no-go decision. Smart-contract activation must not occur implicitly because code is present in a release artifact.

## Miner and pool gate

The miner remains external for v3.0.0. The node provides the mining template and submit validation surface; the miner performs work and returns submissions. This boundary keeps the node consensus surface smaller and keeps mining-device concerns out of node consensus code.

Pool logic is not allowed in the miner. Share accounting, payout policy, pool membership, pool authentication, and pool operator services belong in separate pool infrastructure if they are ever built.

## Explicitly out of scope for v3.0.0

- Activation of smart-contract deployment or execution on v3.0.0 mainnet.
- Treating smart-contract implementation completeness as a prerequisite for v3.0.0 launch.
- Any implicit or configuration-only path that can activate smart contracts without the later dedicated protocol activation gate.
- Embedding a miner inside the node.
- Adding pool logic to the miner.
- Public claims that exceed the documented consensus and network evidence.
- Feature work that bypasses release gates or weakens operator recovery.
- Compatibility claims with other networks unless backed by explicit specifications and tests.

## Promotion rule

A release may be promoted to v3.0.0 only when the gate evidence is complete, current, and reviewable. If evidence is missing, stale, or contradicted by unresolved incidents, the release remains a candidate and must not be called the stable network target.

Smart-contract implementation readiness does not block promotion to v3.0.0. Promotion requires evidence that smart contracts remain inactive and non-interfering under the v3.0.0 mainnet activation state. Their later activation is a separate release/protocol decision.
