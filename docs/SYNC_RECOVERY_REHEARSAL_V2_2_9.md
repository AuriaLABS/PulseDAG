# Sync Recovery Rehearsal v2.2.9

## Goal
Validate restart and catch-up behavior when one node is offline and rejoins later.

## Scenario
1. Start Node A and mine/accept blocks.
2. Keep Node B offline while A advances.
3. Start Node B.
4. Observe B discovering announcements/tips, requesting missing blocks, and catching up to current sync capability.

## What to Observe
- `block_received`: inbound block payload reached node.
- `orphan_stored`: child arrived before parent and was queued.
- `missing_block_requested`: missing parent request intent emitted.
- `orphan_retried`: queued orphans retried after parent acceptance.
- `orphan_accepted`: retried orphans that were accepted.
- `orphan_evicted`: orphan pool eviction to enforce bounded capacity.
- Existing announcement/request visibility:
  - `block_announced`
  - `unknown_block_announced`
  - `block_request_sent`

## Safety Expectations
- Unknown parent does not disappear silently: block is retained in orphan pool with missing parent list.
- Missing parent request path is visible through request-intent logs.
- Orphan pool remains bounded by configured limit/age and emits eviction logs when pruning.
- Invalid PoW orphan never becomes accepted during orphan retry.

## Rehearsal Procedure
1. Launch Node A (`scripts/v2_2_9_start_node_a.sh`) and miner A (`scripts/v2_2_9_start_miner_node_a.sh`).
2. Leave Node B stopped initially.
3. Wait for A to build several blocks.
4. Start Node B (`scripts/v2_2_9_start_node_b.sh`).
5. Watch B logs for the event sequence above.
6. Confirm `/runtime` and `/orphans` endpoints show orphan queue activity and eventual reduction after retries.

## Test Coverage (core orphan path)
- Child-before-parent becomes orphan.
- Parent arrival triggers orphan retry and acceptance.
- Invalid orphan PoW remains rejected.
- Orphan capacity is bounded with eviction.
- Duplicate orphan insertion is ignored.
