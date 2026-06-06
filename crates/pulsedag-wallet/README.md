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
