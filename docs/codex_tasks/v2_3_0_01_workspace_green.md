# v2.3.0 Task 01 — Restore a fully green workspace

## Goal

Make the current `main` commit pass the complete release validation suite without disabling, ignoring, or weakening consensus, DAG, orphan, replay, storage, P2P, RPC, or readiness tests.

## Known blockers

Recent merged PRs reported these pre-existing failures:

1. `orphans::tests::adoption_order_is_deterministic_when_multiple_orphans_become_ready`
   - Current assertion expects one accepted sibling although both valid siblings become ready after their common parent arrives.
   - The correct invariant is that every valid ready orphan is retried and accepted exactly once in deterministic hash order.

2. `replay_order_independence` tests
   - Identify every failing test and record the exact validation error or state divergence.
   - Fixtures must construct blocks against the same canonical replay state/order expected by `sort_blocks_for_deterministic_replay`.
   - Do not weaken state-root, selected-tip, ordered-DAG, parent-closure, or validation checks merely to make fixtures pass.

## Required changes

### Orphan adoption

- Preserve deterministic sorting and deduplication of ready orphan hashes.
- Accept all valid ready siblings, not an arbitrary first sibling.
- Assert:
  - accepted count equals the number of valid ready siblings;
  - each accepted hash appears exactly once;
  - `dag.children[parent]` contains all accepted children in deterministic sorted order;
  - orphan maps and parent indexes contain no stale entries;
  - rejected/malformed siblings do not prevent valid siblings from being accepted.

### Replay determinism

- Make each replay fixture explicit about its canonical replay order.
- For equal-height blocks, use the same ordering key as production: height, timestamp, then hash.
- Build block state roots against the state produced by that canonical order.
- Verify both input permutations rebuild to identical:
  - accepted block set;
  - selected tip and selected chain;
  - ordered DAG and ordered DAG digest;
  - UTXO/state root;
  - merge-set and selection digests;
  - children and tip indexes.
- Add diagnostic output to failing assertions so a future regression identifies the first divergent field/hash.

## Acceptance criteria

All commands must pass on the same commit:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedag-core orphan --locked
cargo test -p pulsedag-core --test replay_order_independence --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

## Guardrails

- No ignored tests.
- No lowering validation strictness.
- No consensus or PoW semantic change.
- No version bump.
- Keep `public_testnet_ready=false`.

## PR report

Include the exact root cause of each failing test, before/after expectations, and complete command results.