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

## Local wallet application

The `pulsedag-wallet` executable is the local custody/application boundary. It does not require node/admin RPC access and never sends raw private keys, seeds, mnemonics, passwords, decrypted keystore payloads, or `WalletSession` material to a node.

Supported recovery/read-only commands:

- `restore --keystore <path> --network-profile <profile> --chain-id <id>` restores a deterministic v2 seed keystore from BIP-39 material;
- `address --keystore <path> --account <n> --branch <receive|change> --index <n>` derives one public address from an unlocked deterministic v2 keystore;
- `watch-export --keystore <path> --account <n> --receive-count <n> --change-count <n>` emits the bounded public watch-only manifest;
- `watch-import --manifest <path>` validates/imports a public watch-only manifest and reports public metadata only; it has no signing capability;
- `backup-verify --keystore <path> --manifest <path>` verifies a public watch-only backup manifest against the authenticated deterministic seed and network identity.

Supported local transaction commands:

- `tx-preview --keystore <path> --utxos-file <path> --network-profile <profile> --chain-id <id> --to <address> --amount <n> --fee <n> --max-fee <n> --max-fee-bps <n> --max-inputs <n> --account <n> --branch <receive|change> --index <n>` unlocks only long enough to derive the selected public signer, verifies the expected network identity, validates a local address-UTXO snapshot, builds `DeterministicPlanV1`, and emits the canonical `WalletReviewSummary` plus the unsigned plan;
- `tx-sign --keystore <path> --plan <path> --account <n> --branch <receive|change> --index <n>` validates an imported unsigned plan, refuses non-deterministic/manual nonce policy, signs only through the bounded `WalletSession` deterministic child, and emits network/review metadata plus a signed relay envelope containing the final transaction.

Secrets are never accepted as command-line options. `restore` reads three line-framed stdin values: wallet password, mnemonic, then an optional BIP-39 passphrase (blank or absent means none). `address`, `watch-export`, `backup-verify`, `tx-preview`, and `tx-sign` read one wallet-password line from stdin. `watch-import` consumes no secret.

Local JSON inputs are bounded to 4 MiB before parsing. Transaction-plan import re-runs structural validation and the official CLI signs only `DeterministicPlanV1`; low-level `ExplicitCallerProvidedV1` remains available only as a compatibility/protocol-test API. UTXO snapshots must identify exactly the selected signer address and may not contain entries for another address.

The restore path persists only the encrypted deterministic seed envelope and refuses to overwrite an existing keystore. Mnemonic/passphrase/password/private-key/seed material is not included in JSON output. Transaction preview and signed outputs contain public transaction metadata only.

`tx-preview` and `tx-sign` deliberately form a two-step local/offline authorization flow. There is still no live `balance`, `history`, automatic UTXO discovery, pending-state persistence, or network `broadcast` command in this CLI. A caller may separately submit the emitted fully signed transaction to the existing public signed-transaction relay only after independently verifying the target network/relay identity.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The public node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

After local signing, an online wallet or relay client may submit the fully formed signed transaction to the canonical public endpoint `POST /api/v1/tx/submit`. That endpoint is a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
