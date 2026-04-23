# Fuzz + Property Coverage (v2.2 hardening)

This repository currently keeps initial abuse/invariant coverage lightweight with
property-driven harnesses that run in normal `cargo test` workflows.

## Included harnesses

- `crates/pulsedag-core/tests/fuzz_parsing_props.rs`
  - JSON roundtrip invariants for `Transaction` and `Block` parsing/serialization.
  - Byte-level fuzz-style parser abuse for `Transaction` deserialization.
- `crates/pulsedag-core/src/mempool.rs` (test module)
  - Invariant: mempool reconciliation never keeps more than one transaction for a
    single conflicting input outpoint.
- `crates/pulsedag-storage/src/lib.rs` (test module)
  - Invariant: replay from snapshot + pruned block set preserves best tip.
- `crates/pulsedag-rpc/src/handlers/blocks.rs` (test module)
  - Invariant: limit normalization for list/page endpoints always respects caps and defaults.

## Local runs

```bash
cargo test -p pulsedag-core --test fuzz_parsing_props
cargo test -p pulsedag-core mempool::tests::reconcile_never_keeps_more_than_one_conflicting_spend
cargo test -p pulsedag-storage replay_from_snapshot_plus_pruned_blocks_preserves_tip
cargo test -p pulsedag-rpc handlers::blocks::tests::limit_normalization
```

## CI

CI executes a lightweight property subset via `.github/workflows/ci.yml` with
`PROPTEST_CASES=32` to keep runtime bounded while exercising abuse/invariant paths.
