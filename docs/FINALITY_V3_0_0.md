# PulseDAG v3.0.0 finality architecture

Status: **ARCHITECTURE APPROVED / PRODUCTION DURATION + CONFLICT + PRUNING FREEZE PENDING**

This document defines the v3 finality architecture used by deferred reward settlement. It does not approve a mainnet finality duration, pruning duration or conflict-resolution procedure. Those remain launch-blocking network-parameter freeze inputs.

## 1. Separate finality, maturity and pruning

PulseDAG v3 treats three concepts independently:

1. **Finality** — when an ordered-DAG prefix is protected by the active finality policy;
2. **Coinbase maturity** — the approved 3,600 economic seconds that a finalized reward must additionally satisfy before it is spendable;
3. **Pruning** — when old DAG/state data may be physically discarded while retaining enough proof/context for safe bootstrap and replay.

A finality decision is not automatically a pruning authorization. A mature reward is not spendable if its ordered position is not final. A final block is not automatically prunable.

## 2. Finality duration is economic-time based

The v3 finality policy expresses its delay in **economic seconds**, not a raw block count.

The exact production value remains `TBD`.

The implementation converts canonical monetary score to economic time using the same frozen `(activation_score, target_interval_ns)` cadence table as monetary policy. Consequently, a future cadence change alters the number of score positions required to represent the finality duration but does not silently shorten or lengthen the intended economic-time delay.

Examples of equivalence only, not approved production values:

- at 1 BPS, 1 economic hour spans approximately 3,600 monetary-score transitions;
- at 2 BPS, the same economic hour spans approximately 7,200 transitions;
- at 4 BPS, approximately 14,400 transitions.

## 3. Selected-chain anchor and ordered-prefix rule

Finality advances only to a block that is an anchor on the deterministic selected chain.

For the current canonical state:

1. derive the authoritative ordered DAG;
2. derive economic time at the current monetary score;
3. inspect selected-chain anchors in their authoritative ordered-DAG positions;
4. select the greatest anchor whose economic age is at least the frozen finality delay;
5. bind finality to the **entire deterministic ordered prefix through that anchor**.

The boundary therefore contains an ordered-DAG monetary score, exact boundary block hash and ordered-prefix digest. It is not a local arrival index and not a naked selected-chain height.

Genesis is the fail-closed floor until a non-genesis selected-chain anchor satisfies the policy.

## 4. Policy identity binds every consensus parameter

`crates/pulsedag-core/src/finality_v3.rs` computes a deterministic SHA-256 digest over the finality policy schema, human-readable policy name and finality delay in economic seconds.

The reward-settlement boundary receives an identity of the form:

`<policy-name>@sha256:<policy-digest>`

Therefore the same policy name cannot silently be reused with a different finality duration. Any production change requires a new frozen policy identity/digest and the normal consensus-upgrade process.

## 5. Monotonic finality

A node with a previously persisted finality boundary must validate that boundary against the current authoritative ordered DAG **before** advancing it.

If valid:

- the finality score may stay unchanged; or
- it may advance to a later eligible selected-chain anchor;
- it must never move backward.

A restarted node must reconstruct the same boundary from the same frozen policy, cadence table and canonical DAG state.

## 6. Finality conflicts fail closed

A previously finalized boundary is bound to the exact ordered prefix. If a newly observed DAG would change the boundary block or any position inside the protected prefix, validation returns a **finality conflict**.

A finality conflict MUST NOT be silently resolved by:

- choosing the newest peer view;
- choosing local arrival order;
- deleting the old boundary;
- changing the finality duration locally;
- rewriting reward settlement state;
- pruning the conflicting context.

Until the production conflict policy is frozen, the safe behavior is:

- stop finality advancement;
- stop materializing new deferred reward UTXOs;
- do not prune affected history;
- preserve diagnostic/proof material;
- keep `launch_ready=false` if the exact-candidate conflict procedure is not implemented and tested.

The exact production conflict-resolution mechanism, including whether any operator/API action exists, remains `TBD` and must be explicit before GO.

## 7. Reward-settlement integration

Production reward settlement may consume only a finality boundary derived by the frozen v3 finality engine and bound to the exact policy identity.

For a reward at monetary score `M`:

`spendable(M) = finality_protected(M) && economic_maturity_reached(M, current_score)`

The approved coinbase maturity remains 3,600 economic seconds. The finality duration is a separate security parameter and may be greater than, equal to or less than maturity only if explicitly approved by the final network freeze.

No reward claim amount supplied by a miner is finality authority.

## 8. Pruning remains a separate deeper boundary

The v3 finality engine does not prune blocks or state.

Before production pruning is enabled, the project must freeze a separate pruning/checkpoint policy proving that enough history remains for:

- GHOSTDAG/order verification;
- finality-conflict diagnosis;
- reward-settlement replay;
- UTXO/contract-state reconstruction;
- snapshot/bootstrap proof validation;
- recovery from partial/corrupt storage.

The pruning duration/depth MUST NOT be silently inferred from the finality duration. It remains `TBD` until replay, bootstrap and adversarial evidence approve it.

## 9. Implementation authority

Current non-live implementation:

`crates/pulsedag-core/src/finality_v3.rs`

It provides:

- `FinalityPolicyV3`;
- deterministic finality-policy digest and identity;
- economic-time finality-delay evaluation;
- selected-chain-anchor selection;
- exact ordered-prefix binding through `RewardFinalityBoundaryV3`;
- previous-boundary revalidation;
- monotonic advancement;
- fail-closed finality-conflict detection;
- 1/2 BPS economic-time equivalence tests and conflict vectors.

The module intentionally contains **no mainnet finality-duration constant**.

## 10. Mandatory freeze evidence

Before `FROZEN`, the exact candidate must prove at minimum:

- identical finality decision for equivalent canonical states received in different valid arrival orders;
- economic-time equivalence at all supported/frozen 1 BPS, 2 BPS and 4 BPS cadences;
- cadence-transition continuity across an activation boundary;
- no finality before the configured economic delay;
- monotonic finality advancement across restart/replay;
- stale prior boundary fails closed after a protected-prefix reorder;
- policy digest changes if any finality parameter changes;
- mainnet/testnet policy identities cannot be silently confused;
- deferred rewards do not materialize without a valid finality boundary;
- no pruning occurs merely because a finality boundary advanced;
- network-partition/rejoin tests exercise finality conflicts and the approved recovery procedure;
- >=1,000,000-block replay reproduces byte-identical finality and reward-settlement state.

## 11. Production values still TBD

The network freeze must still record separately for mainnet and parallel testnet:

- finality policy name/version;
- finality policy SHA-256 digest;
- finality delay in economic seconds;
- exact cadence table/digest used for economic-time conversion;
- conflict-detection implementation digest;
- conflict-resolution policy/version/digest;
- persistence/snapshot schema for the last finalized boundary;
- pruning/checkpoint policy/version/digest;
- pruning duration/depth and retained-context requirements.

Until these values, live state integration and evidence are frozen, overall launch state remains `PRE_FREEZE` and `launch_ready=false`.
