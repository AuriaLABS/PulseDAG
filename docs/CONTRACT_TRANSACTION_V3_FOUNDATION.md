# Contract Transaction v3 foundation (#1041)

This document freezes the first **inactive** consensus-facing programmability boundary for PulseDAG. It does not activate programmability on any existing mainnet or testnet profile and it does not add a VM, compiler, PulseScript, ZK verifier, native-asset semantics, live contract RPC, fee-policy change, wallet/address/PQC change, or persistence migration.

The Contract Transaction foundation lives in `crates/pulsedag-core/src/contract_v3.rs`. Bounded Covenant v1 policy evaluation lives in `crates/pulsedag-core/src/covenant_v1.rs`. Both are deliberately separate from `tx_v3.rs`: the existing `tx_v3.rs` is the reserved PQC transaction authorization path, while this foundation defines a bounded **contract envelope**, covenant policy surface, and their own domains.

## Frozen identity and domains

`ProgrammabilityActivationIdentity` version 1 is bound to:

- a programmability domain (`mainnet`, `testnet`, or a non-zero numbered application domain),
- a bounded canonical `chain_id`,
- a 32-byte genesis hash,
- a 32-byte base-protocol fingerprint.

The canonical identity domain tag is `PulseDAG:programmability-identity:v1`. Exact identity comparison is required before an envelope may be interpreted in a chain/application context. Unknown identity versions, invalid application domains, malformed chain identifiers, and cross-chain substitution fail closed.

No constructor or runtime path in these slices selects or activates this identity for existing network profiles.

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

## Covenant v1 descriptor and execution boundary

Covenant descriptor version 1 reserves bounded identifiers for:

1. timelock,
2. vault,
3. multisig,
4. escrow,
5. atomic swap,
6. payment channel.

A descriptor commits to typed parameters with a 32-byte commitment and caps witness metadata at 64 KiB. A descriptor **by itself** is not executable: execution requires the explicit versioned parameters, witness facts, and execution context passed to `evaluate_covenant_v1`. This keeps the original Contract v3 envelope unchanged and preserves its frozen golden vectors.

Covenant execution semantics version 1 uses two additional domains:

- parameters: `PulseDAG:covenant-parameters:v1`
- witness: `PulseDAG:covenant-witness:v1`

The parameter commitment is SHA-256 over the canonical parameter encoding. Before policy evaluation, the evaluator requires all of the following to match exactly:

- descriptor version and covenant kind,
- recomputed parameter commitment,
- witness covenant kind,
- execution-semantics version,
- descriptor witness-byte budget,
- path-specific spend commitment.

Unknown or mismatched versions/kinds/commitments fail closed.

### Bounded data model

No unbounded participant, authorization, signature, script, or preimage blob is introduced.

- authorization/participant sets have fixed storage for at most 16 32-byte commitments,
- active set entries must be non-zero, strictly sorted, and unique,
- unused fixed slots must be zero,
- atomic-swap preimages have fixed storage capped at 64 bytes,
- unused preimage bytes must be zero,
- the descriptor-level witness budget remains capped at 64 KiB.

The evaluator consumes **authorization facts** represented by 32-byte key commitments. It deliberately does not verify Ed25519/PQC signatures or change wallet/address/PQC behavior. A future transaction integration layer must derive those authorization facts from the applicable signature-validation path before invoking the covenant policy evaluator.

### Primitive semantics v1

All time-dependent rules use block height only; no host clock or randomness is consulted.

**Timelock** requires the committed spend target, the committed authorization key, and `block_height >= not_before_height`.

**Vault** has two explicit paths. Normal spend requires the normal spend target and spend-key authorization. Recovery requires the recovery spend target, recovery-key authorization, and `block_height >= recovery_height`.

**Multisig** commits to a canonical participant set, a threshold, and a spend target. Every supplied authorization must be a participant and the number of distinct authorization facts must meet the threshold.

**Escrow** commits buyer, seller, arbiter, release target, and refund target. Release accepts buyer+seller cooperation or seller+arbiter arbitration. Refund accepts buyer+seller cooperation or buyer+arbiter arbitration.

**Atomic swap** commits claim/refund targets, SHA-256 secret hash, claim/refund authorization keys, and refund height. Claim requires a matching non-empty preimage before the refund height. Refund requires the refund key at or after the refund height and must not carry a preimage.

**Payment channel** commits both parties, a state commitment, exact sequence number, cooperative/unilateral spend targets, and settlement height. Cooperative settlement requires both parties and may occur immediately. Unilateral settlement requires exactly one channel party and is accepted only at or after the settlement height. This slice freezes a bounded close/settlement policy; it does not add an off-chain update protocol, dispute network, or VM.

The evaluator is **not wired into live transaction admission or block validation yet**. Consequently these semantics remain inactive on all existing networks until a separate activation/integration change is reviewed.

## Storage, snapshot, pruning and rollback boundary

`ContractV3CompatibilityMetadataV1` freezes the compatibility tuple that future persistence surfaces may carry:

- compatibility schema version,
- exact programmability identity fingerprint,
- Contract Transaction version,
- proof metadata version,
- resource-budget version,
- covenant-descriptor version.

These slices do **not** modify the current database, snapshot schema, pruning implementation, or rollback implementation. When those layers begin persisting contract state, the following compatibility rules apply:

1. restore/replay must require an exact compatible tuple before interpreting contract commitments;
2. unknown or mismatched versions fail closed rather than being coerced to current semantics;
3. snapshot or rollback data from another chain/application identity must not be accepted by substitution;
4. pruning may not alter retained commitment values or permit evidence to be reused under a different proof/version identity;
5. a future migration must be explicit and versioned; this foundation authorizes no in-place reinterpretation of existing storage.

Final executable state-transition semantics beyond Covenant v1 policy evaluation, proof-retention policy, and persistence migration remain pending.

## Golden vectors

For the frozen test fixture in `contract_v3::tests::contract_v3_golden_vectors_are_frozen`:

- identity fingerprint: `59cb43d2b3cf6bde15c04651572415fa8503d83445820c8ebde3b877f84859df`
- canonical envelope SHA-256: `1a17f90e82ad12b7ef7f37c41589a5a9cbb08ecee1e24c119d7d390b0bd9f611`
- signing digest: `1579121abe104cefc3a2bdc37191ca24697a3e22dc980026925e333dc9f2d9c0`
- contract txid: `cded937ad08c7ba7cdb6beb35ac922376a9810bb57a3a03ac6d8583ddbeffdb2`

The Covenant v1 timelock parameter vector in `covenant_v1::tests::timelock_parameters_have_frozen_commitment_vector` is:

- canonical parameter commitment: `16ddbcef24217eb9bc63084ae900ad45bc7a34f9af6acba58cf141d46ef4230c`

Tests cover all six primitive allow/reject paths, canonical bounded sets/preimages, witness budgets, parameter binding, height boundaries, state/sequence binding, replay-stable parameter commitments, and the original Contract v3 vectors.

## Explicitly deferred

This work still does not claim completion of #1041. Remaining work includes activation gates, wiring covenant authorization facts and spend commitments into live transaction/block validation, executable state/resource accounting beyond the bounded Covenant v1 evaluator, PulseScript/compiler/VM work owned by #1042, proof verification/final evidence retention, application/asset semantics, fee integration, persistence/snapshot migrations, and launch-control decisions under #781/#794.
