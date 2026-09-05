# pulsedag-wallet

Wallet and account management for PulseDAG.

## Purpose

This crate provides:
- **Account structures** for wallet management
- **Serialization** of wallet state (`serde`, `serde_json`)
- **Hex utilities** for address/key encoding
- **Persistence** for encrypted keystores and the versioned pending-transaction journal

## Dependencies

- `pulsedag-core` — core types (transactions, addresses, errors)
- `serde`, `serde_json` — wallet state serialization
- `hex` — address/key encoding

## Key Modules

- `account` — Account metadata and state
- `keys` — Wallet key management (delegates to `pulsedag-crypto`)
- `persistence` — wallet storage interfaces
- `pending` / `pending_persistence` — conservative pending-transaction state and durable generational journal storage
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

- **Pending-state evidence is conservative:** absence from retained node history is never proof that a pending transaction failed and never releases its selected outpoints.
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

- `tx-preview --keystore <path> --pending-journal <dir> --utxos-file <path> --network-profile <profile> --chain-id <id> --to <address> --amount <n> --fee <n> --max-fee <n> --max-fee-bps <n> --max-inputs <n> --high-fee-threshold <n> --high-fee-bps-threshold <n> --ack-self-send <true|false> --ack-spend-all <true|false> --ack-high-fee <true|false> --account <n> --branch <receive|change> --index <n>` unlocks only long enough to derive the selected public signer, verifies the expected network identity, validates a local address-UTXO snapshot, builds `DeterministicPlanV1`, then checks the selected outpoints against the durable pending journal before emitting the canonical `WalletReviewSummary` plus unsigned plan. The complete observed funding snapshot is preserved for spend-all classification; reserved UTXOs are not silently pre-filtered before planning. The three acknowledgement values are explicit, non-secret review decisions: self-send, actual complete-snapshot spend-all, and a fee strictly above either persisted warning threshold fail closed unless the corresponding persisted acknowledgement is `true`. `--max-fee`, `--max-fee-bps`, and `--max-inputs` remain absolute hard caps and cannot be overridden by any acknowledgement;
- `tx-sign --keystore <path> --pending-journal <dir> --plan <path> --account <n> --branch <receive|change> --index <n>` validates the imported unsigned plan, refuses non-deterministic/manual nonce policy, recomputes and revalidates persisted self-send/spend-all/high-fee facts and acknowledgements, acquires the same pending-journal lock before signing, rejects conflicting reservations, signs only through the bounded `WalletSession` deterministic child, then durably reserves the exact final txid, sender and selected outpoints before emitting the signed relay envelope;
- `tx-broadcast --signed <path> --pending-journal <dir> --relay <origin>` validates the secret-free signed envelope and completes relay URL, `/release`, network, capability, version and serialization preflight before crossing the submission boundary. It then requires an exact `signed` journal record matching final txid, sender and transaction input outpoints, durably persists `submission_started`, and only then calls `POST /api/v1/tx/submit`. Relay acceptance is persisted as `relay_accepted`; an explicit generic rejection is persisted as `relay_rejected`; any transport/read/malformed-response failure after submission begins is persisted as `submission_outcome_unknown`. A record already at `submission_started` or any later state cannot be blindly rebroadcast by this command.

A separate public-evidence reconcile binary advances pending state without signing or resubmitting anything:

- `pulsedag-wallet-reconcile --pending-journal <dir> --txid <final-txid> --relay <origin> [--page-size <1..100>] [--max-pages <1..10>]` loads the exact pending transaction and its stored sender/network identity, releases the journal lock while doing bounded public HTTP reads, verifies `/release` network identity plus `explorer_api` and `/address/:address/activity`, then scans retained activity from newest to older pages within the explicit budget. Positive exact-txid evidence may promote state to `observed_mempool` or `confirmed`; `confirmed` is the only automatic state here that releases selected-outpoint reservations. `not_observed`, retained-history exhaustion, and page-budget exhaustion are reported but do not change state or release reservations. The journal is reacquired and revalidated before any positive evidence is persisted, making retry/restart reconciliation idempotent and avoiding a journal lock across network I/O.

Secrets are never accepted as command-line options. `restore` reads three line-framed stdin values: wallet password, mnemonic, then an optional BIP-39 passphrase (blank or absent means none). `address`, `watch-export`, `backup-verify`, `tx-preview`, and `tx-sign` read one wallet-password line from stdin. `watch-import`, `balance`, `utxos`, `tx-broadcast`, `pulsedag-wallet-history`, and `pulsedag-wallet-reconcile` consume no secret.

