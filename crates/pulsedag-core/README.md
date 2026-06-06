# pulsedag-core

Core data structures and DAG (Directed Acyclic Graph) primitives for PulseDAG.

## Purpose

This crate provides the fundamental building blocks for the PulseDAG protocol:
- **Transaction** serialization/deserialization (serde)
- **Block** structures and validation logic
- **Hash** functions (SHA-2, SHA-3)
- **Proof-of-Work** integration with Kaspa ecosystem (`kaspa-hashes`, `kaspa-pow`)
- **Mempool** reconciliation and conflict resolution
- **Error** types (`PulseError`)

## Dependencies

- `serde`/`serde_json` — serialization framework
- `sha2`, `sha3` — cryptographic hashing
- `hex` — hex encoding/decoding
- `ed25519-dalek` — Ed25519 digital signatures
- `kaspa-hashes`, `kaspa-pow` — PoW and hashing integration with Kaspa

## Key Modules

- `transaction` — Transaction parsing and validation
- `block` — Block data structures
- `mempool` — In-memory transaction pool with reconciliation
- `errors` — Error type definitions (`PulseError`)
- `pow` — Proof-of-Work integration

## Tests

Run tests with:
```bash
cargo test -p pulsedag-core
```

Property-based tests (fuzz-style) are included in:
- `tests/fuzz_parsing_props.rs` — JSON roundtrip invariants

## Warnings

- **Consensus-critical:** Changes to transaction/block serialization or PoW must be reviewed against `docs/POW_SPEC_FINAL.md`.
- **Version lock:** The workspace version is the source of truth; do not update local `Cargo.toml` version manually.
