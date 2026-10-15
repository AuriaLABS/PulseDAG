# Native-v3 GHOSTDAG consensus policy freeze — decision record (UNAPPROVED)

**Status: OPEN RFC / NO CONSENSUS FREEZE.** Tracking issue: [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309). This document supplies reviewable decision questions and fixture contracts, **not** adopted network constants, canonical protocol rules, or permission to activate mining.

Read with [GHOSTDAG_SELECTION_DESIGN_SPEC.md](GHOSTDAG_SELECTION_DESIGN_SPEC.md), [GHOSTDAG_SELECTION_MULTI_PR_PLAN.md](GHOSTDAG_SELECTION_MULTI_PR_PLAN.md), and [ROADMAP_V3_0_0.md](ROADMAP_V3_0_0.md). Their general or v2-era proposals do not, on their own, freeze new `BlockHeaderV3` monetary consensus rules.

## Evidence already merged, but not authoritative selection

| Merged PR | Native-v3 capability | Explicit limit |
| --- | --- | --- |
| #1308 (`61d053391c1bac0130bf6b1e98ef187e9d678b24`) | Authenticate a complete, bounded, multiparent nanosecond-header DAG back to independently frozen, unmined genesis, including KHeavyHash on every non-genesis ancestor | Does not derive selected parent, accepted transactions, or work |
| #1310 (`c10e8219a9380a0fc4b8ea972543bce733b26e83`) | Given a **comparison** parent, calculate union of other parents' inclusive pasts minus its inclusive past | Does not authenticate which comparison parent wins or choose a blue/red/order policy |
| #1311 (`16eedbf684b4d8785162e90748b5c889d76e8b14`) | Classify two reachable headers as equal, ancestor/descendant or graph-incomparable (anticone) | Does not specify K, classify blues or reds, or derive blue-work |

The above use **local diagnostic caps** of 128 reachable headers, 2,048 edges and 64 supplied roots. They reject incomplete or pruned ancestry. Do **not** silently promote these local diagnostic budgets into production consensus limits, and do not treat an authenticated graph as an authenticated selected chain.

### Existing source behavior which MUST NOT silently become v3

- `selection_v2::compare_selection_scores_v1` orders stored `blue_work` (`u128`), then committed `blue_score`, then height, then lexically **lowest** hash; `calculate_selected_parent_v1` reads `ChainState.dag.blue_work` and seconds-based `types::Block`.
- `ghostdag_v1::GHOSTDAG_V1_K` currently equals **2**. This is a **v1 value**, not a decided native-v3 K.
- `ghostdag_v1::derive_score_work_v1` computes selected-parent work + 1 + count(merge-set blues), with saturating arithmetic; it **does not weight by compact PoW target** and is not sufficient proof of overflow-safe monetary-v3 work.
- `BlockHeaderV3` commits to nanosecond timestamp, parent hashes, compact target (`difficulty`), nonce, `blue_score` and height. Canonical v3 header hash and chain-bound KHeavyHash work digest are different artifacts. `pow_v3::validate_v3_pow_target` rejects noncanonical or out-of-bounds compact targets. A valid digest does not prove correct selected-parent or cumulative work.
- The current `network_block_v3`/`state_replay_v3` legacy-block pathways are **not** a native-v3 acceptance/replay pipeline. **Critical startup caveat (P1):** `PULSEDAG_PROTOCOL_CONSENSUS_MODE=monetary_v3` is rejected by the present `StartupProtocolMode` parser *before* `Storage::open`, but `ghostdag_v1` startup can open storage and then encounter a persisted `protocol_monetary_activation_record` and select the monetary-v3 runtime-restore path. Therefore a comprehensive pre-storage denial of **all** monetary paths does **not** currently exist; #1276 must close the persisted-sidecar restoration/activation gap. Do not interpret the parser rejection as proof that a persisted monetary sidecar is inert.

## Mandatory versioned protocol decisions (not yet approved)

| ID | Decision requiring explicit independent approval | Completion artifact |
| --- | --- | --- |
| V3-GD-01 | Exact consensus identity, activation/version domain, genesis identity and migration rule; forbid implicitly inheriting v1 metadata | Frozen versioned specification + network-specific genesis vectors |
| V3-GD-02 | GHOSTDAG `K`, eligible parent count, merge-set, anticone and total validation work budgets; precise reject/defer behavior | Numeric constants and adversarial bound tests |
| V3-GD-03 | For each non-genesis header's canonical difficulty target, define its **own** integer work contribution: units vs target-weighted policy; use explicit integer widths, division/rounding, genesis contribution and checked overflow | Mathematical formula and independent cross-language vectors. Do **not** assume `2^256/(target+1)` or unit work is approved |
| V3-GD-04 | Deterministic topological evaluation and selected-parent score with all tie-break fields; all required values must be recomputed from verified ancestors, never trusted from persisted maps | Pseudocode + equal-work/score/height/hash tie vectors |
| V3-GD-05 | Exact candidate merge-set **order**, blue/red anticone rule (including whether comparison is strictly `< K`), tie order and status of a red block. Parent-relative lexical set output alone is not an order | Independent ordered blue/red golden fixtures |
| V3-GD-06 | Expected `BlockHeaderV3.blue_score` derivation and equality check, cumulative blue-work recurrence and relation between the two (including `u64`/`u128`/wider integer overflow) | Checked arithmetic specification + reject-on-overflow tests |
| V3-GD-07 | Use independently derived selected chain for nanosecond retarget/20-sample window, oldest sampled ancestor and independently proven pruning/snapshot boundary | Near-genesis, oldest-window, missing/pruned-history and restart vectors |
| V3-GD-08 | Deterministic DAG total ordering and monetary state transition, duplicate UTXO/fee/reward handling, transaction-bearing native envelopes | Two-independent-implementation replay and conflict vectors |
| V3-GD-09 | Production DoS budgets, orphan and out-of-order arrival behavior, on-disk schema identities, atomic restoration and network admission order | 500ms ≥25-node/16-miner adversarial evidence and restart/rollback tests |
| V3-GD-10 | Independent human security approval, exact integrated source/tree/artifacts and launch authorization | Human review on #1305; #1276 real native runtime complete; #781 separate formal GO |

