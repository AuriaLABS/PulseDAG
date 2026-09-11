# Covenants v1 and Contract State Transition v1

Status: consensus-facing freeze for the second inactive #1041 slice. Nothing in this document activates programmability on an existing network profile.

## Scope

This slice builds on the Contract Transaction v3 foundation and freezes three additional deterministic boundaries owned by #1041:

1. bounded UTXO Covenant v1 program/witness semantics for timelock, vault, multisig, escrow, atomic swap, and payment channel primitives;
2. a covenant-to-UTXO attachment/spend contract that binds those semantics to the exact legacy UTXO contents without modifying the legacy `Utxo` shape; and
3. a Contract State Transition v1 evidence object binding protocol identity, Contract Transaction v3 txid, prior/next state commitments, effect commitments, replay nonce, actual resource usage, and proof evidence metadata.

PulseScript/compiler/VM execution remains #1042 scope. No live transaction path, wallet format, address format, fee policy, storage schema, snapshot schema, or RPC is activated by this slice.

## Authenticated signer boundary

Covenants consume fixed 32-byte signer identifiers from `CovenantSpendContextV1`. Those identifiers mean **already-authenticated signers**. Covenant evaluation does not introduce another signature format or bypass the existing transaction/PQC signature layers. A future activation adapter must derive these identifiers only after the active transaction signature rules have authenticated the corresponding keys.

Signer vectors are bounded to 16 entries, strictly sorted, unique, and non-zero. This prevents duplicate-count ambiguity and gives replay-stable threshold evaluation.

## Covenant v1 semantics

All programs are version 1, domain-separated by `PulseDAG:covenant-program:v1`, and committed by SHA-256. A `CovenantDescriptorV1.parameters_commitment` must equal the canonical program commitment and the descriptor kind must match the program kind. Witnesses are independently versioned/domain-separated and must fit the descriptor witness-byte budget.

- **Timelock** — requires one authenticated signer plus either an absolute block-height boundary or an explicit median-time-seconds boundary supplied by consensus context. Host wall clock is never read.
- **Vault** — recovery signer may spend immediately; hot signer may spend only at or after `activation_height + delay_blocks`.
- **Multisig** — bounded sorted signer set with an integer `m-of-n` threshold.
- **Escrow** — cooperative buyer+seller, buyer award buyer+arbiter, or seller award seller+arbiter.
- **Atomic swap** — redeem signer plus SHA-256 preimage before `refund_height`, or refund signer at/after `refund_height`.
- **Payment channel** — cooperative close by both roles; settlement signer may settle the exactly committed state/sequence during the settlement window; refund signer may refund at/after the refund height.

All unsupported versions, mismatched branches, malformed signer sets, unsatisfied locks, invalid hashlocks, state mismatches, and budget overruns fail closed with stable integer rejection codes.

## UTXO binding

`CovenantUtxoAttachmentV1` is deliberately separate from the historical `Utxo` structure. It carries only a version, a fixed 32-byte canonical commitment to the exact UTXO, and a `CovenantDescriptorV1`.

The UTXO commitment is domain-separated by `PulseDAG:covenant-utxo:v1` and binds the existing outpoint txid/index, address, amount, coinbase flag, and creation height. Spend validation recomputes that commitment before covenant evaluation. Cross-UTXO substitution therefore fails before a covenant branch can authorize the spend.

The attachment itself is independently domain-separated by `PulseDAG:covenant-utxo-attachment:v1` and commits the attachment version, UTXO commitment, covenant descriptor version/kind/program commitment, and witness budget. This keeps Covenant v1 replay-stable without adding fields to legacy transaction or UTXO serialization.

## Deterministic covenant resource accounting

Covenant evaluation uses integer-only metering. The v1 constants are frozen by code for base evaluation, signer checks, hash checks, lock checks, state checks, and canonical witness bytes. The resulting `CovenantResourceUsageV1` is replay-stable and can be merged into `ContractResourceUsageV1`; covenant witness bytes count as deterministic read bytes.

There is no floating point, host randomness, host time, or implementation-dependent gas estimation.

## Contract State Transition v1

`ContractStateTransitionV1` freezes the evidence shape for a deterministic state transition without defining a VM. Validation binds:

- exact programmability identity fingerprint;
- exact Contract Transaction v3 txid;
- caller-supplied expected prior-state commitment;
- `next_state_commitment == ContractTransactionV3Envelope.state_commitment`;
- non-zero write-set and event-set commitments (canonical empty commitments are provided);
- actual integer resource usage no greater than every envelope budget;
- transition replay nonce equal to both the envelope replay nonce and caller-supplied expected replay nonce; and
- proof-evidence metadata accepted by an explicit deterministic evidence policy.

The state-transition commitment is domain-separated by `PulseDAG:contract-state-transition:v1`.

## Proof evidence invalidation

This slice does **not** implement a ZK/proof verifier. It freezes the metadata/invalidation boundary around one:

- evidence names proof system/version, verifier revision, and evidence generation;
- policy names the exact accepted proof system/version/verifier revision and minimum valid evidence generation;
- evidence below the minimum generation is deterministically invalidated;
- external proof evidence commitment must equal the envelope proof commitment;
- the no-proof tuple is exactly system 0/version 0/revision 0/generation 0 with an all-zero commitment;
- unknown tuples fail closed.

Changing proof system, proof-system version, verifier revision, or minimum accepted generation is therefore an explicit consensus-policy change rather than an implicit verifier behavior change.

## Frozen vectors

- Multisig program commitment (`2-of-3`, signer ids `01*32`, `02*32`, `03*32`): `eece867dffa1c1f3b124c224a326457604ce0571640fa821f63d4d649f7163fb`.
- Sample UTXO commitment (txid `11*32`, index `7`, address `pulse1covenanttest`, amount `42000`, non-coinbase, height `123`): `d963255da4b2a57170093a53ae4fd332e679a5642dfae4dab0f3942b48dc3cd1`.
- Sample timelock attachment commitment (signer `01*32`, height `200`, witness budget `4096`): `e2ea41ec14721200fd59643c6cecd3505172548a2c572756215e475a0bb85a9b`.
- Empty write-set commitment: `e8181adc9a6964ee50c2afe73b5f6ca8c3945284b8016af8dac0e38383e11b1d`.
- Empty event-set commitment: `f5e7cd6592dcb69450651ce5e9e63b59ba789243057f371ca760b3180aac1a78`.
- Sample state-transition commitment bound to the #1102 Contract v3 golden envelope: `7ac186b70e972ae732e0c88a1365fae7529ad2e345dd3f0396e6c7474d8b3fcd`.

These supplement, not replace, the #1102 identity/envelope/signing/txid vectors.

## Activation and persistence boundary

The evaluator, UTXO attachment validator, and transition validator are pure inactive primitives. This slice deliberately does not:

- add covenant fields to the legacy `Transaction`, `TxInput`, `TxOutput`, or `Utxo` structs;
- modify v1/v2 transaction bytes, signing messages, txids, or PQC v3 behavior;
- make any mainnet/testnet profile accept Contract Transaction v3;
- change RocksDB/storage/snapshot/pruning layouts;
- apply writes to `ChainState` or execute application bytecode;
- define native assets or change fees; or
- expose live contract RPC methods.

A later activation adapter must be separately gated and must preserve the frozen canonical/evidence boundaries above.
