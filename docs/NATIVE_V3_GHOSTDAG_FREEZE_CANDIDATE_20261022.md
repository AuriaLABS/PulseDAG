# PulseDAG native-v3 GHOSTDAG: concrete freeze candidate (NOT APPROVED)

Status: **PROPOSED DESIGN ONLY** (2026-10-22). This is a reviewable candidate for [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309), not normative protocol, not permission to mine, and not a replacement for [the unapproved ten-part freeze RFC](NATIVE_V3_GHOSTDAG_POLICY_FREEZE_RFC.md). In particular, the numerical K and resource budgets lack the empirical security/DoS evidence needed for sign-off.

## Proposed decisions / explicit remaining gates

| Decision | Concrete proposal | Remaining freeze gate |
| --- | --- | --- |
| V3-GD-01 | A fresh, network-bound `BlockHeaderV3` / KHeavyHash / genesis identity and independently frozen canonical hash; no migration/inheritance of legacy GHOSTDAG v1 scores, work or v2 blocks | Freeze chain/genesis/header/algorithm IDs and exact bytes for rehearsal/mainnet independently |
| V3-GD-02 | **Trial K = 32** for comparative 500ms simulations only; maximum 8 eligible parents *proposed* (the dormant header format currently permits 64); use *inclusive* bound `anticone_blues <= K`; reject non-antichain parent frontiers rather than reinterpret redundant parents | A defensible chosen network delay bound, mining/block-arrival model, adversarial tolerance, false-red target, exhaustive corner cases and reproducible multi-node simulation; benchmark and then choose K, parent cap and exact ancestor/merge/anticone traversal limits. **No K is approved yet** |
| V3-GD-03 | Target-weighted integer work `work(T)=floor(2^256/(T+1))` for every PoW-valid non-genesis block. Validate canonical compact-target bytes and KHeavyHash before using T; genesis contributes **0** work. No floats, unit-work carryover or alternate numerator. | Compare #1340's `2^256` and `2^256-1` alternatives and sign off exact target arithmetic / genesis semantics with independent Rust/Python vectors |
| V3-GD-04 | Derive all parent metadata from complete, independently verified native-v3 ancestry in deterministic topological order. Select maximum `(blue_work, blue_score, height)`, then the **lexicographically smallest 32-byte canonical hash** on an exact tie. Nothing stored or P2P-supplied is authority. | Golden vectors for equal work / divergent score and height, byte ordering, selected-chain reorgs and side-DAG corruption |
| V3-GD-05 | Merge set is the union of other immediate parents' inclusive pasts minus the selected parent's inclusive past. Deterministic *topological* candidate processing via Kahn's algorithm, selecting among currently eligible vertices by **descending** independently derived `(inclusive_blue_work, blue_score, height)` and then **ascending** 32-byte canonical hash. Every vertex's predecessors must be processed first; never sort all merge-set blocks globally if that would place a child before its ancestor. Add a merge block to blues only when **both** GHOSTDAG K-cluster conditions hold: (a) it has at most K incomparable blue blocks in the selected parent's blue past plus previously accepted merge blues; and (b) adding it leaves each existing blue block with at most K incomparable blue blocks. Otherwise classify valid candidate red. | Authenticate the selected-parent blue-past frontier, prove efficient checked K accounting under bounded/pruned history, freeze how reds enter final total block/transaction order, and publish independent colored topological vectors |
| V3-GD-06 | Work/score definitions are **inclusive of the current non-genesis block**. For candidate H with selected parent P and blue merge set B(H): `score(H) = score(P) + 1 + |B(H)|`; **`blue_work(H) = blue_work(P) + work(H) + sum(work(b) for b in B(H))`**. Genesis `score(G)=0`, `blue_work(G)=0`, `work(G)=0`. Require `H.blue_score == score(H)` before acceptance; recompute and never trust a supplied work value. Use checked **u512** accumulation and deterministic overflow error; no wrapping/saturation. | Cross-language exact 256/512-bit vectors, check treatment of genesis and parent work, selected-parent monotonicity, unique-blue accounting, and signed consensus-version arithmetic |
| V3-GD-07 | Recompute retarget's 20-sample ancestry from the **derived** selected-parent chain and authenticated nanosecond timestamps; no caller-provided selected-parent map as authority. Reject incomplete/pruned boundary until network-specific authenticated proof exists. | Freeze near-genesis window, sample boundaries, old/new difficulty exactness, pruning/finality and snapshot proof format |

The above is a **candidate**, including K=32, an eight-parent cap and u512 work width. None are approved constants, and this file is not consulted by live consensus. If security or interoperability benchmarks disagree, revise the candidate and regenerate all vectors before any protocol-owner freeze.

### Critical counterexample: include the current block's own PoW in blue-work

