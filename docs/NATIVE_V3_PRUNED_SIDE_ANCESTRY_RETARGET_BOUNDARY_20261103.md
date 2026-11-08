# Native-v3 GD-07: a valid 20-sample retarget window is NOT a checkpoint proof

**Scope:** Test-only negative evidence for [#1367](https://github.com/AuriaLABS/PulseDAG/issues/1367), which depends on [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309). Base: exact `main@b6b8ceb281174445d5135a58f14365039aedf7db`. **DORMANT, UNAPPROVED; no production admission or monetary activation.**

## Regression boundary

`native_v3_derived_selected_retarget_genesis.rs` extends the existing real-KHeavyHash trial to an authenticated **21-height multiparent DAG**: two height-one siblings have distinct easy/hard targets, and their common descendant joins them at height two. The experimental **K=0** candidate selects the higher-work sibling, leaving the other a red/nonselected required ancestor. Subsequent real-PoW headers extend the selected chain to height 21. Every header and fork edge is originally available and verified by the dormant full-parent-DAG trial.

The independent *20-sample* selected-parent retarget adapter succeeds on the complete corpus. Then two independent negative experiments show its precise limit:

1. Delete the red/nonselected sibling from the supplied headers while preserving the 20 recent selected blocks. The identical 20-sample retarget window still passes, but deriving the full-DAG candidate again **must fail** on missing side ancestry.
2. In a separate copy, delete the frozen genesis while retaining that same 20-header selected window. The retarget adapter can still construct the same PoW-valid window, but the full-DAG candidate **must fail**. Even on the intact corpus, the strict frozen-genesis retarget guard rejects a window whose own 20 samples do not contain the genesis.

This is a **negative trust-boundary regression**. It intentionally does not implement or accept checkpoints. The retarget-window adapter is not a DAG authority or checkpoint verifier; its successful return must never be elevated to trusted historical continuity, work, red/blue completeness, monetary UTXO/reward replay, or a pruning authorization.

## Still required for checkpoint acceptance

Formal GD-01..07 consensus-policy freeze, independently authenticated checkpoint chain/genesis/version/policy identity, trustworthy historical-work and selected-spine commitments, canonical GD-08 red-inclusive monetary ordering, byte-for-byte full-DAG vs pruned restore/retarget/fee/UTXO/reward comparison, crash/reorg atomicity and adversarial invalid checkpoint tests. No proof here covers production retarget with genuine authenticated pruned history or production 500ms convergence.

**Native-v3 monetary mining OFF. Smart contracts INACTIVE/fail-closed in v3.0. [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) public testnet NO-GO first, and mainnet NO-GO later under separate decisions. The real network pruning/rejoin proof belongs to the authorized public testnet after its GO; pre-GO checks remain local/CI.**
