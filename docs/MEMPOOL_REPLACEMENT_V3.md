# Mempool replacement v3 no-RBF contract and assessment

Issue: #1036. Launch authority remains #781 / #794.

## Status

This document records two complementary pieces of the current transaction/mempool contract:

1. automatic RBF/replacement is **disabled** for the frozen transaction protocol; and
2. `MempoolReplacementAssessmentV3` is a deterministic, read-only observation surface for conflict-package facts.

The assessment does not authorize mutation. For transactions that pass v3 policy preflight, the live mempool path rejects every true conflict with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`, including when a caller constructs a policy object with `replacement_enabled=true`.

The compatibility vector remains `0 / u64::MAX / 4096 / false`.

## Transaction-protocol constraint

The frozen transaction object contains `txid`, `version`, `inputs`, `outputs`, `fee`, and `nonce`. There is no transaction-level RBF opt-in/sequence field.

The frozen v2 transaction protocol explicitly disables automatic RBF. Replacement must not be inferred from fee, fee rate, nonce, arrival time, retry/submission identity, or a distinct conflicting txid. A distinct transaction that spends an outpoint reserved by a live mempool transaction is therefore a conflict; after it passes v3 policy preflight, admission rejects that conflict.

That rule is authoritative over mempool policy. `replacement_enabled=true` is not an activation switch for the current transaction protocol. An explicit stricter v3 policy may reject during preflight before either legacy `Duplicate` handling or conflict classification; this document does not override that production ordering.

## Frozen no-RBF admission rule

For transactions that pass v3 policy preflight under the current protocol:

- an exact canonical txid retry keeps historical `Duplicate` handling after preflight;
- a distinct conflicting txid is rejected with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`;
- higher absolute fee does not replace the incumbent;
- higher canonical fee rate does not replace the incumbent;
- nonce does not grant replacement priority;
- arrival/first-seen order does not grant replacement priority;
- submission/retry identity does not grant replacement priority;
- ordinary and protocol-aware v3 admission enforce the same post-preflight conflict rule before mempool mutation;
- conflict rejection preserves the incumbent transaction/package and mempool metadata apart from intentional rejection accounting.

The valid signed regression `mempool_no_rbf_protocol_v3.rs` uses compatibility-default v3 policy so the tested transactions pass preflight. It constructs an incoming conflict whose fee and canonical fee rate are both strictly greater than the incumbent package and whose nonce is larger. The read-only assessment observes those facts, but both `replacement_enabled=false` and `replacement_enabled=true` still reject the post-preflight conflict. The protocol-aware path produces the same result.

## Assessment identity

The read-only assessment has `version = 1` under `MEMPOOL_REPLACEMENT_ASSESSMENT_V3_VERSION`.

For one incoming transaction and one immutable chain state it reports:

- `direct_conflict_txids`: the canonical direct conflict vector from the frozen v3 conflict classifier;
- `replacement_package_txids`: the direct conflicts plus all live in-mempool descendants, unique and lexicographically sorted;
- `replacement_package_total_fee`: sum of all fees in the complete replacement package using `u128` accumulation;
- `replacement_package_total_size_bytes`: sum of canonical signed transaction sizes for every package member using the already-frozen version-specific canonical encodings;
- `replacement_package_fee_rate_per_kb`: integer-only aggregate `floor(total_fee * 1000 / total_size_bytes)`; zero for an empty package;
- incoming transaction fee, canonical size, and canonical fee rate;
- `positive_fee_delta_over_package`: the positive incoming absolute-fee delta over the whole replacement package, or `None` when no strict positive delta exists;
- `pays_strictly_higher_total_fee`: observation that incoming absolute fee is strictly greater than the complete package fee;
- `pays_strictly_higher_fee_rate`: observation that incoming canonical fee rate is strictly greater than the aggregate package fee rate;
- `new_unconfirmed_parent_txids`: live mempool parents consumed by the incoming transaction that are outside the replacement package, unique and lexicographically sorted;
- `depends_on_replacement_package`: whether the incoming transaction spends an output created by a transaction that the same replacement package would remove.

These values are observations, not eligibility or authorization bits.

## Determinism and safety rules

- Assessment is mutation-free and takes `&ChainState`.
- Equivalent mempool states with different internal `HashMap` insertion order produce equivalent assessment values.
- Conflict and replacement-package selection reuse the already-frozen canonical classifier; no independent conflict graph is introduced.
- Package and incoming canonical sizes reuse the already-frozen v1/v2/v3 signed transaction encodings.
- Fee arithmetic uses `u128` accumulation and integer division; no floating-point comparison is used.
- New unconfirmed dependencies and dependencies on would-be-evicted package members are surfaced explicitly instead of silently accepted.
- For retries that pass v3 policy preflight, an exact duplicate remains governed by historical `Duplicate` handling after preflight because the canonical conflict classifier excludes the same txid.

## Future protocol work, not current activation scope

A future protocol version may choose to introduce RBF, but doing so would require a separate protocol contract and new exact-candidate evidence covering at least:

- an explicit opt-in/eligibility signal;
- fee-bump and incremental-relay rules;
- replacement-package limits and descendant handling;
- conflict-package eviction/mutation;
- wallet replacement/reconciliation behavior;
- P2P replacement propagation and anti-DoS behavior;
- restart persistence and recovery for completed replacements;
- a replacement-enabled policy identity/default;
- affected transaction/signing, Task 30, and launch vectors.

None of those future-RBF mechanisms are inferred or activated by the current assessment. They are not prerequisites for enforcing the current protocol's explicit no-RBF semantics.

No #781/#794 launch GO is implied by this contract.