For genesis G and two independently PoW-valid siblings A and B (each having exactly G as parent), assume illustrative compact targets yielding `work(A)=2` and `work(B)=512` under the proposed target-weighted formula from #1340. If blue-work instead includes `work(G)` in each child's recurrence, `blue_work(A)=blue_work(B)=0` and parent selection ignores a 256-fold difficulty difference. This is **not acceptable** if blue-work is to prioritize verified PoW. The **inclusive candidate recurrence** gives `blue_work(A)=2` and `blue_work(B)=512`, so a later child citing A and B selects B on work. **Illustrative calculation only**, not a PoW-valid frozen header vector.

The invariant to test at every block H is that `blue_work(H)` includes `work(H)` **exactly once** and excludes blue merge blocks already in the selected parent's inclusive past. `B(H)` must be a unique-hash set derived by set difference against that past. For any one-parent chain with no blue merges, `blue_work(H) - blue_work(P) == work(H)` and `score(H) - score(P) == 1`. For siblings with unequal validated targets and otherwise equal ancestry, the larger work contribution must win selection without height/hash ties. Score or work overflow must reject, never saturate.

**Ordering gate:** For two graph-incomparable merge-set candidates with identical prerequisites but differing *independently derived inclusive* blue-work, the heavier candidate is processed first under the proposed greedy classification. Topological precedence always wins (no descendant before an ancestor). Freeze needs independent PoW-valid examples for same-height work-differentiated forks, ties by hash bytes and at K vs K+1 anticone pressure.

### Formal processing order proposed for a later dormant Rust implementation

1. Bind to a frozen network/genesis/version. Check canonical header bytes, unique sorted parent hashes, canonical target, immutable PoW digest, temporal/height constraints, and **complete** ancestor closure including *all* side parents. Missing data: stage/defer; invalid data or contradictory closure: reject. Resource exhaustion must never promote a partially evaluated graph to valid.
2. Traverse the fully authenticated DAG in a deterministic parent-before-child order; independently recompute metadata for each parent **before** scoring its child. Never use legacy `ChainState.dag.blue_work` or unverified stored `selected_parent`.
3. For each child, choose selected parent under V3-GD-04. Compute selected-parent-relative merge set, blue/red partition under the proposed inclusive K rule, then score and checked blue work under V3-GD-06. Validate its committed score exactly. This is only a pure dormant computation until all freeze/evidence requirements pass.
4. Derive an explicit ordered selected chain and use it to verify native retarget. Version all derived caches; after restart, reconstruct from authenticated immutable input and compare cached digests, failing closed on mismatch.
5. **Do not apply money yet:** a later separately reviewed V3-GD-08/09 implementation must specify block-envelopes, topologically deterministic blue/red transaction order, issuance/reward/fees, UTXO spend conflicts, reorgs, atomic commits, P2P/mined admission and snapshot recovery. Neither blue/red color alone nor legacy `types::Block` replay decides spend validity.

### Executable selected-parent/K/blue-work candidate reference (symbolic, NOT consensus)

A new **non-activating** Python-standard-library reference now turns this precise proposal into executable *symbolic* expectations: [`scripts/native_v3_ghostdag_policy_candidate.py`](../scripts/native_v3_ghostdag_policy_candidate.py). It recomputes parent metadata only after ancestry is available, selects parents from independently assigned integer work, greedily applies both proposed K-cluster tests, and calculates checked u512 inclusive blue-work. Its [exact-head workflow](../.github/workflows/v3-ghostdag-policy-candidate.yml) runs regular and `python3 -O` executions, with explicit checks that optimizations cannot suppress.

Reproduction:

```bash
python3 scripts/native_v3_ghostdag_policy_candidate.py
python3 -O scripts/native_v3_ghostdag_policy_candidate.py
```

**Worked expected outputs (symbolic values, not real PoW headers):**

| Graph / K | Chosen parent | Merge blues | Merge reds | New `blue_score` | New `blue_work` |
| --- | --- | --- | --- | ---: | ---: |
| `G -> A(2), G -> B(512), {A,B} -> C(4)`, K=0 | B | none | A | 2 | 516 |
| Same fork/rejoin, K=1 | B | A | none | 3 | 518 |
| `G -> A(2), B(512), D(32); {A,B,D} -> X(4)`, K=1 | B | D | A | 3 | 548 |
| Same three-parent merge, K=2 | B | D then A | none | 4 | 550 |

The reference checks all **120 map-arrival permutations** of the five-vertex example, invalid/cyclic/redundant-parent closure, and u256-limit crossing: two hypothetical `2^255` contributions accumulate to `2^256` in the proposed u512 total. It does **not** exercise 500ms networking, mine headers, authenticate genesis, validate canonical compact targets/KHeavyHash, compare a separate Rust implementation, derive actual canonical hash-byte tie breaks, or select a justified production K/parent cap/budget. Any such claim is explicitly forbidden. This is directly tied to V3-GD-02..06 **policy evaluation**, not a runtime or a consensus activation flag. The symbolic K=0,1,2 tests do **not** validate the unrelated trial K=32.

### Exact-head real native-v3 header/PoW bridge (2026-10-22; non-normative)

