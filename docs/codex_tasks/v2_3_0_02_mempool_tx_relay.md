# v2.3.0 Task 02 — Private-testnet mempool and transaction relay

## Goal

Complete the v2.3.0 private-testnet mempool baseline and prove deterministic, bounded transaction propagation across real libp2p nodes.

## Required policy

- Maximum mempool transactions: `4096`.
- Maximum tracked spent outpoints: `8192`.
- Fail closed when either bound is reached unless a documented deterministic eviction policy is implemented.
- Preserve duplicate, confirmed, missing-input, duplicate-input, zero-output, malformed, signature, and double-spend rejection.
- Keep per-address caps optional for this milestone.

## Deterministic ordering

Add explicit first-seen logical sequence metadata.

Canonical order:

1. first-seen sequence ascending;
2. txid lexicographic ascending as tie-breaker.

If old persisted states lack first-seen metadata, migrate deterministically and fall back to txid order.

## Confirmed-transaction cleanup

After every accepted block mutation:

- remove transactions included in the accepted block;
- remove transactions invalidated by the new UTXO state;
- rebuild spent-outpoint indexes deterministically;
- run through the same serialized `ChainStateMutationCoordinator` used by accepted state mutations;
- never require manual `/mempool/sanitize` for correctness.

## P2P relay

Use the existing `NewTransaction` message on the real libp2p path.

Required behavior:

- relay once after local or peer admission;
- do not relay rejected transactions;
- deduplicate by `(peer_id, txid)` in a bounded suppression window;
- do not echo a transaction immediately back to its source peer;
- invalid peer transactions must not disconnect an otherwise healthy peer by default;
- bound relay tracking memory and expire entries deterministically;
- retain chain-ID isolation.

## Metrics

Expose at least:

- `mempool_size`;
- `mempool_capacity_max`;
- `mempool_spent_outpoints`;
- `mempool_spent_outpoints_capacity_max`;
- `mempool_admit_total`;
- `mempool_reject_total{reason}`;
- `mempool_confirmed_removed_total`;
- `mempool_reconcile_runs_total`;
- `mempool_reconcile_removed_total`;
- `mempool_relay_total`;
- `mempool_relay_received_total`;
- `mempool_relay_deduplicated_total`;
- `mempool_relay_rejected_total{reason}`;
- `mempool_relay_tracker_entries`;
- `last_mempool_sanitize_unix` and result fields.

## Tests

### Core

- 4096 transactions accepted, transaction 4097 rejected as capacity.
- spent-outpoint index reaches 8192 and rejects further tracked inputs safely.
- deterministic ordering is independent of HashMap insertion order.
- duplicate and double-spend rejection preserve indexes.
- accepted block removes confirmed and newly invalid transactions.
- replay/snapshot restore preserves deterministic mempool metadata or safely rebuilds it.

### P2P

- 3-node real-P2P transaction relay.
- source node admits once; both peers receive and admit once.
- no echo storm.
- duplicate announcements are suppressed.
- invalid transaction is rejected without peer disconnect.
- relay tracker remains bounded after an expiry-window stress fixture.

### Private-testnet gate

Add a reproducible transaction burst drill:

- five nodes;
- submit valid transactions to one node;
- prove convergence of mempool txid sets before mining;
- mine confirmation blocks;
- prove all five mempools clean confirmed transactions;
- include metrics, logs, manifest, archive, and checksum.

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedag-core mempool --locked
cargo test -p pulsedag-p2p transaction --locked
cargo test -p pulsedag-rpc mempool --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

## Guardrails

- External miner architecture remains unchanged.
- No pool logic.
- No consensus/PoW semantic change.
- No version bump.
- Keep `public_testnet_ready=false`.

## PR report

Document policy defaults, rejection taxonomy, relay lifecycle, bounded-memory behavior, and the five-node transaction drill result.