Local JSON inputs are bounded to 4 MiB before parsing. Transaction-plan import re-runs structural validation and the official CLI signs only `DeterministicPlanV1`; low-level `ExplicitCallerProvidedV1` remains available only as a compatibility/protocol-test API. UTXO snapshots must identify exactly the selected signer address and may not contain entries for another address.

The restore path persists only the encrypted deterministic seed envelope and refuses to overwrite an existing keystore. Mnemonic/passphrase/password/private-key/seed material is not included in JSON output. Transaction preview, signed, broadcast and reconcile outputs contain public transaction metadata only.

`tx-preview`, `tx-sign`, `tx-broadcast`, and `pulsedag-wallet-reconcile` form the supported review -> offline authorization -> durable submission -> public-evidence reconciliation flow. Broadcast and reconcile require HTTPS for non-loopback relays; plain HTTP is accepted only for loopback development. Redirects are disabled and relay responses are bounded. The wallet verifies remote identity before submission or reconciliation and fails closed on network/chain, capability, endpoint, version where applicable, malformed-response or transport mismatch.

Read-only `balance`, `utxos`, `pulsedag-wallet-history`, and `pulsedag-wallet-reconcile` use the same public transport-hardening model and never unlock custody material. Reconcile differs from ordinary history browsing because its query identity comes from the durable pending journal's exact final txid, sender and network rather than from caller-supplied wallet secrets.

This remote identity check is an operational v1 safety boundary, not cryptographic chain binding. PulseDAG v1 signing bytes still do not include `network_profile` or `chain_id`; protocol-level chain binding/replay/RBF/submission-identity semantics remain tracked by #821.

Retained live history is available through `pulsedag-wallet-history` and is used conservatively by `pulsedag-wallet-reconcile`. Neither surface claims complete history after deep pruning. `tx-preview` continues to consume an explicit bounded UTXO snapshot rather than silently fetching spend inputs during signing preparation.

## Pending journal and state contract

The pending journal is wallet-local, versioned, secret-free and bound to exact `network_profile` + `chain_id`. It stores the final signed txid, sender address, exact selected outpoints, conservative state, and relay-rejection observation text when applicable. It never stores a private key, mnemonic, password, decrypted seed or signing session.

The durable store uses an advisory cross-process lock plus immutable generational snapshots and commit markers. Snapshot payloads are bounded and SHA-256-bound; stale generations, cross-network substitution, malformed committed state and digest mismatches fail closed. An orphan snapshot without its commit marker is ignored rather than treated as committed state.

Stable states are `signed`, `submission_started`, `submission_outcome_unknown`, `relay_accepted`, `observed_mempool`, `relay_rejected`, and `confirmed`. Every state except `confirmed` retains the selected-outpoint reservation. In particular, generic public `TX_REJECTED` is only an observation under the current API and is not proof that an earlier submission could not have propagated. A later positive mempool/confirmed observation can therefore strengthen a rejected or unknown record. Retained-history absence is likewise never terminal evidence.

`submission_started` is intentionally non-idempotent as a submission boundary: `signed -> submission_started` may occur only once. A crash after that durable transition must be treated as unresolved; neither restart nor reconcile automatically resubmits the signed transaction.

## Broadcast boundary

Wallet custody and signing stay local to the wallet boundary. The public node/relay does not receive a private key, mnemonic, seed, wallet password, decrypted keystore payload, or `WalletSession` in order to broadcast a payment.

Before `tx-broadcast` submits anything, it parses and validates the signed envelope, constructs and validates the relay origin/client, fetches the public `/release` identity, requires the signed envelope's `network_profile` and `chain_id`, the `signed_transaction_relay` capability, the `signed-transaction-relay-v1` version and the canonical submit endpoint, and serializes the relay body. It then validates the exact pending-journal binding and durably commits `submission_started`. Only after that durable commit can `submit_prepared` begin the `POST /api/v1/tx/submit` operation.

That endpoint remains a transaction-admission boundary only: it does not build the transaction, choose inputs or fees, create nonce policy, or sign on the caller's behalf.

The unversioned `/tx/submit` compatibility path is not part of the `PublicSafe` wallet contract. Transaction nonce/replay/RBF and consensus signing-domain semantics remain a separate protocol decision tracked by #821.
