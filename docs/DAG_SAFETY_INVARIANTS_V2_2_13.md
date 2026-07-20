# DAG Safety Invariants v2.2.13

PulseDAG v2.2.13 is a safety-audit documentation milestone for the current DAG implementation. It records what the node validates today, which deterministic policies are currently used, and which consensus claims are intentionally **not** made yet.

## Scope and compatibility boundary

PulseDAG currently has a DAG structure and a deterministic local tip policy. The current model stores blocks by hash, tracks a set of tips, records parent-to-child edges, anchors state at a single `genesis_hash`, and maintains `best_height` as the maximum accepted block height.

PulseDAG does **not** yet claim full Kaspa or GHOSTDAG consensus compatibility. DAG terminology, Kaspa-informed design goals, and kHeavyHash/PoW alignment do not imply consensus compatibility. A compatible PoW path only says that header hashing/work validation is aligned with the selected PoW policy; it does not provide GHOSTDAG ordering, blue set selection, finality, pruning, merge-set scoring, or external-network consensus equivalence.

v2.2.13 is a safety audit milestone, not a v2.3.0 readiness declaration. v2.3.0 remains the private-testnet readiness decision milestone.

## Current PulseDAG DAG model

The in-memory chain state contains:

- `dag.blocks`: accepted blocks keyed by block hash.
- `dag.tips`: accepted blocks that currently have no accepted child in local state.
- `dag.children`: parent hash to accepted child hashes.
- `dag.genesis_hash`: the hash of the locally initialized genesis block.
- `dag.best_height`: the maximum accepted block height observed by the node.
- `orphan_blocks`, `orphan_missing_parents`, and `orphan_received_at_ms`: bounded queues and indexes for blocks whose parents are not yet known.

A block header carries an ordered `parents` list, timestamp, difficulty, nonce, Merkle root, state root, `blue_score`, and `height`. Multiple parents are allowed, so accepted block relations form a DAG rather than a single linked list.

## Blocks, tips, children, `genesis_hash`, and `best_height` invariants

For accepted non-genesis blocks:

- Every parent referenced by the block must already exist in `dag.blocks` when the block is accepted.
- Every parent hash must be unique inside the block header.
- Applying a block removes each accepted parent from `dag.tips`, appends the block hash to each parent's `dag.children` entry, inserts the new block hash into `dag.tips`, stores the block in `dag.blocks`, and updates `dag.best_height` to `max(existing_best_height, block.header.height)`.
- `dag.best_height` is expected to match the maximum height across accepted blocks.
- Every tip hash is expected to exist in `dag.blocks`.
- The deterministic selected tip must be a member of `dag.tips` whenever a selected tip can be derived.
- Non-genesis accepted blocks are expected to have at least one parent.

Genesis is special:

- The local genesis block has no parents and height `0`.
- `dag.genesis_hash` anchors the initialized local state and starts as the only tip.
- Replay paths skip the genesis block if it appears in persisted/replayed block lists because the replay state initializes genesis first.

## Parent validation rules

Block validation currently enforces:

- The block hash must not already exist in `dag.blocks`.
- The parent list must not be empty for a submitted non-genesis block.
- The parent list must not contain duplicates.
- Each parent hash must already be present in `dag.blocks`; otherwise validation reports a missing parent.
- Missing-parent outcomes are classified separately from malformed block outcomes so P2P/RPC callers can queue or request dependencies.

These rules validate local structural safety. They do not perform GHOSTDAG merge-set selection or external consensus ordering.

## Height validation rules

For a non-genesis block, expected height is computed as:

```text
max(parent.header.height + 1 for parent in block.header.parents)
```

The submitted block height must exactly equal that expected height. This allows multi-parent blocks to extend the highest referenced parent by one local height step. It is a PulseDAG-local height invariant, not a Kaspa/GHOSTDAG blue-score or selected-parent rule.

## Timestamp validation rules

A submitted block must satisfy all current timestamp checks:

- Timestamp must be greater than zero.
- Timestamp must not be farther in the future than the node's configured maximum future drift allowance.
- Timestamp must be at least as new as the newest referenced parent timestamp.

These checks protect local monotonicity and operator safety. They are not a complete external-network time/median-time consensus policy.

## Coinbase placement rules

For block-level validation:

- A block must contain at least one transaction.
- The first transaction must be coinbase-like.
- No transaction after index `0` may be coinbase-like.

