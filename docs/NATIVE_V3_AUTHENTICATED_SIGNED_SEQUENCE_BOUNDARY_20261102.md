# Native-v3: independently authenticated PoW ancestry vs conditional signed UTXO replay

**DORMANT / TEST ONLY. Not production monetary consensus.**

Exact source baseline: `main@4b24bb4f729e68b79d497c886161aca076dea2ff` after merged [#1363](https://github.com/AuriaLABS/PulseDAG/pull/1363) (24/24 exact-head green).

This independent Rust integration suite on *one exact SHA* exercises **two separate, previously implemented trust boundaries** without combining them into an authorization:

1. `derive_dormant_native_v3_topological_evaluation` checks true-KHeavyHash native header PoW, independently frozen zero-allocation genesis, complete bounded parent ancestry, nanosecond timestamp/height and parent-before-child evaluation. Its height/hash order is **NOT** approved GHOSTDAG, blue/red economic ordering or a selected-parent authority.
2. `inspect_dormant_native_v3_explicit_order_trial` validates real chain-bound Ed25519 signed-v2 transfers, exact inputs = outputs + fees, duplicate-transaction checks and an isolated UTXO root/fee attribution against a separately provided **test-injected, synthetic trusted prestate**. It does NOT check PoW, genesis, DAG or the header's committed state root.

**Invariants covered:**

- Real valid-PoW native parent chain with two linked signed spends (`100 → 90 + 10 fee → 80 + 10 fee`) has a separately authenticated ancestry and a **conditional** test replay with exactly 20 candidate fee atoms, two transfer counts, per-block 10+10 accounting and an independent UTXO-root match. Reverse caller order fails. Source prestate remains unmodified.
- Two competing genuine-PoW siblings spending **distinct synthetic UTXOs** lead to equal conditional UTXO roots and fees in both explicitly supplied replay orders. This only demonstrates that two *particular independent* spends commute; it does **not** freeze or infer a total DAG monetary order.
- A valid signed replay **cannot** authenticate a missing real-PoW parent or a wrong-network genesis. Those cases fail the independent full-DAG verifier even if the caller-provided conditional UTXO order would succeed. Header `state_root` is deliberately a placeholder and must never be treated as authenticated.

Critical safety property: **the computed topological order is not fed to the monetary trial**. The two proofs remain independently checked, because choosing the authoritative native-v3 DAG **economic** order is still an open consensus-policy decision under [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309). No selected-parent/K/blue-work/red treatment, fees to miner, claim maturity, reward issuance, atomic state commitment, pruned checkpoint authority, real 500ms network cadence or production activation is implied.

The source diff changes only a new `crates/pulsedag-core/tests/` file, a few test-gate lines in the existing exact-head dormant CI workflow and this document. A production compiler module, node entrypoint, mining, P2P, RPC, state/reward replay or contract config is **not** modified. Rust `cargo fmt`, the focused real-PoW integration tests, strict Clippy and the rest of exact-head gates must all pass before any merge.

**Mining monetary-v3 OFF; smart contracts INACTIVE; mainnet and parallel public testnet #781 NO-GO.**
