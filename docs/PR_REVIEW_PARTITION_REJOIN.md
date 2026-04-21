# PR Review: Partition/Rejoin behavior (3–5 nodes)

Scope reviewed: `crates/pulsedag-p2p/src/lib.rs` from commit `9661d0c`.

## Verdict

**BLOCKER** due to non-recovery/sync-divergence risk.

## Findings against requested checks

### 1) Failure to converge after rejoin — **BLOCKER**

- `register_peer_result` removes peers from `connected_peers` immediately when a single failure is recorded (`connected = false`), and only adds peers back when a later retry succeeds.
- In the current runtime skeleton, retries are driven by `tick_peer_maintenance` every 5 seconds and may keep marking peers unstable by deterministic jitter/tick logic.
- There is no convergence criterion or bounded time-to-recovery SLO check in code/tests for 3–5 node churn/rejoin scenarios.

**Risk:** under partition/rejoin churn, the active peer set can flap and fail to reconverge within predictable bounds.

### 2) Stale peer state — **BLOCKER**

- Peer liveness is represented by a single boolean `connected` and `next_retry_unix` in memory only.
- No separation between transport-connectivity vs sync-health vs gossip-health; no timestamped freshness for last successful data exchange.
- `connected_peers` is derived only from currently `connected == true`, so peers can disappear from status due to transient probe outcomes without richer context.

**Risk:** operators and higher layers may act on stale/incomplete connectivity state and miss real recovery progress.

### 3) Bad backoff resets — **CONCERN**

- On success, `fail_streak` resets to 0 and `next_retry_unix = now` immediately.
- This is acceptable for full success, but there is no partial-success path and no hysteresis/cooldown guard to prevent oscillation when links are unstable.
- Exponential backoff caps at `2^6` seconds (+ jitter), which may be too short for prolonged churn and may induce synchronized retry pressure in small clusters.

**Risk:** repeated fast retry/reset cycles can amplify churn.

### 4) Repeated disconnect loops — **BLOCKER**

- Maintenance loop applies deterministic `unstable` outcomes from `(peer_jitter + tick) % 4`, forcing periodic failure transitions.
- No loop-detection metric/circuit breaker (e.g., disconnects per peer per window, flapping threshold, quarantine window).

**Risk:** persistent disconnect/reconnect oscillation without automatic dampening.

### 5) Weak or missing recovery metrics — **BLOCKER**

- `P2pStatus` exports aggregate counters (`broadcasted_messages`, `publish_attempts`, etc.) but lacks peer recovery metrics such as:
  - reconnect attempts/successes/failures per peer,
  - current fail streak per peer,
  - time since last successful peer exchange,
  - convergence duration after partition heal,
  - flapping/disconnect-loop counters.

**Risk:** recovery behavior cannot be measured against SLOs.

### 6) Missing integration tests for churn and rejoin — **BLOCKER**

- Added tests are unit-only and validate local score/backoff transitions for a single peer.
- No integration tests covering 3–5 node partition, rejoin, and eventual convergence.
- No assertions for non-divergence or bounded recovery time.

## Required follow-ups before approval

1. Add integration tests simulating 3-, 4-, and 5-node partition/rejoin with explicit convergence assertions.
2. Add peer-level recovery telemetry and expose it via status/runtime endpoints.
3. Introduce anti-flap logic (hysteresis/circuit breaker) and validate with churn tests.
4. Define and enforce recovery SLO in tests (e.g., max time/ticks to restored connectivity set + sync target catch-up).
5. Ensure peer state distinguishes connection state from sync state to avoid stale-status decisions.
