# Native-v3 authentic PoW + signed transfer prestate boundary (INERT)

This *test-only* bridge extends the merged [#1358](https://github.com/AuriaLABS/PulseDAG/pull/1358), [#1359](https://github.com/AuriaLABS/PulseDAG/pull/1359) and [#1360](https://github.com/AuriaLABS/PulseDAG/pull/1360) diagnostic layers on their exact integrated `main@94cecc6ecb6da42eafaf30b9904523ffee813bd3`. It does **not** create a new consensus admission function. It touches only this documentation, one Rust integration test and the existing exact-SHA gated workflow.

**Why:** `native_v3_envelope_trial` authenticates complete DAG envelope commitments but deliberately rejects ALL ordinary transfers, because native monetary DAG order/reward/finality/state-root authority is unimplemented. Separately, `native_v3_signed_prestate_trial` verifies Ed25519 v2-wire inputs, exact fee conservation and an ephemeral UTXO root against a **caller-supplied synthetic trusted prestate**, but it does not verify native PoW, any parent DAG, finality or the committed state root. These boundaries must not be silently conflated.

The three new regressions mine **genuine KHeavyHash-valid native v3 nanosecond headers**, each with a correctly committed Merkle root, canonical v2 txids and a zero-value reward claim, then check:

1. A good Ed25519 spend has a valid speculative prestate delta with exact fee=10 while full-envelope consensus preflight **continues to reject** the ordinary-transfer block pending authenticated native monetary total-order/replay, even though the header has true PoW. The header deliberately commits an unrelated placeholder state root; it is *not* validated as economic authority.
2. Two competing real-PoW sibling blocks carry distinct signed v2 transfers spending the **same synthetic UTXO**. Each standalone trial is internally valid, but either caller-provided two-block order must reject the double-spend without mutating the original trusted prestate or crediting both fees.
3. An invalid Ed25519 signature cannot be made valid by rebuilding a canonical transaction hash, Merkle root and genuine native PoW; hidden additional amountless claims are rejected; an enabled-contract prestate must fail closed.

The test is registered in the exact-head `v3-dormant-ghostdag-core-probe.yml` alongside both pre-existing adversarial KHeavyHash/K-matrix and complete-envelope/claim tests. The workflow's static scanner still rejects any new **production Rust** callsite of the unapproved native GHOSTDAG candidate.

**Not covered/authorized:** canonical GHOSTDAG K/blue-work policy freeze, selecting a definitive total DAG monetary order, multi-block state root, reward settlement/maturity/finality, authenticated checkpoint, crash-atomic replay, production 500ms network convergence, PQC transaction signature activation, or any active node/miner/P2P/RPC/contract path. Source economic state is made up in tests, not funded by zero-allocation genesis.

**v3 monetary mining: OFF/fail closed. Smart contracts: INACTIVE. Public testnet/mainnet issue #781: NO-GO.**
