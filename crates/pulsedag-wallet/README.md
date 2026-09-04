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
- `backup-verify --keystore <path> --manifest <path>` verifies a public watch-only backup manifest against the authenticated deterministic seed and network identity;
- `balance --manifest <path> --branch <receive|change> --index <n> --relay <origin>` selects a public address from a validated watch-only manifest, verifies the relay network identity and explorer surface, then returns its confirmed balance metadata;
- `utxos --manifest <path> --branch <receive|change> --index <n> --relay <origin>` performs the same fail-closed network/surface checks and returns the selected public address's validated UTXO set.

A companion public-only binary, `pulsedag-wallet-history`, exposes retained-node address activity without unlocking custody material:

- `pulsedag-wallet-history --manifest <path> --branch <receive|change> --index <n> --relay <origin> [--limit <1..100>] [--offset <n>]` verifies the watch-only manifest and relay network identity, requires the advertised canonical `/address/:address/activity` explorer endpoint, validates pagination and transaction-state coherence, and emits a machine-readable `history_scope="retained_node_history"` result. This is intentionally not a claim of complete history after deep pruning; durable pruned-history indexing remains separate work.

Supported transaction commands:

- `tx-preview --keystore <path> --utxos-file <path> --network-profile <profile> --chain-id <id> --to <address> --amount <n> --fee <n> --max-fee <n> --max-fee-bps <n> --max-inputs <n> --ack-self-send <true|false> --ack-spend-all <true|false> --account <n> --branch <receive|change> --index <n>` unlocks only long enough to derive the selected public signer, verifies the expected network identity, validates a local address-UTXO snapshot, builds `DeterministicPlanV1`, and emits the canonical `WalletReviewSummary` plus the unsigned plan. Both acknowledgement values are explicit, non-secret review decisions: self-send and actual complete-snapshot spend-all fail closed unless the corresponding persisted value is `true`;
- `tx-sign --keystore <path> --plan <path> --account <n> --branch <receive|change> --index <n>` validates an imported unsigned plan, refuses non-deterministic/manual nonce policy, revalidates the persisted self-send/spend-all acknowledgements and self-verifying complete funding snapshot without accepting acknowledgement overrides, signs only through the bounded `WalletSession` deterministic child, and emits network/review metadata plus a signed relay envelope containing the final transaction;
- `tx-broadcast --signed <path> --relay <origin>` reads that secret-free signed envelope, validates its canonical final txid and signature/public-key material, verifies the relay's public `network_profile`, `chain_id`, relay capability and relay version, then submits only the signed transaction to `POST /api/v1/tx/submit`.

Secrets are never accepted as command-line options. `restore` reads three line-framed stdin values: wallet password, mnemonic, then an optional BIP-39 passphrase (blank or absent means none). `address`, `watch-export`, `backup-verify`, `tx-preview`, and `tx-sign` read one wallet-password line from stdin. `watch-import`, `balance`, `utxos`, `tx-broadcast`, and `pulsedag-wallet-history` consume no secret.

Local JSON inputs are bounded to 4 MiB before parsing. Transaction-plan import re-runs structural validation and the official CLI signs only `DeterministicPlanV1`; low-level `ExplicitCallerProvidedV1` remains available only as a compatibility/protocol-test API. UTXO snapshots must identify exactly the selected signer address and may not contain entries for another address.

The restore path persists only the encrypted deterministic seed envelope and refuses to overwrite an existing keystore. Mnemonic/passphrase/password/private-key/seed material is not included in JSON output. Transaction preview, signed and broadcast outputs contain public transaction metadata only.

`tx-preview`, `tx-sign`, and `tx-broadcast` form the supported review -> offline authorization -> online relay flow. Broadcast requires HTTPS for non-loopback relays; plain HTTP is accepted only for loopback development. Redirects are disabled and relay responses are bounded. The wallet verifies remote identity before submission and fails closed on network/chain, capability, version, malformed-response or transport mismatch.

Read-only `balance`, `utxos`, and `pulsedag-wallet-history` use the same transport-hardening model, derive the queried address from public watch-only material, require exact relay network/chain identity plus the advertised explorer capability and canonical endpoint, and never unlock custody material.

This remote identity check is an operational v1 safety boundary, not cryptographic chain binding. PulseDAG v1 signing bytes still do not include `network_profile` or `chain_id`; protocol-level chain binding/replay/RBF/submission-identity semantics remain tracked by #821.

Retained live history is now available through the separate `pulsedag-wallet-history` binary, while durable history across deep pruning and durable pending-state persistence remain incomplete. `tx-preview` also continues to consume an explicit bounded UTXO snapshot rather than silently fetching spend inputs during signing preparation.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The public node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

Before `tx-broadcast` submits anything, it fetches the public `/release` identity and requires the signed envelope's `network_profile` and `chain_id`, the `signed_transaction_relay` capability, the `signed-transaction-relay-v1` version, and the canonical submit endpoint to match. Only then is the fully formed signed transaction sent to `POST /api/v1/tx/submit`.

That endpoint remains a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
