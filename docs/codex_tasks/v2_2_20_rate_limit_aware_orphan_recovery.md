# v2.2.20 Codex task: rate-limit aware orphan recovery

## Status

Current post-#601 evidence:

- 5N/1M baseline: PASS on `af4e3ae0076840fe1eecd8e7ef32f7d23b5ed36c`.
- 5N/4M stress observe: still `OBSERVE_FAIL`.
- Under 5N/4M, several miners now submit continuously, but one node can still become listener-alive/RPC-timeout and the surviving nodes saturate orphan/missing-parent backlog.

Observed 5N/4M pattern:

```text
orphan_count=512
pending_missing_parents=512
missing_parent_entries=0
inv_hashes_requested=0
peer_health:rate_limited very high
```

This means the backlog is no longer just a plain missing-parent lookup problem. Under rate limiting, the system can end with a full orphan queue but no currently actionable missing-parent index/request state.

## Goal

Add a narrow, non-consensus, rate-limit-aware recovery path so 5N/4M does not leave orphan backlog in an opaque saturated state.

The end state must never be:

```text
pending_missing_parents > 0
missing_parent_entries = 0
inv_hashes_requested = 0
```

unless the backlog is explicitly classified as bounded/stale/evictable with metrics.

## Required behavior

When the recovery tick sees orphan backlog with no actionable request state:

1. Rebuild orphan parent indexes from queued orphan blocks.
2. Recompute orphan missing roots.
3. If active peers exist but the request tracker is rate-limited, mark the backlog as `retryable_rate_limited` instead of leaving it opaque.
4. If roots cannot be requested because the queue is saturated, increment a bounded suppression metric and keep the roots visible in runtime state.
5. If the orphan queue is full and no roots can be recovered, classify oldest entries as stale/evictable and expose that count.
6. Recovery must remain bounded per tick and must not monopolize the runtime.

## Suggested files

Start by inspecting:

```text
apps/pulsedagd/src/main.rs
apps/pulsedagd/src/block_request.rs
crates/pulsedag-core/src/lib.rs
crates/pulsedag-rpc/src/api.rs
```

Do not start by changing consensus. This is sync/recovery accounting and bounded request scheduling only.

## Suggested implementation sketch

Add or reuse runtime fields for:

```text
orphan_backlog_retryable_rate_limited
orphan_backlog_request_queue_saturated
orphan_backlog_eviction_candidates
last_orphan_backlog_recovery_action
last_orphan_backlog_recovery_reason
```

In the recovery tick:

```text
if orphan_count > 0 {
    if pending_missing_parents > 0 && missing_parent_entries == 0 {
        rebuild_orphan_parent_index(...);
        recompute orphan_missing_roots(...);
    }

    if roots_exist && active_peers_exist {
        try request roots through BlockRequestTracker;
        if request suppressed due rate limit/backpressure {
            record retryable_rate_limited / queue_saturated;
        }
    }

    if orphan queue is full and no request can be issued {
        classify stale/evictable candidates, bounded per tick;
    }
}
```

## Acceptance criteria

Required:

```text
5N/1M remains PASS
5N/2M remains at least as good as post-#600/#601
No consensus-rule changes
No PoW semantic changes
No smart contracts
No pool logic
Miner remains external
```

For 5N/4M, acceptable intermediate result:

```text
No opaque saturated state:
  orphan_count=512
  pending_missing_parents=512
  missing_parent_entries=0
  inv_hashes_requested=0

If still FAIL, evidence must show one of:
  roots requested
  retryable_rate_limited > 0
  request_queue_saturated > 0
  stale/evictable candidates > 0
```

Final target:

```text
5N/4M OBSERVE_FAIL becomes bounded and diagnosable,
or PASS if backlog drains and final tips converge.
```

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Then:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=2700 bash scripts/v2_2_20_private_5n_1m_rehearsal.sh
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=2700 bash scripts/v2_2_20_private_5n_2m_rehearsal.sh
DURATION_SECS=800 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=3000 bash scripts/v2_2_20_private_5n_4m_stress_observe.sh
```
