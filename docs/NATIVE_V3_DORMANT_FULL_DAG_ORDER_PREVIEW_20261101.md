# DORMANT native-v3 parent-first full-DAG order experiment

**UNAPPROVED candidate only** — not consensus, monetary replay, reward settlement, finality, storage, P2P, mining, activation or a 500ms network benchmark.

This PR is based on exact merged `main@94cecc6ecb6da42eafaf30b9904523ffee813bd3` and isolates the byte-identical **test support helper** `native_v3_trial_order_preview.rs` from huge unmerged experimental [#1341](https://github.com/AuriaLABS/PulseDAG/pull/1341). It adds an independent real-KHeavyHash Rust integration suite and exact-head CI; the test helper stays under `crates/pulsedag-core/tests/support/`, outside any production module.

The proposed **non-normative** order is a bounded parent-before-child topological sort of fully authenticated DAG headers, where first selected-chain merge anchor, unapproved blue/red classification, trial blue-work, score, height and hash give deterministic **experimental** tie-breaking. It must never be used to order transactions or pay rewards until protocol-owner policy `K` / blue-work / red-block treatment / reorg / total transaction order is frozen and independently verified.

Three real-PoW cases:

1. Three genesis siblings joined by a genuinely KHeavyHash-mined multiparent block at test K=0,1,2. Independently assert expected selected spine, score, work, blue/red counts; **all** vertices (including reds) appear exactly once, parent-before-child, deterministically irrespective of map insertion order.
2. A real-PoW two-branch, depth-two fork joined by a child, test K=2. Assert no emitted child precedes its parent, removal of a side ancestor rejects, and a **separately PoW-valid join with forged blue_score** rejects even when its corpus contains no unrelated original join.
3. Two genuine competing tips must **not** be presented as one canonical ordered DAG. The test-only ordering helper fails closed until a uniquely chosen and verified tip is supplied.

The `v3-dormant-ghostdag-core-probe.yml` gate checks the test with `cargo test` and Clippy at the exact PR head. It extends the scanner allowlist only for the two added **test-only** files; production Rust use of `native_ghostdag_policy_candidate_v3` remains a strict CI failure.

Concurrent DRAFT [#1361](https://github.com/AuriaLABS/PulseDAG/pull/1361) adds signed-prestate vs authenticated full-envelope economic separation to the **same** workflow. If either merges first, reconstruct the second against updated `main` and preserve *both* CI additions before integration. Do not inherit green CI evidence from an older base.

**Gaps:** no actual signed native DAG monetary total order, finality, reward maturity, state-root replay, stored checkpoint authority or true 25-node / 16-miner / 500ms measured propagation. K32 and proposed 8-parent cap UNAPPROVED. V3 monetary mining **OFF**; contracts **INACTIVE**; public #781 mainnet/testnet **NO-GO**.
