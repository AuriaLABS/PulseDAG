# PR Review: Network reliability and sync recovery

Reviewed commit: `9661d0c` ("Improve libp2p peer scoring and retry backoff under churn")

## Merge blockers

1. **Synthetic churn is injected into the production libp2p runtime loop.**
   - `tick_peer_maintenance()` marks peers as unstable using deterministic pseudo-random logic every heartbeat and then records failures (`(peer_jitter + tick) % 4 == 0`).
   - This is not tied to real dial/connect outcomes and can penalize healthy peers, causing false disconnect state and degraded peer set quality.
   - Impact: can trigger avoidable reconnect behavior and unreliable operator perception of network health.

2. **`connected_peers` no longer reflects actual connectivity.**
   - `connected_peers` is rebuilt from `peer_book` entries where `connected == true`, and `connected` is now driven by synthetic maintenance outcomes rather than actual swarm events.
   - Existing bootstrap events set peers connected once, but periodic maintenance can later mark them disconnected absent real transport evidence.
   - Impact: status API can report incorrect peers and may mislead sync diagnostics.

3. **No persistence/recovery path for peer health state after restart.**
   - `peer_book`, `fail_streak`, and `next_retry_unix` are in-memory only and reset at process restart.
   - No catch-up bootstrap strategy was added to compensate for lost state.
   - Impact: restart behavior can oscillate and does not improve sync catch-up robustness.

## Risky behavior changes (non-blocking once blockers fixed)

- **Backoff policy is global and fixed (2^n with cap at n=6 + jitter 0..2s).**
  This may be too aggressive under partition/rejoin events and too short for persistent failures.
- **No explicit storm guardrails.**
  There is no cap on concurrent retry attempts, no token bucket/rate limiter, and no partition-aware suppression.

## Missing tests

1. Test that peer scoring/backoff is driven by **real connectivity events** (dial success/failure), not periodic synthetic churn.
2. Test for **reconnect storm resistance** with many peers failing simultaneously.
3. Test for **partition/rejoin recovery**: peers should re-enter candidate set promptly on successful reconnection.
4. Test for **restart catch-up** behavior: verify node rehydrates usable peers and resumes sync quickly after restart.
5. Test operator visibility fields in `P2pStatus` for failure reasons/counters, not only last message/event strings.

## Operator visibility gaps

Current status exposes counters and `last_swarm_event`, but there is no structured visibility for:
- per-peer retry deadline,
- fail streak,
- last failure reason,
- dial success/failure counters,
- "sync stalled" indicators.

Add these fields (or an endpoint) to support operational debugging of sync failures.
