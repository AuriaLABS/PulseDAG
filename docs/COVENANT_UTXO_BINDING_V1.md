# Covenant UTXO binding v1 (#1041)

This slice binds the already-frozen Covenant v1 descriptor and evaluator to an exact historical PulseDAG UTXO under an explicit programmability activation identity without changing the legacy `Utxo`, transaction, signing, txid, wallet, storage, snapshot, mempool, or block-validation shapes.

The implementation lives in `crates/pulsedag-core/src/covenant_utxo_v1.rs` and remains **inactive**. Nothing in this slice wires covenant evaluation into live transaction admission or block validation.

## Attachment boundary

`CovenantUtxoAttachmentV1` version 1 contains only:

- the attachment version,
- a 32-byte commitment to the exact legacy UTXO under an explicit activation identity,
- the existing `CovenantDescriptorV1`.

The UTXO commitment domain is `PulseDAG:covenant-utxo:v1`. Its canonical preimage commits, in order, to:

1. the domain tag as u32-length-prefixed bytes,
2. the 32-byte fingerprint of the expected `ProgrammabilityActivationIdentity`,
3. outpoint txid as u32-length-prefixed bytes,
4. outpoint index as little-endian `u32`,
5. address as u32-length-prefixed bytes,
6. amount as little-endian `u64`,
7. coinbase flag as one byte,
8. creation height as little-endian `u64`.

The activation identity fingerprint already commits to the programmability identity version, mainnet/testnet/application domain, bounded chain ID, genesis hash, and base-protocol fingerprint. This makes an otherwise identical legacy UTXO tuple distinct across activation identities and prevents a covenant attachment from being replayed onto a matching historical UTXO on another chain.

The attachment commitment domain is `PulseDAG:covenant-utxo-attachment:v1`. Its canonical preimage commits to the chain-bound UTXO commitment plus descriptor version, covenant kind, parameter commitment, and witness budget.

Unknown attachment versions, invalid activation identities, zero UTXO commitments, cross-chain identity substitution, cross-UTXO substitution, descriptor/parameter substitution, malformed descriptor versions, and unknown self-describing Serde fields fail closed.

## Spend validation boundary

`validate_covenanted_utxo_spend_v1` performs the following deterministic checks:

1. validates the attachment and its existing Covenant v1 descriptor,
2. validates and fingerprints the supplied expected programmability activation identity,
3. recomputes the chain-bound commitment of the supplied historical UTXO and requires an exact match,
4. invokes the already-merged `evaluate_covenant_v1` with explicit typed parameters, witness facts, and execution context,
5. records the attachment commitment, UTXO commitment, parameter commitment, and canonical witness byte count as validation evidence.

This layer does not authenticate signatures itself. Authorization facts continue to be supplied to Covenant v1 only after the future live integration layer has derived them from the applicable transaction-signature path.

## Legacy compatibility

The legacy `Utxo` struct remains byte-for-byte unchanged:

- outpoint,
- address,
- amount,
- coinbase flag,
- creation height.

The attachment is an independent commitment object. The activation identity is not serialized into the attachment; its fingerprint is transitively committed by the 32-byte UTXO commitment. The attachment is not serialized into legacy UTXOs, transactions, signing messages, or txids, and this slice adds no persistence migration.

## Frozen vectors

For the sample activation identity and UTXO used by `covenant_utxo_v1::tests::sample_utxo_and_attachment_commitments_are_frozen`:

- activation domain: `testnet`,
- chain id: `pulsedag-testnet-v3`,
- genesis hash: `0x11` repeated 32 bytes,
- base-protocol fingerprint: `0x22` repeated 32 bytes,
- activation identity fingerprint: `59cb43d2b3cf6bde15c04651572415fa8503d83445820c8ebde3b877f84859df`,
- txid: `11` repeated 32 bytes as lowercase hex text,
- output index: `7`,
- address: `pulse1covenanttest`,
- amount: `42000`,
- coinbase: `false`,
- height: `123`.

Frozen commitments:

- chain-bound UTXO commitment: `2c2c2afbe2b8a3cdc2902781525138bc7edee7a7a10abbfdf5d90be6e40a4fe2`
- current Covenant v1 timelock parameter commitment: `16ddbcef24217eb9bc63084ae900ad45bc7a34f9af6acba58cf141d46ef4230c`
- canonical attachment length: `114` bytes
- attachment commitment: `6e7c5f9c1c6c189646d113b1ebdcde6661b12febd41a44cf4244e0c502fbc07d`

The former draft #1103 attachment vector is intentionally not reused because that draft used a superseded Covenant program model. This slice binds the exact descriptor/parameter surface already merged by #1104.

## Explicitly deferred

This slice still does not activate programmability and does not complete #1041. Deferred work includes:

- deriving live authorization facts and spend commitments from transaction validation,
- wiring covenant attachments into live UTXO admission/spend validation,
- Contract State Transition evidence and deterministic resource accounting,
- proof-evidence retention/invalidation enforcement,
- persistence, snapshot, pruning, and rollback migration,
- application commitment budgets and activation decisions,
- PulseScript/compiler/VM work owned by #1042,
- any #781/#794 GO decision.
