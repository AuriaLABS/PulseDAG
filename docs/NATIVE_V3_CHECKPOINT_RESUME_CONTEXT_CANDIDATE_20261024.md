# Native-v3 checkpoint/resume context — candidate evidence contract

**2026-10-24 — DESIGN ONLY; UNAPPROVED (#1309 / V3-GD-07, V3-GD-09).**

This is a proposal for evidence that a *future* versioned checkpoint and continued DAG validation would have to carry. It is **not** the checkpoint implementation, an activation decision, trusted snapshot, proof of finality, pruning policy, or complete retarget witness. Current native-v3 bounded verifiers deliberately require **all ancestors to frozen genesis** and reject pruned or >128-header histories. They must keep doing so until an approved versioned implementation replaces that behavior with equivalent authenticated evidence.

## Why the current 128-header cap is not a protocol

At 500ms per block, a continuously produced **linear** history would exceed 128 non-genesis headers after about 64 seconds. A multiparent DAG can exhaust the same bounded diagnostic budget sooner. Raising the cap without a measured memory/CPU and pruning protocol is not a fix. The alternative of trusting a locally stored checksum is worse: an attacker can alter selected parent, work or colors *and then recompute its SHA256 checksum*.

The isolated test `candidate_retention_inventory_revalidates_full_history_and_rejects_mutations` in `crates/pulsedag-core/tests/native_v3_ghostdag_perf_probe.rs` uses **real accepted KHeavyHash** native headers and fully rederives the entire prefix. It produces two review-only lists:

- **Recent selected spine candidates:** up to 20 NON-GENESIS recent selected-parent IDs, tip-first. This is merely an **inventory**, not the exact V3-GD-07 20-sample retarget specification or enough evidence to prune. The specific retarget sampling rule and its genesis/boundary behavior must be frozen separately.
- **Colored side headers:** sorted/deduplicated union of all merge-blue and merge-red block IDs in the complete authenticated trial evaluation, including red blocks. A selected-spine-only retention rule loses necessary DAG ordering/anticone/monetary context.

It records test chain identity, exact frozen genesis, selected tip, experimental K and a domain-separated **informational** full-history fingerprint. All fields are re-derived from **full** authentic source; peer-provided copies of these values have no authority, and missing ancestors always fail closed.

## Candidate frozen checkpoint commitment (all items must bind canonically)

| Evidence | Required exact binding / rejection rule | Current support |
| --- | --- | --- |
| Trust root | Network-specific, independently frozen genesis and policy/version/activation ID, canonical serialization, chosen trust/approval authority; cross-network and downgrade rejection | Frozen test genesis only; **no approved checkpoint authority** |
| Selected DAG | Boundary block hash, externally anchored selected ancestor, height, timestamp, target, 512-bit PoW-weighted cumulative blue-work, selected parents, committed blue-scores, exact K and tie-break | Trial-only full-history recomputation |
| Merge topology | Complete committed blue-past frontier and anticone relation witnesses, ordered merge blue/red sets including red blocks and deterministic total block/transaction order | Full-prefix test-only colored-side inventory; **no compressed proof** |
| Retarget | Selected-chain history sufficient for the **actual approved** nanosecond retarget algorithm, 20-sample indices, oldest sample, precise target and genesis/pruning boundary proofs | Last-20 selected IDs **only**, not normative sampling or proof |
| Economic state | UTXO/state-root, accepted transaction IDs, ordered-DAG digest, realized fee allocations, monetary emission, reward maturity and settlement claims, conflict replay, exact finality/reorg rollback point | Separate isolated synthetic reorg tests; **no native-v3 signed monetary proof or authoritative snapshot** |
| Host/resource | Bounded size/edges, missing-parent and malicious fanout behavior, optional proof chunking, state/schema versions, integrity checks, independent test vectors | 128 headers / 2,048 edges local diagnostics only |
| Storage/restart | Atomic stored checkpoint+state+header frontier commit, power-loss recovery, rollback to last verified state, peer replay, trusted anchor update; never accept self-signed checksum | No native-v3 RocksDB checkpoint path |
| Empirical security | True 500ms >=25-node/16-miner adversarial network corpus, propagation/clock skew, anticone tails, CPU/memory/pruning/DoS/reorg and retarget performance | Toy probabilistic sensitivity and single-CI-run CPU/RSS only |

**A hash commits bytes but cannot create trust.** A checkpoint cannot be accepted merely because its digest matches one supplied in the same file, or because a peer offers a signature with no approved trust anchor. In particular, accumulated work/selected-parent scores must be derived or authenticated by a frozen proof rather than asserted. The source of trusted authority and revision/upgrade/rollback rules are still decisions for the project owner to freeze; there is no external-auditor prerequisite.

## Fail-closed design and acceptance route

1. **Now:** retain complete ancestry and verify KHeavyHash and frozen genesis; reject omitted ancestors and over-budget graphs; do not prune, do not accept stored GHOSTDAG metadata as authority. Test-only side/context inventories are **not** sufficient.
2. **Candidate freeze:** approve native-v3 genesis and consensus version, exact K, maximum direct parents, selected-parent/blue-work/merge order, retarget and reward/UTXO rules, and a separate checkpoint authority model. Publish byte-frozen independent cross-language vectors.
3. **Proof format and verifier:** define canonically encoded boundary evidence with explicit fail-closed behavior on unknown version, duplicate/unordered/omitted ancestor, forged parent, K, blue-work, red/blue list, target samples, fee/reward/UTXO roots and stale/cross-chain manifest. Verify against *independently authenticated* trust roots. Test equivalence with full-history replay, including multiple competing forks crossing the boundary.
4. **Durable transitions:** atomic storage+RocksDB startup/restoration and recovery tests, crash in each commit stage, reorg/rejoin/pruned replay and backfill behavior. No accepting placeholder roots, no rollback that resurrects double spends.
5. **Operational readiness:** adversarial network propagation under 500ms with sufficient peer/worker topology, sustained CPU/RSS/IO measurements, post-pruning finality bounds, and exact release SHA CI. Only then separately request authorization of monetary-v3 mining **in an isolated, private pre-mainnet rehearsal**. Production/public network remains NO-GO without #781 approval; smart contracts remain INACTIVE in v3.0.

This document is deliberately **not** a protocol freeze or authority to change consensus, the startup guard, live node storage, mining, P2P, wallet or contract execution.

## Authentic derived retarget: explicit 20-sample check versus forged selection (2026-10-24)

The standalone TEST-ONLY `retarget_must_use_recomputed_genuine_kheavyhash_selected_parent_not_peer_cache` exercises the **existing** `retarget_context_v3::selected_parent_retarget_window_v3_ns` and `retarget_genesis_guard_v3::verify_dormant_frozen_genesis_retarget_v3_ns` rather than inventing difficulty arithmetic. It derives a selected-parent map from an entirely authenticated native-v3 KHeavyHash/GHOSTDAG trial **before** asking the real retarget adapter to produce the sample window. Any saved peer/cache map must match the fresh trial exactly, or fail. The test deliberately does not approve/freeze K nor choose difficulty for mining.

For a 120-deep linear 500ms test chain, the 20 newest-first samples are exact heights 120..101; the arithmetic sees nineteen 500ms intervals. The near-genesis-only retarget guard rejects this far-from-genesis tip as intended. Truncating the old history despite retaining those twenty PoW-valid samples also **fails**, because neither those samples nor an informational prefix inventory authorizes pruned consensus.

For an eight-parent K6 fork, the complete authentic DAG chooses one PoW-valid sibling deterministically, and the native retarget window and near-genesis guard agree. The test **intentionally replaces that winner** with a different real PoW-valid listed sibling: the raw retarget context adapter alone may consider that *locally structurally valid* parent map and produce a different sampling window, because it does not derive consensus choice; the new independent-trial wrapper rejects the forged parent map. Missing side ancestry and K6-to-K7 substitution also fail closed.

This is a non-activating bounded *composition proof*; it DOES NOT provide the authenticated pruning boundary, an actual production selected-tip policy, signed checkpoint authority, full v3 monetary/UTXO commitment, or long-lived retarget history. All 128-header and complete-genesis requirements remain enforced.
