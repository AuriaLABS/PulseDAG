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
- reviewed transaction plans, deterministic wallet-plan nonces, and bounded local signing;
- same-directory temporary-file publication with file sync before rename;
- 64 KiB file-size bounds and strict schema validation;
- Unix owner-only (`0600`) keystore/lock permissions and parent-directory sync;
- a dedicated local recovery/watch-only `pulsedag-wallet` application boundary;
- local deterministic transaction preview and offline signing flows;
- identity-checked signed-transaction broadcast through the public relay;
- a repository-only local signing harness for smoke/rehearsal automation;
- keyless node operation with signed-transaction-only relay submission.

Still pending before an official end-user wallet is ready:

- replacement/password-rotation and migration UX;
- balance/history/network-client state and pending-transaction reconciliation;
- automatic UTXO discovery, reservations and advanced coin-selection policy;
- platform-specific packaged-wallet hardening and restore testing;
- broader custody review;
- protocol-level replay/RBF/submission-identity/signing-domain decisions tracked in #821.

## Local wallet application

The `pulsedag-wallet` executable is the local custody/application boundary. It does not require node/admin RPC access and never accepts raw private keys, seeds or mnemonics over RPC.

Supported recovery/read-only commands:

- `restore --keystore <path> --network-profile <profile> --chain-id <id>` restores a deterministic v2 seed keystore from BIP-39 material;
- `address --keystore <path> --account <n> --branch <receive|change> --index <n>` derives one public address from an unlocked deterministic v2 keystore;
- `watch-export --keystore <path> --account <n> --receive-count <n> --change-count <n>` emits the bounded public watch-only manifest;
- `watch-import --manifest <path>` validates/imports a public watch-only manifest and reports public metadata only; it has no signing capability;
- `backup-verify --keystore <path> --manifest <path>` verifies a public watch-only backup manifest against the authenticated deterministic seed and network identity.

Supported transaction commands:

- `tx-preview --keystore <path> --utxos-file <path> --network-profile <profile> --chain-id <id> --to <address> --amount <n> --fee <n> --max-fee <n> --max-fee-bps <n> --max-inputs <n> --account <n> --branch <receive|change> --index <n>` unlocks only long enough to derive the selected public signer, verifies the expected network identity, validates a local address-UTXO snapshot, builds `DeterministicPlanV1`, and emits the canonical `WalletReviewSummary` plus the unsigned plan;
- `tx-sign --keystore <path> --plan <path> --account <n> --branch <receive|change> --index <n>` validates an imported unsigned plan, refuses non-deterministic/manual nonce policy, signs only through the bounded `WalletSession` deterministic child, and emits network/review metadata plus a signed relay envelope containing the final transaction;
- `tx-broadcast --signed <path> --relay <origin>` reads that secret-free signed envelope, validates its canonical final txid and signature/public-key material, verifies the relay's public `network_profile`, `chain_id`, relay capability and relay version, then submits only the signed transaction to `POST /api/v1/tx/submit`.

Secrets are never accepted as command-line options. `restore` reads three line-framed stdin values: wallet password, mnemonic, then an optional BIP-39 passphrase (blank or absent means none). `address`, `watch-export`, `backup-verify`, `tx-preview`, and `tx-sign` read one wallet-password line from stdin. `watch-import` and `tx-broadcast` consume no secret.

Local JSON inputs are bounded to 4 MiB before parsing. Transaction-plan import re-runs structural validation and the official CLI signs only `DeterministicPlanV1`; low-level `ExplicitCallerProvidedV1` remains available only as a compatibility/protocol-test API. UTXO snapshots must identify exactly the selected signer address and may not contain entries for another address.

The restore path persists only the encrypted deterministic seed envelope and refuses to overwrite an existing keystore. Mnemonic/passphrase/password/private-key/seed material is not included in JSON output. Transaction preview, signed and broadcast outputs contain public transaction metadata only.

`tx-preview`, `tx-sign`, and `tx-broadcast` form the supported review -> offline authorization -> online relay flow. Broadcast requires HTTPS for non-loopback relays; plain HTTP is accepted only for loopback development. Redirects are disabled and relay responses are bounded. The wallet verifies remote identity before submission and fails closed on network/chain, capability, version, malformed-response or transport mismatch.

This remote identity check is an operational v1 safety boundary, not cryptographic chain binding. PulseDAG v1 signing bytes still do not include `network_profile` or `chain_id`; protocol-level chain binding/replay/RBF/submission-identity semantics remain tracked by #821.

There is still no live `balance`, `history`, automatic UTXO discovery, or pending-state persistence in this CLI.

## Persistence boundary

`WalletKeystoreFile::try_acquire(path)` holds an exclusive advisory lock for the lifetime of the handle. It is intended to prevent concurrent use by cooperating PulseDAG wallet processes.

`create_new()` validates the encrypted envelope, refuses an existing target, writes the already-encrypted JSON to a random temporary file in the same directory, syncs it, applies supported restrictive permissions, renames it into place, and syncs the parent directory on Unix. Replacement and password rotation are deliberately separate future operations.

`load()` size-bounds the file, parses the strict keystore schema and validates the envelope before returning it.

On platforms where the Unix permission or directory-sync guarantees are not available, the persistence API reports that status rather than claiming the guarantee was enforced.

## Transaction construction

Supported wallet application flows should use `build_deterministic_transaction_plan`. Its v1 nonce is a deterministic wallet-plan identifier/salt derived under the fixed domain `PulseDAG:wallet-plan-nonce:v1` from transaction version, sender, recipient, amount, fee, and the exact selected UTXO outpoints/amounts in plan order.

That gives stable retry behavior: rebuilding the same intent with the same selected inputs produces the same nonce and unsigned template id. Changing the recipient, amount, fee, or selected input set changes the nonce. The nonce is **not** an account sequence, consensus replay barrier, or cryptographic chain binding. Network/chain identity remains a separate fail-closed wallet check before signing and broadcast, and PulseDAG v1 signing bytes still do not include that identity.

The low-level `build_transaction_plan(..., nonce)` API remains available for compatibility and protocol tests, where the caller intentionally controls the v1 nonce. The official `pulsedag-wallet` transaction flow does not sign that explicit-caller policy.

```rust
use pulsedag_wallet::build_deterministic_transaction_plan;

let plan = build_deterministic_transaction_plan(
    network_identity,
    spend_policy,
    intent,
    &available_utxos,
)?;

println!("reviewed nonce: {}", plan.transaction.nonce);
```

The current first-fit selector is a construction primitive, not the final privacy/coin-selection policy.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

Before `tx-broadcast` submits anything, it fetches the public `/release` identity and requires the signed envelope's `network_profile` and `chain_id`, the `signed_transaction_relay` capability, the `signed-transaction-relay-v1` version, and the canonical submit endpoint to match. Only then is the fully formed signed transaction sent to `POST /api/v1/tx/submit`.

That endpoint remains a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf. The historical `/wallet/new`, `/wallet/sign` and `/wallet/transfer` names are permanent fail-closed tombstones; there is no compatibility feature that restores raw-key node signing.

The repository's `pulsedag-wallet-harness` signs smoke/rehearsal transactions locally, derives the supported deterministic v1 wallet nonce internally, and accepts its keystore password through stdin only. The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Protocol replay/RBF and consensus signing-domain semantics remain a separate decision tracked by #821.