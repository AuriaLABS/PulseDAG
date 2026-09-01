# PulseDAG post-quantum cryptography migration

Status: **foundation / not consensus-activated**

This document defines the migration path from the current Ed25519-only transaction authorization model to a hybrid post-quantum model. The goal is crypto-agility without invalidating historical transactions or silently changing the v1/v2 consensus domains.

## Threat model

PulseDAG currently authorizes ordinary transaction inputs with Ed25519. A sufficiently capable fault-tolerant quantum computer running Shor's algorithm would break the discrete-log assumption behind Ed25519 once the relevant public key is available.

Generic quantum search also reduces the effective preimage security of an n-bit hash toward roughly n/2 bits. The legacy `pulse1` address path commits to only 160 bits of SHA-256 output. Post-quantum addresses therefore must not reuse that truncated commitment.

## Target authorization suite

The first post-quantum suite is hybrid:

- classical signature: Ed25519;
- post-quantum signature: ML-DSA-65, standardized by FIPS 204;
- authorization rule after activation: **both signatures MUST validate over the same chain-bound canonical signing message**;
- post-quantum address commitment: full SHA3-256 digest with domain separation;
- address prefix: `pulseq1`.

Retaining Ed25519 alongside ML-DSA-65 avoids making security depend exclusively on one new primitive during migration. The hybrid rule must be AND, never OR; accepting either signature would permit a downgrade to the weaker component.

## Canonical envelope v1

Until the transaction wire format gains native tagged byte fields, `TxInput.public_key` and `TxInput.signature` can carry a strict textual envelope.

Public key:

```text
pqc1:<ed25519-public-key-lowerhex>:<ml-dsa-65-public-key-lowerhex>
```

Exact raw component sizes:

- Ed25519 public key: 32 bytes;
- ML-DSA-65 public key: 1,952 bytes.

Signature:

```text
pqc1:<ed25519-signature-lowerhex>:<ml-dsa-65-signature-lowerhex>
```

Exact raw component sizes:

- Ed25519 signature: 64 bytes;
- ML-DSA-65 signature: 3,309 bytes.

Parsing is fail-closed. Unknown envelope versions, uppercase/non-canonical hex, missing components, extra components and wrong lengths are rejected.

## Post-quantum address v1

`pulseq1` commits to both public keys using the following domain-separated material:

```text
SHA3-256(
    "PulseDAG:pq-address:v1" ||
    le_u32(ed25519_len) || ed25519_public_key ||
    le_u32(ml_dsa_65_len) || ml_dsa_65_public_key
)
```

The full 256-bit digest is encoded as lowercase hex after `pulseq1`. It is intentionally separate from the legacy `pulse1` address derivation so a validator cannot accidentally apply Ed25519-only authorization to a post-quantum output.

## Rollout phases

### Phase 1 — crypto-agility foundation (this PR)

- freeze the hybrid key/signature envelope;
- freeze exact component sizes;
- add strict encode/decode helpers;
- add `pulseq1` full-width address commitments;
- add fail-closed unit tests;
- make no consensus activation and add no new cryptographic dependency.

Historical v1/v2 serialization and signature verification remain unchanged.

### Phase 2 — ML-DSA implementation and wallet support

- select a reviewed/audited FIPS 204 implementation suitable for consensus-critical use;
- pin the dependency and update `Cargo.lock` so `--locked` CI remains reproducible;
- add ML-DSA-65 key generation, signing and verification in `pulsedag-crypto`;
- store and zeroize post-quantum private material safely;
- extend wallet derivation/keystore formats with explicit versioning and migration tests;
- ensure the same reviewed transaction bytes are supplied to both signing algorithms.

No in-house implementation of ML-DSA should be introduced.

### Phase 3 — transaction v3 consensus primitives

- introduce frozen chain-bound v3 signing and txid domains;
- verify the Ed25519 and ML-DSA-65 signatures with an AND rule;
- derive the spent output address through `pulseq1` for v3 inputs;
- reject algorithm/version downgrade and mixed malformed envelopes;
- add deterministic golden vectors and cross-chain replay tests;
- add transaction weight/fee rules accounting for large post-quantum keys and signatures.

The current hex envelope is substantially larger than the raw cryptographic material. A native binary tagged representation should be evaluated before production activation to reduce bandwidth and storage overhead.

### Phase 4 — testnet protocol activation

- add a protocol identity/activation gate for v3 rather than modifying v1/v2 semantics;
- exercise mempool, P2P relay, RPC, mining templates, replay and persistence under v3;
- publish wallet migration tooling from legacy `pulse1` outputs to `pulseq1` outputs;
- benchmark verification throughput, transaction propagation and block capacity;
- complete third-party cryptographic/security review before mainnet activation.

### Phase 5 — mainnet migration policy

After a separately reviewed activation decision:

- permit migration spends according to an explicit transition rule;
- prefer/require new value to be locked to `pulseq1` outputs;
- eventually reject creation of new Ed25519-only outputs if governance/release policy chooses to deprecate them;
- retain frozen historical validation paths for deterministic replay.

## Consensus/security invariants

A production v3 implementation must preserve all of these invariants:

1. Both Ed25519 and ML-DSA-65 signatures authenticate the exact same chain-bound message.
2. Unknown algorithms, versions and malformed encodings fail closed.
3. There is no `classical OR post-quantum` fallback in a post-quantum authorization path.
4. `pulseq1` outputs cannot be spent through legacy `pulse1` address derivation.
5. Exact key/signature sizes are enforced before expensive verification work.
6. Legacy v1/v2 canonical bytes and replay behavior remain frozen.
7. Post-quantum activation occurs only through an explicit protocol-version gate.
8. Transaction/block resource limits account for post-quantum bandwidth and CPU cost.
9. Private ML-DSA material is versioned, encrypted at rest and zeroized in memory where practical.
10. Mainnet must not be described as post-quantum resistant until hybrid verification is activated and funds can be migrated to post-quantum outputs.

## Review checklist for activation PRs

- FIPS 204 implementation dependency pinned and security provenance documented;
- `Cargo.lock` committed and `cargo metadata --locked` passes;
- deterministic key/signature vectors and negative vectors included;
- signature malleability/canonical encoding behavior documented;
- chain-id replay protection covered by tests;
- wallet backup/recovery and migration tested;
- P2P/RPC size limits updated;
- mempool fee/weight and DoS limits updated;
- verification benchmarks attached;
- independent cryptographic review completed before mainnet activation.
