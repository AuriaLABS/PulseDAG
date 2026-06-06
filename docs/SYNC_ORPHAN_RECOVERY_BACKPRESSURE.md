# Sync orphan recovery backpressure and aging metrics

The block orphan pool is intentionally bounded so missing-parent recovery cannot grow without
limit during partitions, miner bursts, or reconnect storms.  Orphans are tracked in memory by block
hash, by the missing-parent hashes they wait for, and by their first-seen timestamp.

## Bounded queue behavior

- `DEFAULT_ORPHAN_MAX_COUNT` caps the queued orphan block count at 512.
- `DEFAULT_ORPHAN_MAX_AGE_MS` caps orphan residency at 15 minutes.
- Inserting a new orphan calls `queue_orphan_block_bounded`, which indexes missing parents,
  records the receive timestamp, and then prunes expired entries and oldest overflow entries.
- Pruning removes the block, its missing-parent index entries, and its timestamp together, so
  `/sync/missing` cannot retain dangling orphan index entries after backpressure eviction.
- Evictions increment `orphan_blocks_evicted` and emit an `orphan_evicted` log with the remaining
  orphan count.

This means a non-zero orphan backlog is either actively waiting on missing parents, below the
bounded capacity/age limits, or has already started dropping oldest/expired entries with explicit
operator-visible counters.

## Aging metrics

Operators can inspect backlog age directly instead of inferring it from counts:

- `oldest_orphan_age_secs`: age of the oldest queued orphan block.
- `oldest_missing_parent_age_secs`: age of the oldest missing-parent wait, derived from the oldest
  orphan waiting on any currently missing parent and the oldest pending GetBlock request.
- `max_orphan_age_secs`: legacy alias retained for compatibility; it reports the same value as
  `oldest_orphan_age_secs`.

The metrics are exported through `/sync/status`, `/p2p/status`, and `/metrics`.

## Reprocess accounting

Every orphan reprocess pass records attempts and outcomes:

- `orphan_reprocess_attempts`: number of queued orphan blocks retried.
- `orphan_reprocess_success`: number of retried orphans accepted into the DAG.
- `orphan_reprocess_failed_missing_parent`: retries that still failed because a parent was missing.
- `orphan_reprocess_failed_persist`: recovery/adoption passes that could not be persisted.
- `orphan_reprocess_failures_by_reason`: deterministic reason buckets for all non-success retry
  outcomes, including `missing_parent`, `invalid_pow`, `invalid_transaction`, `malformed`,
  `duplicate`, or normalized validation rejection messages.
- `last_orphan_reprocess_failure_reason`: the most recent failure bucket observed by the runtime.

These counters reset on node restart along with the other runtime counters.