The accompanying `scripts/native_v3_ghostdag_kheavyhash_bridge.py` now connects the otherwise symbolic candidate classifier to **actual canonical v3 SHA256 header hashes and independently checked KHeavyHash** in a deterministic *test-only* fork/rejoin. It derives one unmined local genesis SHA256 domain-bound to a distinct test chain ID, mines two easy-target (0x207fffff) PoW-valid sibling headers referencing that exact genesis, and mines a two-parent rejoin committing the sorted real parent hashes and computed header `blue_score`. It verifies compact-target work contribution 2, independent KHeavyHash < = target, nanosecond/height structure, genesis chain-domain separation, equal-work tie by **actual canonical header hash bytes**, and the proposed K=0/K=1 colors and inclusive work from the symbolic scorer.

Run at exact head through `.github/workflows/v3-ghostdag-policy-candidate.yml`, or locally:

```bash
python3 scripts/native_v3_ghostdag_policy_candidate.py
python3 -O scripts/native_v3_ghostdag_policy_candidate.py
python3 scripts/native_v3_ghostdag_kheavyhash_bridge.py
```

The symbolic K=0 result for the join is score=2, cumulative trial work=4, with the lexically greater sibling red; K=1 yields score=3, cumulative trial work=6, with both siblings contributing to the blue set under the *candidate* rule. The generated headers are deterministic given this Python implementation but **their individual hash/nonces are not yet frozen into canonical independent cross-language golden vectors**. The test-only merkle/state roots are placeholders; **do not claim monetary state validity, authorized native GHOSTDAG admission, production genesis authentication, end-to-end retarget, K=32 security, miner activation, or any independent Rust consensus implementation**. A separate native-Rust selected-parent/classifier, frozen full-byte cross-language acceptance vectors, and #1309 explicit policy approval are still missing.

### Adversarial acceptance vectors required before approval

- Same authenticated DAG presented in reversed and arbitrary peer/index/map order must produce identical selected parents, blue/red order, scores, 512-bit work and selected tip.
- One-parent chain, two equal-work siblings, three-parent fork/rejoin, redundant direct parent, selected-parent tied on work but differing blue score, blue score overflow, work overflow, cross-network PoW, missing side-branch PoW, invalid compact target, exactly K versus K+1 incomparable blue blocks.
- Two PoW-valid minimum-target contributions that overflow u256 must remain correct in u512; manually supplied/truncated `blue_work` must never win selection. Handle duplicated candidates, ancestor chain pruning and restart.
- Compare two **separate implementations** on chain-bound full-byte fixtures with canonical header hash, KHeavyHash digest, target bytes, selected-parent, colors/order, committed blue score, blue-work and exact reject code. Pure symbolic graph fixtures and Python-only #1340 arithmetic are not sufficient.
- For monetary rollout: deterministic 500ms multi-node convergence, reward/fee supply accounting, UTXO double-spend prevention, native mined/P2P admission, atomic persistence, rollback/reorg, wallet and miner templates, isolated non-public network rehearsal. Public launch requires the independent #781 formal GO.

### K and resource-budget approval is the immediate decision blocker

The trial `K=32` is intentionally **not** a safety claim. K depends on the actual 2 blocks/s rate, the selected worst-case network propagation envelope (not just median RTT), block-arrival/tail assumptions and desired threat probability. Historical v1 `K=2` and local 128-header/2048-edge diagnostics are **not** native-v3 constants. Likewise, an 8-parent cap is a **policy candidate**, not proof of interoperability. Before freeze, include measured worst-case/quantile delay, adversarial edge/merge constructions, CPU/memory under intended 500ms production workload, and signed numeric K/parent/visit budgets with deterministic fail-closed handling.

### Smart contracts stay complete-in-code / INACTIVE-in-consensus

Per [#1333](https://github.com/AuriaLABS/PulseDAG/issues/1333), fully build and test the bounded compiler/VM/state core offline, including default and `executable-contracts` builds. These are **not** v3.0 network activation. Ensure no contract envelope, state effect, execution, public callable endpoint, feature flag or upgrade trigger affects v3.0 DAG selection, PoW/retarget, rewards, UTXO, P2P, mining template, RPC, persistence or restarts. Pin disabled-feature versus compiled-but-disabled comparisons to the **same** active monetary-v3 fixture: identical acceptance, hash/roots, ordering, supply and UTXO state. `CONTRACT_IMPLEMENTATION_READY` remains **NOT PROVEN** pending the complete #1333 matrix.

### Activation boundary — unchanged

Current `monetary_v3` direct startup denial before storage open and persisted-sidecar denial after opening RocksDB but before runtime restore/ingress/mining (#1313) remain unchanged. No public or isolated monetary-v3 mining is authorized by this proposal. Contracts remain **INACTIVE** in public/mainnet and the future isolated rehearsal, regardless of whether the offline core is complete. The only public release authorization is #781; status: **NO-GO**.

**Approval needed:** named protocol owner explicitly selects/rejects each V3-GD-01..07 rule after recording numbered evidence, precise versioned spec hash and independent implementation vectors on #1309. Merging design discussion is never equivalent to a freeze.
