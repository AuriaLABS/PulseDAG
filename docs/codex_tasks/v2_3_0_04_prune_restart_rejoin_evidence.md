# v2.3.0 Task 04 — Non-zero pruning, restart, and rejoin evidence

## Goal

Convert the retained-set, non-zero pruning, snapshot+delta, restart, and offline-rejoin unit foundations into a reproducible private-testnet release gate with complete artifacts and checksums.

## Required operational drill

Use five real nodes and external miners.

1. Start a healthy five-node private network.
2. Mine enough blocks and side-DAG activity to exceed a test retention boundary.
3. Create a validated snapshot and record its generation, state root, selected tip, ordered DAG tip, retained-set metrics, and checksum.
4. Trigger pruning with a fixture profile that guarantees `blocks_pruned_total > 0`.
5. Verify retained storage and retained memory digests match on every node.
6. Stop one non-root node cleanly.
7. Restart it from persisted snapshot+delta state.
8. Verify the restarted node has the same selected tip, ordered DAG tip, state root, retained parent closure, and readiness as peers.
9. Stop or isolate that node again while miners advance by at least 64 selected blocks.
10. Reconnect it and require selected-tip inventory plus selected-segment catch-up.
11. Verify final five-node convergence.

## Retained-set invariants

The evidence must distinguish intentional historical pruning from corruption.

Required fields:

- prune boundary height;
- blocks considered and pruned;
- selected blocks retained;
- side-DAG blocks retained;
- parent-closure blocks retained;
- finality-window blocks retained;
- historical blocks eligible for deletion;
- retained storage hash digest;
- retained memory hash digest;
- storage-only retained hashes;
- memory-only retained hashes.

Pass requires:

- `blocks_pruned_total > 0`;
- retained storage digest equals retained memory digest;
- no storage-only or memory-only retained hashes;
- every retained block has required parent closure;
- no accepted block inside the retention set is terminalized or quarantined;
- live `ChainState` is never overwritten by a captured snapshot;
- concurrent accepted parent/child visibility remains intact during pruning.

## Restart invariants

After restart from snapshot+delta:

- chain ID matches;
- selected height/tip matches peers;
- ordered DAG tip matches peers;
- ordered-DAG state root matches peers;
- retained accepted-hash digest matches peers;
- storage/memory retained digests match;
- node health and readiness pass;
- no active orphan/missing-parent/storage blockers;
- snapshot verification has no stable corruption failure.

## Rejoin invariants

After the node is offline for at least 64 selected blocks:

- fresh remote selected-tip inventory is visible;
- canonical gap reflects the real peer gap;
- a correlated selected-segment session starts;
- blocks are requested, received, applied, and completed in chunks;
- final selected tip, ordered DAG, state root, and retained digests match all peers.

## Evidence package

Produce:

- command and environment capture;
- evaluated commit and branch;
- version guard output;
- per-node pre-prune status/readiness/checks/metrics;
- snapshot metadata and checksum;
- prune report per node;
- process stop/start timeline;
- restart captures;
- offline/rejoin timeline;
- selected-segment timeline;
- final convergence table;
- incident/warning list;
- manifest JSON;
- `evidence.tar.gz` and sha256.

## Harness behavior

- Fail if pruning is requested but every cycle reports zero pruned blocks.
- Fail if restart or rejoin phases are skipped.
- Fail if a PASS manifest omits snapshot, prune, restart, or rejoin fields.
- Classify startup, snapshot, pruning, restart, convergence, storage, readiness, and evidence-consistency dimensions separately.

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedag-storage pruning --locked
cargo test -p pulsedag-storage snapshot --locked
cargo test -p pulsedag-core replay --locked
cargo test -p pulsedagd auto_prune --locked
cargo test -p pulsedagd restart_rejoin --locked
cargo test --workspace --locked snapshot_restore
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

## Guardrails

- Do not delete blocks required by the retained parent closure.
- Do not republish stale snapshot state into live memory.
- No consensus/PoW semantic changes.
- No version bump.
- Keep `public_testnet_ready=false`.

## PR report

Include the first real non-zero pruning metrics, snapshot checksum, restart result, offline gap, selected-segment recovery metrics, final retained digests, and archive checksum.