# pulsedag-wallet

Wallet-side transaction construction and custody foundations for PulseDAG.

## Status

This crate is still under the v2.4.x wallet-hardening work tracked in #819 and is not yet an official production-custody wallet.

Implemented foundations include:

- unsigned transaction construction and basic first-fit UTXO selection;
- explicit secret wrappers with redacted formatting and zeroizing storage;
- a strict versioned keystore v1 envelope;
- Argon2id + XChaCha20-Poly1305 encryption with authenticated network/account metadata;
- Ed25519 key/address consistency checks;
- create-new/load keystore persistence with a session-scoped advisory file lock;
- same-directory temporary-file publication with file sync before rename;
- 64 KiB file-size bounds and strict schema validation;
- Unix owner-only (`0600`) keystore/lock permissions and parent-directory sync.

Still pending before an official end-user wallet is ready:

- replacement/password-rotation and migration writes;
- deterministic seed/mnemonic restore and account derivation;
- timed wallet lock/unlock state;
- account/history persistence and wallet application UX;
- platform-specific packaged-wallet hardening and restore testing;
- migration away from legacy raw-private-key node RPC handlers;
- final fee/coin-selection policy and broader custody review.

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
