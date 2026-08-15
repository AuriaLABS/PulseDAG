# PulseDAG Transaction and Signing Protocol v2

Status: **Task 23 implementation contract**

Issue: `#863`

Depends on: `docs/PROTOCOL_ACTIVATION_V2_4_0.md`

## Purpose

This document freezes the transaction-level behavior required by PulseDAG v2.4.0 without mutating historical transaction/signing v1 semantics.

It is consumed by core validation, mempool, RPC, P2P, storage/replay, mining templates and wallet code. Downstream components must not invent independent version, replay, replacement or retry identity rules.

## 1. Protocol versions

PulseDAG transaction versions are explicit:

- version `1`: historical/legacy transaction and signing protocol;
- version `2`: v2.4.0 chain-bound transaction and signing protocol.

Transaction v1 remains reproducible under its original protocol context. Transaction v2 is never interpreted through v1 canonical bytes as a fallback.

## 2. Canonical signing and transaction domains

### v1

Historical v1 domains are frozen:

- unsigned/signing: `PulseDAG:unsigned-tx:v1`;
- canonical transaction/txid: `PulseDAG:tx:v1`.

No chain binding is inserted into v1 bytes.

### v2

Version 2 uses:

- unsigned/signing: `PulseDAG:unsigned-tx:v2`;
- canonical transaction/txid: `PulseDAG:tx:v2`.

The exact UTF-8 `chain_id` is encoded as a little-endian `u32` length followed by the original bytes immediately after the domain and before the transaction body.

No trimming, Unicode normalization, case folding or alias substitution is allowed. An empty `chain_id` fails closed.

`network_profile` is operator configuration and is not canonical signing identity.

## 3. Source of chain identity

For v2.4.0, canonical chain identity is supplied by the activated consensus/network context rather than duplicated as a mutable field inside `Transaction`.

Consequences:

- wallet/offline signing must know the target `chain_id` before signing;
- node verification must use the configured/persisted activated `chain_id`;
- RPC callers cannot override the node's consensus chain identity for admission;
- P2P transaction validation uses the receiving node's compatible peer/chain context;
- storage/replay selects the historical protocol identity from persisted activation metadata.

A transaction object is therefore not self-authorizing for a chain merely because it claims a version.

## 4. Cross-chain replay behavior

Transaction v2 is cryptographically chain-bound.

The same logical spend under different chain identities must produce different:

- signing messages;
- signatures;
- canonical transaction bytes;
- txids;
- submission identities.

A v2 signature or txid created for chain A is invalid on chain B.

Historical transaction v1 replay remains governed by the original legacy chain/protocol context. v1 is not made safe for new cross-chain use by retroactively changing its bytes.

## 5. Admission and coexistence

The release rule is profile-gated, not ambiguous mixed acceptance.

### Legacy protocol

A legacy release context admits transaction version `1` only.

- version `1`: validate with frozen v1 canonical/signature rules;
- version `2`: inactive and rejected for ordinary mempool/RPC admission;
- unknown versions: unsupported and rejected;
- no attempt may reinterpret a v2 transaction using v1 bytes.

### Activated `ghostdag_v1` protocol

The final v2.4.0 activated clean-chain context admits transaction version `2` only for ordinary new submissions.

- version `2`: validate with chain-bound v2 canonical/signature rules;
- ordinary version `1` mempool/RPC submission: inactive and rejected;
- unknown versions: unsupported and rejected;
- historical v1 data remains decodable/replayable only in its recorded historical context.

### Development mixed-version testing

A mixed-version compatibility harness may exist only as an explicit development/test profile. It is never public-testnet-ready and must not become the default through fallback behavior.

## 6. Duplicate, conflict and replacement semantics

PulseDAG v2.4.0 does **not** enable implicit replace-by-fee (RBF).

The transaction admission states are frozen as follows:

### Duplicate

A transaction with the exact canonical txid already known in mempool/accepted state is a duplicate.

Duplicate submission is idempotent. It must not create a second mempool entry, second wallet spend, second broadcast identity or replacement event.

### Conflict

A distinct transaction that attempts to spend an outpoint already reserved by another live mempool transaction is a conflict.

For v2.4.0:

- conflict is rejected;
- a higher fee does not replace the incumbent transaction;
- fee rate does not grant replacement priority;
- transaction nonce does not grant replacement priority;
- arrival timing must not silently transform conflict into replacement semantics.

Conflict must be observable distinctly from exact duplicate once the admission/RPC status surface is wired.

### Replacement

Automatic RBF/replacement is disabled for v2.4.0.

