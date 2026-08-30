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

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The public node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

After local signing, an online wallet or relay client may submit the fully formed signed transaction to the canonical public endpoint `POST /api/v1/tx/submit`. That endpoint is a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
