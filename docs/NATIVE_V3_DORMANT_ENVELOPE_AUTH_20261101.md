# Dormant native-v3 full-envelope authentication probe (UNAPPROVED)

This isolated DRAFT for [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) imports only two byte-identical Rust files from broad experimental [#1341](https://github.com/AuriaLABS/PulseDAG/pull/1341): a **non-activating** complete-`BlockEnvelopeV3` inspector and two real-KHeavyHash integration tests. It is based on `main` after [#1358](https://github.com/AuriaLABS/PulseDAG/pull/1358), not on #1341's experimental replay branch.

**Positive scope:** verify independently pinned test genesis, native SHA256/KHeavyHash and complete DAG ancestry, indexed block-hash/txid/Merkle body commitments, a single chain-bound amountless reward claim per mined block, and independently derived trial GHOSTDAG K=0/1 selected parent and blue-work.

**Fail-closed negatives:** reject a PoW-valid block with hidden extra inputless claim, missing reward claim/body or side parent, unrecognized extra envelope, wrong map index, altered Merkle-committed transaction, invalid committed blue score and wrong genesis/network identity. Reject **every ordinary transfer**, even with valid header PoW, canonical txid and Merkle commitment: without signed spendability and exact native monetary total-order/replay proof, header/body validation is not monetary acceptance.

`crates/pulsedag-core/src/lib.rs` only exports the dormant module. Existing exact-head Rust/Python CI is expanded to check the new integration test, Clippy, and a static callsite allowlist that permits only another dormant module to refer to the trial selector. No change to production startup, PoW target, retarget, P2P, miner, RPC, contracts, storage or `state_replay_v3.rs`.

A concurrently pending [#1359](https://github.com/AuriaLABS/PulseDAG/pull/1359) changes the same **CI workflow** for adversarial real-PoW K boundaries. Reconcile both workflow extensions and rerun exact-head CI before merging either after the other. These samples are not full signature/UTXO/reward settlement, production immutable genesis, 500ms network evidence or a freeze of **unapproved** K=32/eight parents.

**Native-v3 monetary mining: OFF/fail-closed. Contracts v3.0: INACTIVE. Mainnet/public testnet #781: NO-GO.**
