# Native-v3 dormant RocksDB restart and missing-history regression

**Scope:** Focused extraction from the unmerged policy proposal [#1341](https://github.com/AuriaLABS/PulseDAG/pull/1341) for [#1367](https://github.com/AuriaLABS/PulseDAG/issues/1367) (checkpoint provenance and eventual full-vs-pruned monetary replay). This is a **test-only** integration regression using a disposable RocksDB database and test-private keys. Production storage, P2P, mining, RPC and contract activation are unchanged.

## What the two tests actually establish

The tests create two easy-target, real-KHeavyHash native-v3 block envelopes on a test-only chain, canonicalize/encode them through existing `NativeV3EnvelopeRestoreRecordV1`, persist the frames to a disposable real RocksDB database, **close it and reopen it**, decode/verify the records and validate their genesis-anchored **single-parent** linkage.

They also require rejection when an independently stored envelope's committed header/root is corrupted or when an earlier linked frame is missing. A valid standalone descendant PoW record does **not** authorize its incomplete ancestor chain. These are basic fail-closed restore prerequisites.

## Exact local reproduction

```sh
cargo fmt --all -- --check
cargo test --locked -p pulsedag-storage --test native_v3_dormant_rocksdb_roundtrip -- --nocapture
cargo clippy --locked -p pulsedag-storage --test native_v3_dormant_rocksdb_roundtrip -- -D warnings
```

The existing exact-head workspace CI must pass before integration. This test cannot independently establish authenticated multiparent GHOSTDAG selection, blue/red classification, monetary transaction fees/UTXO/reward state, trustworthy checkpoint provenance, pruning with restart, atomic crash rollback, a real production node restart, or convergence at 500ms. Its header `state_root` is deliberately **unverified** and cannot become monetary authority.

The remaining critical path is owner-approved GD-01..07, GD-08 canonical monetary replay, a checkpoint generated from a fully independently authenticated state, and exact full-history versus pruned restore equivalence, including reorg and crash negatives.

**No automatic checkpoint acceptance. Monetary-v3 mining OFF; smart contracts INACTIVE; mainnet/public testnet #781 NO-GO.**
