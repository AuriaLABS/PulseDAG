# pulsedag-wallet

Wallet-side transaction construction and custody foundations for PulseDAG.

## Status

This crate is still under the v2.4.x wallet-hardening work tracked in #819 and is not yet an official production-custody wallet.

Implemented foundations include:

- unsigned transaction construction and basic first-fit UTXO selection;
- explicit secret wrappers with redacted formatting and zeroizing storage;
- versioned encrypted private-key and deterministic-seed keystores;
- Argon2id + XChaCha20-Poly1305 encryption with authenticated network/account metadata;
- deterministic hardened Ed25519 derivation and mnemonic restore primitives;
- Ed25519 key/address consistency checks;
- create-new/load keystore persistence with a session-scoped advisory file lock;
- bounded timed wallet lock/unlock state;
- watch-only backup manifest export and verification;
- reviewed transaction plans and bounded local signing;
- same-directory temporary-file publication with file sync before rename;
- 64 KiB file-size bounds and strict schema validation;
- Unix owner-only (`0600`) keystore/lock permissions and parent-directory sync;
- a repository-only local signing harness for smoke/rehearsal automation;
- keyless node operation with signed-transaction-only relay submission.

Still pending before an official end-user wallet is ready:

- replacement/password-rotation and migration UX;
- account/history persistence and wallet application UX;
- platform-specific packaged-wallet hardening and restore testing;
- final fee/coin-selection policy and broader custody review;
- protocol-level nonce/replay/RBF/submission-identity/signing-domain decisions tracked in #821.

## Persistence boundary

`WalletKeystoreFile::try_acquire(path)` holds an exclusive advisory lock for the lifetime of the handle. It is intended to prevent concurrent use by cooperating PulseDAG wallet processes.

`create_new()` validates the encrypted envelope, refuses an existing target, writes the already-encrypted JSON to a random temporary file in the same directory, syncs it, applies supported restrictive permissions, renames it into place, and syncs the parent directory on Unix. Replacement and password rotation are deliberately separate future operations.

`load()` size-bounds the file, parses the strict keystore schema and validates the envelope before returning it.

On platforms where the Unix permission or directory-sync guarantees are not available, the persistence API reports that status rather than claiming the guarantee was enforced.

## Transaction construction

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

The current first-fit selector is a construction primitive, not the final privacy/coin-selection policy.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

After local signing, an online wallet or relay client may submit the fully formed signed transaction to the canonical public endpoint `POST /api/v1/tx/submit`. That endpoint is a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The historical `/wallet/new`, `/wallet/sign` and `/wallet/transfer` names are permanent fail-closed tombstones; there is no compatibility feature that restores raw-key node signing. The repository's `pulsedag-wallet-harness` signs smoke/rehearsal transactions locally and accepts its keystore password through stdin only.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
