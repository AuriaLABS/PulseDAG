# pulsedag-wallet

Wallet and account management for PulseDAG.

## Purpose

This crate provides:
- **Account structures** for wallet management
- **Serialization** of wallet state (`serde`, `serde_json`)
- **Hex utilities** for address/key encoding
- **Persistence** interfaces (storage-agnostic)

## Dependencies

- `pulsedag-core` — core types (transactions, addresses, errors)
- `serde`, `serde_json` — wallet state serialization
- `hex` — address and key encoding

## Key Modules

- `account` — Account metadata and state
- `keys` — Wallet key management (delegates to `pulsedag-crypto`)
- `persistence` — Traits for wallet storage backends
- `errors` — Wallet-specific error types

## Usage Example

```rust
use pulsedag_wallet::Wallet;

let wallet = Wallet::new()?;
let account = wallet.create_account()?;
println!("Address: {}", account.address);
```

## Tests

Run with:
```bash
cargo test -p pulsedag-wallet
```

## Warnings

- **No built-in persistence:** Wallet implementation is storage-agnostic; use `pulsedag-storage` for production persistence.
- **Key management:** Private keys should be encrypted at rest in production.

## Local recovery and watch-only application

The `pulsedag-wallet` executable is the first end-user-shaped local application boundary. Its current scope is intentionally non-spending and does not require node/admin RPC access.

Supported commands:

- `restore --keystore <path> --network-profile <profile> --chain-id <id>` restores a deterministic v2 seed keystore from BIP-39 material;
- `address --keystore <path> --account <n> --branch <receive|change> --index <n>` derives one public address from an unlocked deterministic v2 keystore;
- `watch-export --keystore <path> --account <n> --receive-count <n> --change-count <n>` emits the bounded public watch-only manifest;
- `watch-import --manifest <path>` validates/imports a public watch-only manifest and reports public metadata only; it has no signing capability;
- `backup-verify --keystore <path> --manifest <path>` verifies a public watch-only backup manifest against the authenticated deterministic seed and network identity.

Secrets are never accepted as command-line options. `restore` reads three line-framed stdin values: wallet password, mnemonic, then an optional BIP-39 passphrase (blank or absent means none). `address`, `watch-export`, and `backup-verify` read one wallet-password line from stdin. `watch-import` consumes no secret.

The restore path persists only the encrypted deterministic seed envelope and refuses to overwrite an existing keystore. Mnemonic/passphrase/password/private-key/seed material is not included in JSON output. Current machine-readable output is deliberately public metadata only.

There are no official `send`, `sign`, `broadcast`, `balance`, or `history` commands in this boundary yet. Transaction spending remains dependent on the broader #819 work and the protocol-level decisions still tracked in #821.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The public node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

After local signing, an online wallet or relay client may submit the fully formed signed transaction to the canonical public endpoint `POST /api/v1/tx/submit`. That endpoint is a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
