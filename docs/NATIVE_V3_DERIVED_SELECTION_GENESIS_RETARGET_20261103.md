# Native-v3 selected-parent → retarget: candidate-derived genesis-bound proof (DORMANT)

**Status: UNAPPROVED diagnostic — does not change consensus.** Source baseline: `main@e06afa5055dd0bb9b1b875e8591ffb466ae3a6f5`; tracked in [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) V3-GD-04/06/07.

## Concrete gap addressed

The existing `retarget_context_v3::selected_parent_retarget_window_v3_ns` independently verifies chain-bound native header identity, canonical compact target, KHeavyHash PoW of every sampled non-genesis selected ancestor, parent membership and nanosecond ordering **but accepts a caller-supplied selected-parent map**. Even if those edges are real parent edges and carry valid PoW, they can choose the *wrong* branch. The existing `retarget_genesis_guard_v3::verify_dormant_frozen_genesis_retarget_v3_ns` authenticates exact independent frozen genesis within the at-most-20-sample window, but **does not derive the map**. No existing production path is therefore authorized to use either as v3 GHOSTDAG-derived monetary retarget.

New isolated Rust integration test composes these two **already existing dormant validators** with the independently authenticated experimental GHOSTDAG trial; selected-parent metadata is re-derived from **all reachable real-KHeavyHash ancestors**, not from P2P/persistence or an externally supplied map. It uses an experimental K=0 fork with two genesis siblings of unequal targets (proposed per-block work 2 and 512) and a real-PoW merge. At the join, the candidate selector chooses the **heavier** sibling; its derived selected chain is then fed into the **independently frozen-genesis** retarget guard. The existing *unbound* retarget adapter is additionally shown to admit a deliberately wrong but PoW-valid selected-parent map — demonstrating why this proof composition is necessary. Removing an unselected sibling or changing frozen genesis fails the composed diagnostic.

A second bounded regression authenticates a complete linear 20-header native-v3 trial and proves that the strict near-genesis retarget guard **rejects** the latest tip when the 20-sample window no longer contains the frozen genesis. It does **not** pretend that a window of 20 valid PoW samples authenticates a historical pruned boundary or a 21st ancestor.

The implementation is **test-only**: a single new `crates/pulsedag-core/tests/native_v3_derived_selected_retarget_genesis.rs` file plus CI path/test/Clippy coverage and this document. The GHOSTDAG candidate remains uncalled by production modules; the CI scanner allowlists only the added integration test. No changes to production consensus, target/retarget formulas, native mining templates, P2P, persistence, rewards/UTXO, startup guards, network identity or contracts.

## What is still blocked

- Explicit protocol-owner freeze of V3-GD-01..07, including target-work numerator/width, `K`, eligible parent count, tie/merge ordering, exact score/work recurrence, and validated retarget policy. **K=0 here is just one diagnostic input; K=32/8 parents remain unapproved proposals.**
- Authentication of later **pruned/retarget checkpoint boundaries**, proof-carrying restart/rollback, and full native 20-sample retarget ancestry for production chains.
- Candidate-normative agreement on blue/red economic inclusion, ordinary signed transactions, UTXO state roots, reward/fee finality and atomic DAG reorgs.
- Actual native-v3 500ms multi-node runtime and network-bound isolated monetary rehearsal, exact-candidate security and formal [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) launch GO.

**Native monetary-v3 mining OFF; smart contracts INACTIVE/fail-closed; public mainnet and parallel testnet NO-GO.**