**Freeze requires a named protocol owner and independent security reviewer to record, for each ID, a chosen rule, alternative rejected, rationale, exact spec hash/revision and reproducible vectors.** Until that happens, all IDs remain open. Work on purely diagnostic helpers may continue without adopting unresolved policy.

## Fixture contracts for independent implementations

All actual hash/PoW fixture files must fix chain ID, independent frozen nanosecond genesis hash, canonical `BlockHeaderV3` bytes, nonce, compact target, KHeavyHash digest, parent sets, expected scores/work, and exact error category. Symbolic letters below are **not** cryptographic test vectors; each fixture must first be materialized as PoW-valid headers with parents sorted by their real hashes.

- **K-neutral ancestry:** `G -> A`, `G -> B`, `A,B -> M`. `A` and `B` are incomparable; `A` is an ancestor of `M`; equality is separate from anticone.
- **K-neutral merge set with COMPLETE edge list:** exactly `G -> A`, `G -> B`, `{A,B} -> M`, `A -> A2` and `B -> B2`, where each arrow is a **direct parent link** and the parents of `A`, `B`, `M`, `A2` and `B2` are respectively **only** `{G}`, `{G}`, `{A,B}`, `{A}` and `{B}`. There are no intermediate or additional cross edges. The immediate candidate parents are `{M,A2,B2}` (sorted by their actual canonical hashes in implementation). Relative to `M`, the merge set is `{A2,B2}`; relative to `A2`, it is `{B,M,B2}`; relative to `B2`, it is `{A,M,A2}`. These are **sets, not blue-ordered lists**.
- **K-neutral redundant-parent case:** `G -> A -> D` with comparison parents `A,D`: relative to `D` merge set is empty; relative to `A` it is `{D}`.
- **Full-closure diagnostic authentication negatives (#1308/#1310/#1311 only):** correct header SHA but invalid KHeavyHash on an unselected side branch; forged height-zero root; wrong chain ID; missing *any* required ancestor (including the 21st ancestor **only when the full-closure verifier is the function under test**); noncanonical parent order/duplicate; inconsistent height or nanosecond timestamp; **querying** an otherwise valid but unreachable extra map block in the #1311 pair-relation API (must fail), while an **unqueried** extra map entry is allowed and must not cause #1308's root-reachable full-closure verifier to reject; above-budget complete closure. **Do not** extrapolate the 21st-ancestor failure to the separate 20-sample retarget helper, which can validly stop its bounded sample walk there; pruning-boundary authenticity for production remains V3-GD-07 **unapproved**.
- **Policy-sensitive fixtures still BLOCKED:** equal cumulative work with unequal count/height/hash; different compact targets with different blue scores; anticone precisely at and just over `K`; candidate permutation, restart, pruning and record replay; overflowing accumulated work; v2/v3 cross-version divergence.

For every approved fixture, verify independent implementations on at least two different runtimes and report header hash, work digest, selected parent, ordered merge blues/reds, selected tip, derived blue score/work, ordering digest, accepted/rejected UTXO set and error code (or explicit `N/A` before that stage). Never report placeholder values as passing golden vectors.

## Implementation ordering after approval

1. Freeze V3-GD-01 through V3-GD-07 and approve independent integer-work/tie/anticone vectors. Do not implement an authoritative selector before this.
2. Add a **versioned, dormant** independently recomputed header-DAG scoring/classifier (fail closed for missing or over-budget history). Compare the committed v3 blue score against derived authority.
3. Authenticate retarget ancestry and its pruning proofs against the newly derived selected chain, never a caller-supplied map.
4. Introduce native transaction/reward/UTXO replay, atomic persisted DAG state, P2P/mined admission and 500ms end-to-end convergence with separate security reviews.
5. Preserve the **existing explicit monetary-v3 mode rejection before `Storage::open`**, but **do not claim it covers persisted monetary-sidecar activation**. Until #1276 closes that separately evidenced P1 gap, fail closed on any unapproved persisted monetary activation before runtime restoration, P2P/mining or other monetary side effects (ideally identify protocol identity before opening storage; otherwise deny immediately after reading the record, without activating). Independently prove the startup boundary with regressions for both explicit mode and valid/malformed monetary sidecars. Smart contracts remain **INACTIVE**; #781 is mainnet/parallel-testnet **NO-GO** without formal authorization.

**Do not merge a later PR that treats this unapproved RFC as the chosen policy.** It becomes normative only after a separate explicit freeze decision and protocol version/hash are recorded on #1309.
