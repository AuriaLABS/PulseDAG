# PulseDAG v2.2.10 Release Notes

## Summary

v2.2.10 closes the **real PoW completion** milestone for PulseDAG. This release aligns versioning, implementation, RPC metadata, and documentation around one active Proof-of-Work truth, while keeping P2P completion explicitly deferred to v2.2.11.

## What v2.2.10 closes

- Kaspa-based **kHeavyHash PoW engine** is the canonical consensus identity and implementation path.
- **256-bit target comparison** semantics are the required validation rule.
- **PulseDAG header adapter** path is treated as canonical for PoW preimage formation.
- **Node/miner agreement** is enforced through shared `pulsedag-core` PoW behavior.
- Final `/pow` metadata is expected to reflect active devnet PoW truth.
- `/mining/template` metadata includes canonical `target_hex` for external miners.
- PoW vectors/tests are aligned to the finalized v2.2.10 path.
- Specs/docs are aligned to one active PoW truth.

## Implementation and API coherence

v2.2.10 closeout requires coherent behavior across:

- Core consensus PoW validation.
- Miner submit/verification flow.
- `/pow` metadata semantics.
- `/mining/template` miner-facing target metadata (`target_hex`).

## Known limitations (intentional)

- This release does **not** claim production readiness.
- This release does **not** complete P2P networking behavior.
- No smart contracts are introduced.
- No pool logic is introduced.
- Miner remains an external process/application.

## Handoff to v2.2.11

v2.2.11 is the dedicated **P2P completion** milestone, including peer connectivity hardening and multi-node synchronization closure, without reopening v2.2.10 PoW identity decisions.
