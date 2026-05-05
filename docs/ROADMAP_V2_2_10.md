# Roadmap v2.2.10 — Real PoW Completion

## Milestone Positioning

- **v2.2.9**: private-testnet rehearsal baseline.
- **v2.2.10**: real PoW completion.
- **v2.2.11**: P2P completion.
- **v2.2.12**: full private-testnet rehearsal.
- **v2.3.0**: official private testnet.

This roadmap defines **v2.2.10** as the release focused on finishing one production-intent Proof-of-Work path for PulseDAG before deeper networking completion in v2.2.11.

## Problem Statement

PulseDAG currently has a naming and implementation contradiction:

- Public PoW identity says **kHeavyHash**.
- Some specs and/or code paths may still reference **BLAKE3** or **Keccak** placeholders.

That contradiction must be removed in v2.2.10. At the end of this milestone there must be exactly **one active consensus PoW truth** across specification, node validation, miner workflow, and operator-facing APIs.

## v2.2.10 Objective

Complete and lock the real PoW path so that PulseDAG can move into v2.2.11 with consensus hashing settled and testable.

## In Scope (v2.2.10)

1. **Kaspa-based kHeavyHash engine**
   - Finalize PoW engine behavior based on the Kaspa-style kHeavyHash approach adopted for PulseDAG.
   - Keep implementation scoped to PulseDAG consensus needs for this phase.

2. **256-bit target comparison**
   - Enforce canonical 256-bit hash-vs-target comparison semantics.
   - Remove ambiguity in endian/order handling and document the exact comparison rules.

3. **PulseDAG canonical header adapter**
   - Finalize deterministic mapping from PulseDAG block header format into the PoW preimage expected by the kHeavyHash pipeline.
   - Explicitly version and document this adapter behavior.

4. **Single shared PoW engine for node + miner**
   - Node and miner must use the same `pulsedag-core` PoW engine implementation.
   - Eliminate parallel or divergent PoW code paths.

5. **Accurate mining/PoW metadata surfaces**
   - `/pow` and `/mining/template` must expose accurate algorithm and target metadata.
   - Metadata returned by APIs must match the active consensus path exactly.

6. **Comprehensive PoW test coverage**
   - Add/complete tests for:
     - valid PoW acceptance,
     - invalid nonce/hash rejection,
     - mutated-header rejection,
     - duplicate-share / duplicate-submit behavior.
   - Ensure test fixtures reflect the final v2.2.10 canonical path.

7. **Operator and developer documentation**
   - Publish/update mining runbook for the final flow.
   - Publish/update final PoW spec reflecting the single active truth.

## Out of Scope / Boundaries

- No smart contracts.
- No smart-contract runtime.
- No pool logic inside miner.
- Miner remains external.
- Do **not** claim full Kaspa consensus compatibility.
- Do **not** claim production readiness.
- Do **not** implement full P2P in this PR set; full P2P completion is v2.2.11.

## Deliverables

- Finalized `pulsedag-core` PoW engine path for kHeavyHash.
- Canonical header-to-PoW adapter definition.
- Updated node/miner integration on the single PoW engine.
- `/pow` and `/mining/template` metadata alignment.
- Full PoW validation test suite (valid/invalid/mutated/duplicate cases).
- Updated mining runbook and final PoW spec.

## Exit Criteria (Definition of Done)

v2.2.10 is complete when all of the following are true:

1. There is one active PoW algorithm identity and implementation path: **kHeavyHash**.
2. Placeholder references (BLAKE3/Keccak PoW paths) are removed or clearly marked non-consensus and inactive.
3. Node block validation and miner-facing workflows resolve to the same shared `pulsedag-core` engine.
4. 256-bit target comparison behavior is fixed, tested, and documented.
5. `/pow` and `/mining/template` return metadata that is correct for the active consensus path.
6. PoW tests cover valid, invalid, mutated, and duplicate scenarios.
7. Mining runbook and PoW spec are published in their final v2.2.10 form.

## Forward Link to v2.2.11

After v2.2.10 closes real PoW completion, **v2.2.11** is the dedicated milestone for **P2P completion** (peer discovery/connectivity/sync hardening), without reopening PoW algorithm identity decisions.

## Acceptance Mapping

This document satisfies the roadmap acceptance requirements by ensuring:

- `docs/ROADMAP_V2_2_10.md` exists.
- v2.2.10 is clearly scoped as real PoW completion.
- v2.2.11 is clearly scoped as P2P completion.
- v2.3.0 remains the official private testnet milestone.


## Closure status (v2.2.10)

- Version alignment finalized at `v2.2.10` / `2.2.10`.
- Release notes, closing checklist, and smoke test docs are published for v2.2.10 closeout.
- PoW closure scope remains limited to consensus/path coherence; P2P completion stays in v2.2.11.
