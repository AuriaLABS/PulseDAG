# Native-v3 real PoW: candidate red blocks cannot bypass signed UTXO conflicts

This is a deliberately narrow **test-only and non-normative** economic safety regression based on merged `main@504db317ab6c1393657f58e92e0867c544cfadeb` ([#1362](https://github.com/AuriaLABS/PulseDAG/pull/1362)). No production runtime, native consensus selection, wallet, P2P, mining, monetary state, RPC, storage or contracts source is changed.

We now have three independent diagnostic surfaces in `main`:
- Genuine KHeavyHash PoW + signed-v2 transaction **prestate preview** with exact input=output+fee, synthetic trusted funding and explicit caller order (from #1361).
- A complete native block/amountless reward claim **envelope inspector** that continues to **reject all ordinary transfers** because canonical money ordering and state-root authority are missing (#1360).
- An unapproved, **test-only** parent-before-child full-DAG order preview preserving blue/red block existence (#1362). Its output is NEVER passed to an economic executor.

A new regression in the existing exact-head Rust test mines two competing native-v3 sibling blocks carrying genuinely chain-bound Ed25519-v2 transactions spending one and the same **synthetic** 100-atom UTXO. Standalone prestate checks verify fee=10 and fee=20 individually. It mines a separate genuinely KHeavyHash-valid **join** for candidate `K=0` and `K=1`. The order preview must contain **all 4 authenticated headers exactly once**, respect ancestry, and classify the losing sibling as red for `K=0` or blue for `K=1`. The independent signed prestate evaluator rejects **both** manually supplied complete A/B and B/A sequences (rather than silently dropping a red block or awarding both 10+20 fees), leaving the immutable synthetic UTXO root untouched. This is a *negative consistency boundary*, not a proposal for true red-block reward or transaction semantics.

The test file already runs via the SHA-exact `v3-dormant-ghostdag-core-probe.yml` and workspace-wide lint/pre-burn-in workflows. Its test-only import of the independent preview helper does not add a production GHOSTDAG callsite.

**Still NOT proven:** native consensus and economics policy freeze (trial K32/8-parent), red-block transaction inclusion/fee disposition, canonical selected-parent/total order, PoW-driven committed monetary UTXO state root, reward finality/maturity, 500ms physical propagation, atomic checkpoint/restart, or all-node agreement. In particular *rejecting a conflicting pair* does **not** determine which valid branch wins or whether any red block's transaction may be included.

**Constraints:** native v3 monetary mining OFF; smart contracts INACTIVE/fail-closed; mainnet and public parallel testnet **NO-GO** until #781 authorization.
