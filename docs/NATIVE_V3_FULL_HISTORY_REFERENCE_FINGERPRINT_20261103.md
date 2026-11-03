# Native-v3 complete-history fingerprint: independent Python/Rust reference (DORMANT)

**Scope:** [#1367](https://github.com/AuriaLABS/PulseDAG/issues/1367), GD-07 checkpoint provenance prerequisite. Based on `main@0ec98a0ff0d29f2070549380bc6d6416bb47e2c6` after merged [#1368](https://github.com/AuriaLABS/PulseDAG/pull/1368). **TEST-ONLY, NO CHECKPOINT AUTHORITY.**

The missing foundation for meaningful full-history versus pruned-history comparison is an **independently reproducible reference fingerprint of every authenticated ancestor**, including nonselected side/red predecessors, for a fully available native-v3 header-DAG. Prior tests showed 20 PoW-valid selected-parent retarget samples can remain identical when those ancestors are removed. A lone self-claimed checkpoint root cannot prove that they existed.

This slice adds:
- `scripts/native_v3_full_history_reference_fingerprint.py`: Python-stdlib independent SHA-256 encoding and ancestry traversal, grounded in the existing independently generated complete-byte SHA256/KHeavyHash fork corpus; asserts the frozen corpus equals `native_v3_crosslang_ghostdag_vectors.generate()`, checks missing side-parent rejection, map-order independence and foreign genesis identity rejection.
- `crates/pulsedag-core/tests/native_v3_full_history_reference_fingerprint.rs`: native Rust cross-check; first authenticates **all** reachable headers (including nonselected sibling) through the existing real-KHeavyHash, frozen-genesis full-DAG verifier, then encodes the identical binary closure; checks alternative PoW-valid tips produce different fingerprints and that missing indispensable side ancestry is refused.
- `docs/fixtures/native_v3_full_history_reference_trial.json`: immutable independent X0 and X1 expected digests with the exact test chain identity and genesis.
- Exact-head CI wiring and strict Rust test Clippy, without introducing any production callsite or state mutation.

## Research reference encoding v1 — deliberately non-normative

The full-history reference is `SHA256(payload)`. Every integer is unsigned little-endian. A string is encoded as a **u32 byte length** followed by its raw UTF-8 contents. All hashes are exactly 32 raw decoded bytes. The byte stream is:

1. length-prefixed ASCII domain `PulseDAG:dormant-v3-full-history-reference:v1`
2. length-prefixed frozen test chain ID
3. frozen genesis Unix-nanosecond timestamp (`u64`)
4. independently reconstructed frozen genesis hash (32 bytes)
5. number of reachable full-genesis headers (`u32`)
6. for each completely verified reachable header, sorted by **verified height then canonical 32-byte hash**: hash (32 bytes), height (`u64`), timestamp nanoseconds (`u64`), direct-parent count (`u32`), and each canonically ordered direct-parent hash (32 bytes).

The entire header hash already commits to version, parents, timestamp, difficulty, nonce, Merkle root, state root, blue score and height, but **does NOT prove those fields' economic validity**. Parent-before-child (height/hash) is a **diagnostic traversal**, never GHOSTDAG canonical monetary order. The string `full-history reference` explicitly means a comparison target for the tiny **fully present** test corpus, not a checkpoint, validated state root or trust anchor.

| Full verified tip | Frozen test-only SHA-256 reference |
| --- | --- |
| X0 (K=0 proposal variant) | `e86075be1831d4ba453861aeadd67124e1929e98210c022bcb09c414fa9c1467` |
| X1 (K=1 proposal variant) | `14fbdce48347cb5624d6a074b47b53e132f429eee993b95784828b18c46720b1` |

The generator assumes **full unpruned ancestry** and uses an externally fixed research chain ID, genesis timestamp and genesis hash. An unrelated extra header is ignored if not an ancestor; a missing sibling inside the reachable closure causes verification failure. It never accepts a purported checkpoint, never synthesizes missing history and never grants replay or restoration authority.

## Critical missing gates

- The full production GD-01..07 versioned selected-parent/K, work width/numerator/anticone/retarget policy remains **UNAPPROVED**. This test does not select parents, validate a final work policy or attest any real network.
- **No monetary state or finalized state root** is computed: ordinary signed UTXO transfers, red transaction eligibility, fees, rewards, issuance and maturity require GD-08 canonical economic ordering/replay and an independent reference implementation.
- No stored checkpoint identity/provenance, atomic RocksDB restore, pruning/restart, reorg, trusted authority to accept a full-history commitment, 500ms network or public GO exists here. A matching fingerprint for an untrusted or self-generated history **cannot** establish canonical chain state.
- The remaining #1367 acceptance criteria require a separately approved policy and real full-DAG monetary reference, then positive full-history versus pruned restore differential tests. Production must continue to reject incomplete ancestor/history proof.

**Native-v3 monetary mining OFF; smart contracts INACTIVE/fail-closed; [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) mainnet/public parallel testnet NO-GO.**
