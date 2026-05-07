# Release Notes v2.2.13 — Consensus/DAG Safety Audit

PulseDAG v2.2.13 is an intermediate consensus and DAG safety audit milestone between the v2.2.12 full private-testnet rehearsal and the v2.3.0 private-testnet readiness decision.

This release is **not** v2.3.0 readiness and does **not** declare official private-testnet readiness. v2.3.0 remains the readiness decision milestone.

## Milestone intent

v2.2.13 uses the evidence and operator feedback from v2.2.12 to audit consensus-facing DAG behavior before the project makes a readiness decision. The goal is to improve confidence in deterministic validation, orphan handling, replay behavior, and documentation boundaries without expanding product scope.

## Consensus/DAG safety audit scope

v2.2.13 focuses on tests, review notes, and safety documentation for:

- DAG invariant checks that verify accepted blocks preserve expected graph structure.
- Deterministic tip selection tests that produce stable results for equivalent local DAG state.
- Parent, height, and timestamp validation tests.
- Missing-parent and orphan adoption safety tests.
- Replay and order-independence tests where practical.
- Documentation that clearly describes PulseDAG's current DAG model and compatibility limits.

## Compatibility boundary

PulseDAG does **not** claim full Kaspa or GHOSTDAG compatibility in v2.2.13. The PoW integration path and DAG terminology may be Kaspa-informed, but v2.2.13 is an audit of PulseDAG's current consensus/DAG behavior, not a declaration that the implementation is consensus-compatible with Kaspa, GHOSTDAG, or any external network.

Any compatibility statements must remain limited to explicitly documented PulseDAG behavior and tested repository fixtures.

## Guardrails

- No smart contracts are introduced or claimed for v2.2.13.
- No pool coordination logic is introduced in the node or `pulsedag-miner`.
- `pulsedag-miner` remains external and standalone.
- v2.2.13 must not claim v2.3.0 readiness.
- Consensus rule changes should be avoided unless they fix a clear safety bug, include focused tests, and document the safety rationale.

## Expected evidence

A v2.2.13 closeout package should include:

- Test output or review notes for DAG invariant coverage.
- Test output or review notes for deterministic tip selection.
- Test output or review notes for parent, height, and timestamp validation.
- Test output or review notes for missing-parent/orphan adoption safety.
- Replay/order-independence evidence where practical, including any documented constraints.
- A clear statement of any unresolved consensus/DAG safety risks that must inform the v2.3.0 decision.

## Relationship to v2.2.12 and v2.3.0

v2.2.12 remains the full private-testnet rehearsal and hardening milestone. v2.2.13 follows it as a focused consensus/DAG safety audit. v2.3.0 remains the private-testnet readiness decision milestone and should consume the combined evidence from v2.2.12 and v2.2.13.
