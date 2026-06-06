# pulsedag-storage

Persistent storage and database abstraction for PulseDAG.

## Purpose

This crate provides:
- **Block storage** using RocksDB
- **Serialization** with `bincode` and `serde_json`
- **Snapshot** and recovery mechanisms
- **State persistence** for blocks, transactions, and metadata
- **Replay** invariants to verify consistency after restart

## Dependencies

- `pulsedag-core` — core data structures
- `rocksdb` — embedded key-value store (default backend)
- `serde`, `serde_json`, `bincode` — serialization formats
- `proptest` (dev) — property-based testing for consistency checks

## Key Modules

- `db` — RocksDB backend and schema
- `snapshot` — Snapshot creation and recovery
- `replay` — Block replay logic with tip preservation invariant
- `errors` — Storage-specific errors

## Usage Example

```rust
use pulsedag_storage::Database;

let db = Database::open("./path/to/db")?;
db.store_block(&block)?;
let best_tip = db.best_tip()?;
```

## Tests

Run with:
```bash
cargo test -p pulsedag-storage
```

Snapshot + replay invariant test:
```bash
cargo test -p pulsedag-storage replay_from_snapshot_plus_pruned_blocks_preserves_tip
```

## Warnings

- **RocksDB locks:** Only one process can hold the database at a time; ensure exclusive access.
- **Disk usage:** RocksDB can grow significantly; implement pruning or archival policies.
- **Data loss:** Backups and snapshots are critical for production deployments.
