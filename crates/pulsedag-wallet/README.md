# pulsedag-wallet

Wallet-side transaction construction and security foundations for PulseDAG.

## Current status

This crate is **not yet a production-custody wallet**. The v2.4.x hardening program is tracked in issue #819.

Today the crate provides:

- deterministic construction of unsigned PulseDAG transactions from supplied UTXOs;
- basic first-fit UTXO selection;
- explicit secret-formatting redaction through `SecretString`;
- a versioned encrypted-keystore envelope contract for the upcoming wallet keystore implementation.

It does **not** yet provide:

- encrypted key persistence or password-based unlock;
- mnemonic/seed backup and deterministic account derivation;
- wallet account/history persistence;
- a standalone wallet CLI/application;
- watch-only or offline-signing workflows;
- professional fee/coin-selection policy.

Do not describe this crate or the current node wallet RPC handlers as production custody.

## Security boundary

`SecretString` deliberately does not implement `Serialize`, `Clone`, `Deref<Target = str>` or `AsRef<str>`. `Debug` and `Display` render only `[REDACTED]`; code that genuinely needs secret material must call `expose_secret()` explicitly.

This redaction boundary is **not secure memory erasure**. Reviewed zeroization support will be introduced together with the encrypted-keystore dependency update in #819.

`WalletKeystoreEnvelope` defines the public structure of the future version-1 keystore:

- format/version and network identity;
- public address metadata;
- Argon2id KDF metadata;
- XChaCha20-Poly1305 cipher metadata;
- ciphertext only.

The envelope intentionally contains no plaintext private-key, mnemonic, seed or password fields. Encryption/decryption is not implemented by this foundation change.

## Existing transaction construction

```rust
use pulsedag_wallet::build_transaction;

let built = build_transaction(
    "pulse1sender",
    "pulse1recipient",
    100,
    1,
    &available_utxos,
    nonce,
)?;

println!("unsigned txid: {}", built.transaction.txid);
```

The current selector is first-fit and should not be treated as the final wallet coin-selection/privacy policy.

## Dependencies

- `pulsedag-core` — transaction, address, UTXO and error types;
- `serde`, `serde_json` — public envelope and transaction metadata serialization;
- `hex` — encoded signing/keystore metadata.

No new cryptographic dependency is introduced by the secret-boundary/format foundation PR.

## Tests

```bash
cargo test -p pulsedag-wallet
```

The wallet hardening program will expand this matrix with encrypted-keystore round trips, wrong-password/tamper rejection, deterministic restore vectors, secret-scanning regressions and packaged Windows/Linux wallet tests.