The current coinbase predicate is structural: no inputs, exactly one output, and zero fee. v2.2.13 documents this as the current safety rule; it does not claim final emission, maturity, subsidy schedule, or Kaspa-compatible coinbase semantics.

## Duplicate transaction rules

Within a block:

- Transaction IDs must be unique.
- Duplicate transaction IDs in the same block are rejected.
- Non-coinbase transactions are validated and applied against a cloned working state in block order so duplicate spends inside one block are rejected by UTXO/spent-output checks.

At acceptance level, duplicate block hashes return a duplicate result and do not mutate DAG state. Mempool duplicate transaction handling is separate from block duplicate transaction handling.

## Missing-parent and orphan adoption behavior

When a block references unknown parents:

- Core block acceptance classifies it as `MissingParent`; it is not applied to the DAG.
- P2P/RPC flows can compute the missing parent list, queue the block in `orphan_blocks`, record `orphan_missing_parents`, and record receive time.
- Orphan queues are bounded by count and age; oldest or expired entries are pruned.
- Adoption scans queued orphans, recomputes missing parents, sorts ready orphan hashes deterministically, removes a ready orphan from the queue, and re-runs normal block acceptance.
- An orphan is adopted only if it passes the same PoW, parent, height, timestamp, coinbase, duplicate-transaction, and transaction validation path used for direct acceptance.
- If a queued block still has missing parents, its missing-parent index is refreshed.
- If a queued block becomes ready but fails normal acceptance, it is dropped from the orphan queue rather than bypassing validation.

This behavior is intended to make child-before-parent arrival safe under P2P reordering. It is not a full consensus resolution algorithm for competing DAG branches.

## Tip selection policy

PulseDAG currently uses a deterministic local tip ordering policy:

1. Higher `height` wins.
2. If height ties, higher `blue_score` wins.
3. If blue score ties, newer `timestamp` wins.
4. If timestamp ties, lexicographically higher hash wins.

`preferred_tip_hash` returns the first hash from that sorted tip list. This gives stable local node behavior and stable RPC diagnostics, but it is intentionally simple and PulseDAG-specific. It is **not** full GHOSTDAG, does not compute blue sets or merge sets, and does not establish Kaspa consensus compatibility.

## Replay expectations

Replay paths are expected to rebuild local state deterministically for blocks that satisfy the current validation rules:

- Rebuild from a block list initializes genesis, sorts blocks by height, skips genesis, validates each block, and applies it.
- Rebuild from a snapshot sorts new blocks by height, timestamp, and hash; skips genesis, already-applied blocks, and blocks at or below snapshot height; then validates and applies remaining blocks.
- Defensive replay sorts by height, timestamp, and hash, accepts valid blocks, and reports skipped hashes and reasons for invalid or unavailable dependencies.

Replay order-independence is therefore constrained by parent availability after sorting and by the current validation model. v2.2.13 does not claim arbitrary topological recovery, GHOSTDAG-compatible replay, or consensus compatibility with externally ordered block sets.

## Known current consensus limits

The v2.2.13 audit explicitly records these limits:

- PulseDAG has a DAG structure and deterministic tip policy, but no full GHOSTDAG implementation.
- PulseDAG does not claim full Kaspa consensus compatibility.
- kHeavyHash/PoW alignment does not imply consensus compatibility.
- `blue_score` is currently a header field/tie-break input, not evidence of complete GHOSTDAG blue-set computation.
- Tip selection is a simple deterministic local policy, not a complete selected-parent, merge-set, or finality rule.
- Replay is validation-driven and sorted by local keys; it is not a proof of full order independence for arbitrary DAG histories.
- Orphan adoption revalidates queued blocks but does not resolve all possible adversarial consensus races.
- Coinbase validation is structural and does not yet document a final production emission policy.
- This document is part of v2.2.13 safety audit evidence and must not be read as v2.3.0 readiness.

## Documentation cross-check

Related immutable v2.2.13 tag material:

- [Closing Checklist v2.2.13](https://github.com/AuriaLABS/PulseDAG/blob/v2.2.13/docs/CLOSING_CHECKLIST_V2_2_13.md)
- [Release Notes v2.2.13](https://github.com/AuriaLABS/PulseDAG/blob/v2.2.13/docs/RELEASE_NOTES_V2_2_13.md)
- [Version Matrix](VERSION_MATRIX.md)