If replacement is introduced later, it requires a separate protocol contract defining eligibility, fee bump, descendant handling, wallet reconciliation, propagation and deterministic conflict selection. It cannot be inferred from the current mempool implementation.

## 7. Stable submission identity

`txid` identifies the final canonical signed transaction. It is not the only identity needed by wallet/RPC retry workflows.

Task 23 defines a separate deterministic v2 submission identity with domain:

`PulseDAG:tx-submission:v1`

Canonical bytes are:

```text
len("PulseDAG:tx-submission:v1")
"PulseDAG:tx-submission:v1"
len(chain_id)
chain_id
len(canonical_v2_transaction_bytes)
canonical_v2_transaction_bytes
```

All lengths are little-endian `u32` values.

`submission_id = SHA256(canonical_submission_bytes)`.

Properties:

- identical retry of the same signed v2 transaction on the same chain has the same `submission_id`;
- changing canonical transaction bytes changes `submission_id`;
- changing `chain_id` changes `submission_id`;
- `submission_id` is domain-separated from `txid`;
- `submission_id` does not authorize replacement;
- `submission_id` is distinct from any wallet-plan identity used before final signing.

Golden vectors for the core sample transaction are frozen in unit tests.

## 8. Wallet identity model

Wallet implementations must keep three concepts distinct:

1. **wallet-plan identity**: local intent before final canonical signing; may survive rebuild/reselection according to wallet policy;
2. **submission identity**: deterministic identity of one final signed v2 transaction submission/retry;
3. **txid**: canonical consensus transaction identifier.

A wallet must not assume two plans are the same transaction merely because they target the same outputs, amount or nonce.

Required reconciliation states for later Task 23 wallet wiring are:

- accepted;
- duplicate/idempotent retry;
- conflict;
- rejected;
- confirmed.

`replaced` is reserved but is not produced by the v2.4.0 no-RBF policy.

## 9. Stable rejection classes

Admission/RPC/P2P surfaces must converge on stable semantic classes rather than parsing arbitrary text.

Required classes are:

- `unsupported_transaction_version`;
- `inactive_transaction_version`;
- `wrong_chain_domain`;
- `invalid_txid`;
- `invalid_signature`;
- `duplicate`;
- `conflict`;
- `orphan`;
- `malformed_transaction`;
- `insufficient_funds`;
- `mempool_full`.

The exact transport-level error envelope may differ between RPC and P2P, but the semantic classification must not.

## 10. Storage and indexing

Storage/indexing must preserve the transaction version required to select historical canonical rules.

Activated storage/snapshot identity additionally records the chain/protocol activation identity defined by Task 22/24. A v2 transaction must never be reconstructed under a different chain identity from operator defaults alone.

Indexes remain keyed by canonical txid unless an index explicitly stores submission identity for retry/observability purposes. `submission_id` never replaces txid as consensus identity.

## 11. P2P behavior

P2P propagation must not make unsupported transaction versions appear valid through decode fallback.

Before activated `ghostdag_v1` propagation is enabled:

- legacy peers continue to use v1 transaction semantics;
- v2 propagation requires compatible chain/protocol capability negotiation from Task 27;
- unsupported/inactive versions are rejected explicitly;
- conflict is not punished as malformed cryptography merely because the transaction is otherwise well formed.

## 12. Mining/template behavior

Legacy templates retain v1 transaction rules.

Activated v2.4.0 templates include only transactions admitted under the activated transaction v2 context. The miner cannot override `chain_id` or transaction protocol version independently from the node activation identity.

No v1/v2 mixed template is release-valid under the clean-chain activation contract.

## 13. Rollback and downgrade

Before public launch, a candidate may be abandoned, but returning to legacy means returning to a compatible legacy data directory/snapshot or starting a clean legacy chain.

After activated launch, v2 state is not downgraded in place by ignoring transaction/protocol identity.

Any change to canonical v2 signing bytes, txid bytes, submission identity bytes, version admission or conflict/replacement semantics invalidates affected Task 30/release evidence.

## 14. Implementation sequence

Task 23 implementation is intentionally staged:

1. canonical chain-bound v2 signing/txid primitives and golden vectors;
2. stable submission identity and this admission/replacement contract;
3. explicit legacy/v2 validation gates and semantic rejection classes;
4. mempool/RPC submission and conflict wiring;
5. wallet build/offline-sign/reconciliation wiring;
6. P2P/storage/replay/miner integration with Tasks 24/27/28;
7. complete exact-SHA validation matrix.

No intermediate slice authorizes public-testnet launch or protocol activation.
