# Mempool replacement v3 assessment foundation

Issue: #1036. Launch authority remains #781 / #794.

## Status

This document freezes a **read-only, non-activating** replacement assessment contract. It does not authorize RBF, does not change the live mempool mutation path, and does not change the `MempoolPolicyV3` identity or compatibility defaults.

The current compatibility vector remains `0 / u64::MAX / 4096 / false`, and every live mempool conflict continues to fail closed with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`, including when a caller constructs a policy object with `replacement_enabled=true`.

## Transaction-protocol constraint

The frozen transaction object contains `txid`, `version`, `inputs`, `outputs`, `fee`, and `nonce`. There is no transaction-level RBF opt-in/sequence field.

The frozen v2 transaction protocol additionally requires that replacement must not be inferred from fee, fee rate, nonce, arrival time, retry/submission identity, or a distinct conflicting txid. Therefore this assessment exposes deterministic replacement facts only. It does not decide whether a transaction opted in to replacement and it cannot authorize mutation.

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

These values are observations, not an eligibility or authorization bit.

## Determinism and safety rules

- Assessment is mutation-free and takes `&ChainState`.
- Equivalent mempool states with different internal `HashMap` insertion order must produce byte-for-byte equivalent assessment values.
- Conflict and replacement-package selection reuse the already-frozen canonical classifier; no independent conflict graph is introduced.
- Package and incoming canonical sizes reuse the already-frozen v1/v2/v3 signed transaction encodings.
- Fee arithmetic uses `u128` accumulation and integer division; no floating-point comparison is used.
- New unconfirmed dependencies and dependencies on would-be-evicted package members are surfaced explicitly instead of silently accepted.
- An exact duplicate remains governed by the historical `Duplicate` precedence because the canonical conflict classifier excludes the same txid.

## What this foundation does not freeze

This slice intentionally does **not** freeze or activate:

- any RBF opt-in mechanism;
- a positive incremental relay-fee/bump constant;
- a maximum replacement-package count/weight beyond existing mempool resource policy;
- whether new unconfirmed parents are ultimately forbidden or bounded;
- wallet replacement/reconciliation behavior;
- P2P replacement propagation or anti-DoS rules;
- actual conflict-package eviction/mutation;
- restart persistence for a completed replacement event;
- replacement-enabled policy identity/defaults;
- final mainnet fee values.

A later activation slice must explicitly resolve those items and rerun exact-candidate launch evidence. This foundation alone does not complete the `RBF/replacement semantics` checkbox in #1036 and does not imply #781/#794 GO.
