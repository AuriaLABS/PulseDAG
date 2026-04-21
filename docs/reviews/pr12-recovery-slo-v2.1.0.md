# PR Review: Recovery SLO impact for churn and sync catch-up (v2.1.0)

Reviewed commit: `9661d0c` ("Improve libp2p peer scoring and retry backoff under churn")

## Likely impact on recovery time

- **Partial short-term improvement:** The PR introduces per-peer exponential backoff with jitter (`2^fail_streak + jitter`) and score recovery on success, which can reduce immediate retry spam under repeated failures.
- **Likely net degradation in real recovery SLOs:** The maintenance loop applies synthetic, deterministic instability every 5 seconds and marks peers failed independent of actual libp2p outcomes. This can push healthy peers into backoff and shrink the active peer set, increasing median/tail catch-up time under churn.
- **Observability-induced delay risk:** `connected_peers` is now derived from synthetic health state. During incidents this can mislead operators into diagnosing wrong peers and slow mitigation.

## Operational risks

1. **False negative connectivity state:** Healthy peers can be marked disconnected by synthetic maintenance behavior.
2. **Reconnect oscillation:** Deterministic periodic failure injection can create avoidable reconnect cycles.
3. **Status-plane drift from data-plane truth:** P2P status may diverge from real swarm connectivity because no dial/connect result plumbing exists yet.
4. **No restart continuity for peer health:** All peer health/backoff state is in-memory and resets on restart, risking churny warm-up during recovery windows.

## Missing metrics for SLO management

The current status exposes aggregate counters but not peer-health internals needed for recovery SLOs. Missing:

- per-peer `fail_streak`
- per-peer `next_retry_unix`
- per-peer score distribution (min/p50/p95)
- dial/connect success vs failure counters
- retry attempt counters and suppression counts
- sync catch-up latency histogram (time-to-tip after partition/restart)
- stalled-sync duration and trigger counters

## Missing test coverage

1. **Real-event wiring tests:** verify scoring/backoff updates only from real dial/connect outcomes.
2. **Churn soak tests:** many-peer simultaneous failures with bounded retry pressure.
3. **Partition/rejoin recovery tests:** fast re-admission of recovered peers and bounded time-to-tip.
4. **Restart recovery tests:** peer set and sync catch-up behavior after process restart.
5. **Status correctness tests:** `connected_peers` and peer-health fields match actual swarm state transitions.

## Merge recommendation for PulseDAG v2.1.0

**Recommendation: Do not merge as-is for v2.1.0.**

Merge only after these gates are met:

1. Remove or strictly gate synthetic churn logic from production runtime path.
2. Wire peer health transitions to real swarm dial/connect/disconnect results.
3. Add minimum recovery-SLO metrics and dashboards for churn/catch-up.
4. Add integration tests for partition/rejoin and restart catch-up.

With those fixes, the peer-scoring/backoff design can improve resilience; without them, this PR likely weakens recovery SLO confidence for v2.1.0.
