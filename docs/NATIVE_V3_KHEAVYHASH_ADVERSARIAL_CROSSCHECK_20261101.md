# Real-KHeavyHash GHOSTDAG trial K boundaries — dormant, not normative

**NO POLICY FREEZE / NO NETWORK AUTHORIZATION.** This is a dependent adversarial test set on the DRAFT native Rust cross-check [#1358](https://github.com/AuriaLABS/PulseDAG/pull/1358). Unlike the symbolic work model integrated in [#1357](https://github.com/AuriaLABS/PulseDAG/pull/1357), these regression tests construct truly PoW-valid native-v3 nanosecond headers against independently frozen **test-only** genesis and authentic KHeavyHash targets. The K-cluster and target-work formula are still unapproved proposals.

## Exact, isolated scope

`crates/pulsedag-core/tests/native_v3_ghostdag_k_pow_boundaries.rs` extracts five adversarial tests from the broad experimental [#1341](https://github.com/AuriaLABS/PulseDAG/pull/1341) **without** copying its detached UTXO support or native monetary replay/state changes:

- Real target-work disagreement: two work=2 siblings versus one hard work=512 sibling, selected parent derived as the hard sibling independent of height/hash; K=0/1/2 exact blue/red partitions, score and cumulative trial work.
- Thirty-four truly mined sibling headers: K=32 accepts exactly 32 side blues while K=33 accepts 33, rejecting a forged committed blue score despite genuine KHeavyHash PoW. This 34-parent case is deliberately a **stress boundary**, NOT a proposal for a 34-parent production cap; current header format is broader than unapproved 8-parent policy.
- Eight real-PoW genesis siblings: K=0/6/7/32 colors and exact cumulative work; unsorted untrusted tip frontiers reject until the caller canonicalizes them, and incomplete side ancestry rejects.
- Inherited blue-past anticone saturation: a new merge block must fail the **second** trial K-cluster condition at K=2 even when the new block itself is within K; at K=3 it may enter the blues. Map insertion permutation does not change exact evaluation.
- Multi-depth real-PoW eight-parent-or-smaller branch matrix: trial K cutoffs 0, depth-1, depth, 2depth-1, 2depth and 32; deterministic score/work/blue count and insertion-order invariance.

The dedicated SHA-pinned workflow inherited from #1358 now explicitly checks the new test and strict Clippy, alongside its independent Python-Rust full-byte vectors. No runtime activation is possible from the test file. No production networking, P2P, mined admission, retarget policy, transactions, UTXO, storage, smart contracts or startup code changes.

## Reproduce

```bash
cargo test --locked -p pulsedag-core --test native_v3_ghostdag_k_pow_boundaries -- --nocapture
cargo clippy --locked -p pulsedag-core --test native_v3_ghostdag_k_pow_boundaries -- -D warnings
```

**Merge ordering:** #1358 has merged into `main` as `636a2daedd483c16a74cec10684e8bf42720ec64` after its 29/29 exact-head workflows passed (including monetary reachability after rerunning the disk-exhausted job). This candidate is re-anchored on that merged `main` and needs its own full exact-head CI before being merged. The ongoing policy freeze #1309, 500ms measured 25-node/16-miner propagation, trial K=32/8 cap safety and v3 monetary DAG state authority remain unproven. Monetary mining OFF; contracts INACTIVE; mainnet #781 NO-GO.
