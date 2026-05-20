# Release Notes v2.2.13 — Consensus/DAG Safety Audit

PulseDAG v2.2.13 is an intermediate consensus and DAG safety audit milestone between the v2.2.12 full private-testnet rehearsal and the v2.3.0 private-testnet readiness decision.

This release is **not** v2.3.0 readiness and does **not** declare official private-testnet readiness. v2.3.0 remains the readiness decision milestone.

## Milestone intent

v2.2.13 uses the evidence and operator feedback from v2.2.12 to audit consensus-facing DAG behavior before the project makes a readiness decision. The goal is to improve confidence in deterministic validation, orphan handling, replay behavior, and documentation boundaries without expanding product scope.

## Consensus/DAG safety audit scope

v2.2.13 focuses on tests, review notes, and safety documentation for:

- DAG invariant tests that verify accepted blocks preserve expected graph structure.
- Block structural validation tests for parent linkage, height, timestamps, and header/body shape.
- Transaction validation negative-path tests for malformed, invalid, or otherwise rejected transactions.
- Orphan adoption tests covering missing-parent recovery without unsafe acceptance.
- Tip selection tests that produce stable deterministic results for equivalent local DAG state.
- Replay/order-independence tests where practical.
- Block acceptance taxonomy tests that keep accepted, rejected, orphaned, duplicate, and structurally invalid outcomes distinct.
- Documentation that clearly describes PulseDAG's current DAG model and compatibility limits.

The detailed audit document is [DAG Safety Invariants v2.2.13](DAG_SAFETY_INVARIANTS_V2_2_13.md).

## Compatibility boundary

PulseDAG does **not** claim full Kaspa or GHOSTDAG compatibility in v2.2.13. The PoW integration path and DAG terminology may be Kaspa-informed, but v2.2.13 is an audit of PulseDAG's current consensus/DAG behavior, not a declaration that the implementation is consensus-compatible with Kaspa, GHOSTDAG, or any external network. kHeavyHash/PoW alignment does not imply consensus compatibility. PulseDAG currently uses a DAG structure and deterministic local tip policy, not full GHOSTDAG.

Any compatibility statements must remain limited to explicitly documented PulseDAG behavior and tested repository fixtures.

## Guardrails

- No smart contracts are introduced or claimed for v2.2.13.
- No pool coordination logic is introduced in the node or `pulsedag-miner`.
- `pulsedag-miner` remains external and standalone.
- v2.2.13 must not claim v2.3.0 readiness.
- Consensus rule changes should be avoided unless they fix a clear safety bug, include focused tests, and document the safety rationale.

## Expected evidence

A v2.2.13 closeout package should include:

- Confirmation that `VERSION` is `v2.2.13` if this release bumps the repository version.
- Confirmation that `[workspace.package].version` is `2.2.13` if this release bumps the Cargo workspace version.
- Test output or review notes showing DAG invariant tests pass.
- Test output or review notes showing block structural validation tests pass.
- Test output or review notes showing transaction validation negative-path tests pass.
- Test output or review notes showing orphan adoption tests pass.
- Test output or review notes showing tip selection tests pass.
- Test output or review notes showing replay/order-independence tests pass, or documented constraints where full coverage is impractical.
- Test output or review notes showing block acceptance taxonomy tests pass.
- Passing output for `cargo fmt --check`.
- Passing output for `cargo test -p pulsedag-core`.
- Passing output for `cargo test --workspace`.
- Passing output for `cargo build --workspace`.
- A guardrail review confirming no smart contracts were added, no pool logic was added, and the miner remains external.
- A documentation review confirming docs do not claim v2.3.0 readiness and do not claim full Kaspa/GHOSTDAG compatibility.
- A clear statement of any unresolved consensus/DAG safety risks that must inform the v2.3.0 decision.
- Link and filename verification for the v2.2.13 DAG safety document and related release documents.

## Relationship to v2.2.12 and v2.3.0

v2.2.12 remains the full private-testnet rehearsal and hardening milestone. v2.2.13 follows it as a focused consensus/DAG safety audit. v2.3.0 remains the private-testnet readiness decision milestone and should consume the combined evidence from v2.2.12 and v2.2.13.
