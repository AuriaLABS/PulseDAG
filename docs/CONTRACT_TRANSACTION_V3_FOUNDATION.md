# Contract Transaction v3 foundation (#1041)

This document freezes the first **inactive** consensus-facing programmability boundary for PulseDAG. It does not activate programmability on any existing mainnet or testnet profile and it does not add a VM, compiler, PulseScript, ZK verifier, native-asset semantics, live contract RPC, fee-policy change, wallet/address/PQC change, or persistence migration.

The implementation lives in `crates/pulsedag-core/src/contract_v3.rs`. It is deliberately separate from `tx_v3.rs`: the existing `tx_v3.rs` is the reserved PQC transaction authorization path, while this foundation defines a bounded **contract envelope** and its own domains.

## Frozen identity and domains

`ProgrammabilityActivationIdentity` version 1 is bound to:

- a programmability domain (`mainnet`, `testnet`, or a non-zero numbered application domain),
- a bounded canonical `chain_id`,
- a 32-byte genesis hash,
- a 32-byte base-protocol fingerprint.

The canonical identity domain tag is `PulseDAG:programmability-identity:v1`. Exact identity comparison is required before an envelope may be interpreted in a chain/application context. Unknown identity versions, invalid application domains, malformed chain identifiers, and cross-chain substitution fail closed.

No constructor or runtime path in this slice selects or activates this identity for existing network profiles.

## Contract Transaction v3 envelope

`ContractTransactionV3Envelope` version 3 contains only bounded or fixed-width consensus metadata:

- bounded namespace: at most 64 bytes and restricted to canonical lowercase ASCII identifier characters,
- fixed 32-byte payload commitment,
- fixed 32-byte state commitment,
- fixed 32-byte authorization commitment,
- versioned proof-system metadata and a fixed 32-byte proof commitment,
- versioned deterministic integer-only resource budgets,
- `nonce` and `replay_nonce`,
- optional bounded Covenant v1 descriptor.

Payloads, executable code, witnesses, proof blobs, events, and mutable state blobs are **not** embedded in the consensus envelope.

The frozen canonical domain tags are:

- envelope: `PulseDAG:contract-envelope:v3`
- signing: `PulseDAG:contract-signing:v3`
- txid: `PulseDAG:contract-txid:v3`

All integer fields use fixed-width little-endian encoding. Variable strings and nested canonical objects are u32-length-prefixed. The signing digest and txid use distinct domain-separated preimages. Existing transaction v1/v2 serialization, signing and txid domains are untouched.

## Resource budget v1

Resource metadata version 1 is integer-only and bounded by these frozen maxima:

| Resource | Maximum |
| --- | ---: |
| Compute units | 10,000,000 |
| Read bytes | 16 MiB |
| Write bytes | 4 MiB |
| Proof bytes | 8 MiB |
| Event bytes | 1 MiB |

These values freeze an admission/commitment boundary only. The executable metering algorithm, state transition rules, refund semantics, and fee conversion are intentionally deferred to later #1041/#1042 work.

## Proof metadata and evidence invalidation

Proof metadata version 1 reserves two proof-system identifiers:

- `0`: no proof, requiring system version 0 and an all-zero proof commitment;
- `1`: external commitment v1, requiring system version 1 and a non-zero 32-byte proof commitment.

Any unknown proof-system identifier, proof metadata version, or incompatible proof-system version fails closed. This foundation does not verify a proof.

Persisted or external evidence must be treated as version-bound. A change in programmability identity fingerprint, proof metadata version, proof-system identifier, proof-system version, or proof commitment invalidates prior evidence for the new interpretation. Evidence must never be silently reinterpreted across those boundaries.

## Covenant v1 reservation

Covenant descriptor version 1 reserves bounded identifiers for:

1. timelock,
2. vault,
3. multisig,
4. escrow,
5. atomic swap,
6. payment channel.

A descriptor commits to future parameters with a 32-byte commitment and caps witness metadata at 64 KiB. Descriptor structure can be canonically committed now, but every executable covenant kind remains unsupported in this foundation and must reject deterministically if execution is attempted. Primitive semantics are follow-up work.

## Storage, snapshot, pruning and rollback boundary

`ContractV3CompatibilityMetadataV1` freezes the compatibility tuple that future persistence surfaces may carry:

- compatibility schema version,
- exact programmability identity fingerprint,
- Contract Transaction version,
- proof metadata version,
- resource-budget version,
- covenant-descriptor version.

This PR does **not** modify the current database, snapshot schema, pruning implementation, or rollback implementation. When those layers begin persisting contract state, the following compatibility rules apply:

1. restore/replay must require an exact compatible tuple before interpreting contract commitments;
2. unknown or mismatched versions fail closed rather than being coerced to current semantics;
3. snapshot or rollback data from another chain/application identity must not be accepted by substitution;
4. pruning may not alter retained commitment values or permit evidence to be reused under a different proof/version identity;
5. a future migration must be explicit and versioned; this foundation authorizes no in-place reinterpretation of existing storage.

Final executable state-transition semantics, proof-retention policy, and persistence migration remain pending.

## Golden vectors

For the frozen test fixture in `contract_v3::tests::contract_v3_golden_vectors_are_frozen`:

- identity fingerprint: `59cb43d2b3cf6bde15c04651572415fa8503d83445820c8ebde3b877f84859df`
- canonical envelope SHA-256: `1a17f90e82ad12b7ef7f37c41589a5a9cbb08ecee1e24c119d7d390b0bd9f611`
- signing digest: `1579121abe104cefc3a2bdc37191ca24697a3e22dc980026925e333dc9f2d9c0`
- contract txid: `cded937ad08c7ba7cdb6beb35ac922376a9810bb57a3a03ac6d8583ddbeffdb2`

Tests also cover replay-stable encoding, mainnet/testnet/application separation, cross-chain substitution, unknown versions/domains, malformed proof metadata, oversized namespaces/resource budgets, strict unknown-field rejection, and unsupported covenant execution.

## Explicitly deferred

This foundation does not claim completion of #1041. Still pending are activation gates, executable state/resource accounting, PulseScript/compiler/VM work owned by #1042, covenant primitive semantics, proof verification/final evidence retention, application/asset semantics, fee integration, persistence/snapshot migrations, and launch-control decisions under #781/#794.
