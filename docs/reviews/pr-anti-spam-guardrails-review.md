# PR Review: Anti-Spam Guardrails

## Scope reviewed

- Transaction intake and validation (`/tx/submit`, `/wallet/transfer`, inbound p2p tx path).
- Block intake and orphan handling for inbound p2p blocks.
- Mining-pool share/config endpoints where request volume can be attacker-controlled.
- Runtime counters/events and what operators can observe when requests are dropped/rejected.

---

## Executive summary

The codebase has **good correctness validation** (txid recomputation, signature checks, duplicate-input checks, and block structure checks), but currently has **limited anti-spam protection at the transport and admission-control layer**:

1. **No explicit request rate caps** on high-risk write endpoints.
2. **No mempool-size admission limit**; valid transactions can accumulate without a hard bound.
3. **Unbounded in-process queues and dedupe sets** in p2p runtime can grow under sustained adversarial traffic.
4. **Drop/reject observability is mostly aggregate counters + log lines**, without structured per-reason metrics for RPC/p2p tx rejections.
5. **Few anti-abuse tests** exist; most tests today are API-shape/p2p-peer-health focused, not spam-path focused.

---

## Detailed findings

### 1) Rate caps

**Status:** ❌ Missing for critical endpoints.

- The router exposes mutating endpoints like `/tx/submit`, `/wallet/transfer`, `/mining/submit`, `/mining/jobs/submit`, and others without middleware-based rate limiting or per-IP/per-key quotas.
- `post_tx_submit` directly attempts mempool acceptance and optional p2p broadcast for each call; there is no short-circuit limiter in handler or shared middleware.
- Mining pool handlers accept share/config submissions without visible caller throttling.

**Risk:** Bot traffic can force high-frequency validation/signature checks and persistent state writes, degrading node responsiveness.

### 2) Fast-fail validation paths

**Status:** ⚠️ Partially good, partially expensive.

What is good:
- `validate_transaction` fails early on empty inputs/outputs, zero outputs, duplicate inputs, and txid mismatch before signature verification.

What is missing:
- No lightweight pre-check on global pressure (e.g., mempool saturation) before deeper validation work.
- No fast fail for oversized request payload frequency or sender abuse heuristics.

**Risk:** Under flood conditions, node still does full validation work for many requests that should be dropped earlier due to policy pressure.

### 3) Abuse amplification risks

**Status:** ❌ Present in multiple paths.

- **Unbounded channels:** p2p runtime uses unbounded inbound/outbound channels, so burst traffic can increase memory pressure.
- **Unbounded dedupe growth:** `seen_message_ids` is a `HashSet` with no explicit pruning/TTL cap in p2p state.
- **No mempool hard cap:** accepted txs are kept in-memory until mined/reconciled; no max count/byte policy enforcement is visible in acceptance path.
- **Per-accepted write amplification:** accepted submissions persist chain state and may broadcast to p2p, increasing I/O and fan-out cost per accepted tx.

### 4) Accidental rejection of honest traffic (false-positive risk)

**Status:** ⚠️ Medium risk due to strict-but-static checks.

- Duplicate-spend guard based on `spent_outpoints` is correct for conflict prevention, but if mempool cleanup lags or stale entries linger, honest replacement/refresh behavior is blocked (there is no RBF-like policy).
- Future timestamp tolerance is static (`max_future_drift_secs` default 120). Nodes with clock skew or delayed propagation may reject otherwise honest blocks.
- Duplicate tx detection by txid is strict and expected, but operationally there is limited context returned to clients beyond generic rejection code in RPC.

### 5) Observability for drop reasons

**Status:** ⚠️ Partial.

- Runtime counters track totals (accepted/rejected/duplicate p2p txs/blocks), which is useful.
- However, rejections are not consistently tagged with structured reason enums/counters (e.g., `invalid_signature`, `utxo_not_found`, `double_spend`, `txid_mismatch`, `mempool_full`).
- RPC `TX_REJECTED` currently returns a generic code with message text; there is no standardized machine-readable reject reason taxonomy across handlers.

**Operational impact:** Incident response cannot quickly distinguish benign malformed traffic from targeted abuse without parsing logs.

### 6) Weak config defaults

**Status:** ⚠️ Mixed.

- Some defaults are safety-oriented (`prune_require_snapshot=true`, nonzero drift and intervals).
- But anti-spam-specific defaults are weak because critical controls are absent by default (rate limits, bounded queues, mempool max policy, per-peer penalties on invalid message rate).

---

## Missing tests (high-priority)

1. **RPC rate-limiter behavior tests**
   - Ensure high-rate submit calls return throttled responses and do not starve normal traffic.
2. **Mempool saturation admission tests**
   - Validate behavior when mempool at cap: deterministic reject reason, no extra writes.
3. **P2P flood resilience tests**
   - Large unique message-id stream should not allow unbounded memory growth.
4. **Drop-reason observability tests**
   - Each rejection path increments the expected reason-specific metric/event key.
5. **False-positive regression tests**
   - Honest tx replay / delayed block with realistic clock skew should have predictable acceptance policy.

Current test coverage in reviewed files is sparse for these scenarios and mainly checks API response shape and peer-health bookkeeping.

---

## Recommendations (ordered)

1. Add global and route-specific rate limiting middleware (token bucket/leaky bucket) for write endpoints.
2. Add mempool admission bounds (`max_tx_count` and/or memory budget) with explicit reject code (`MEMPOOL_FULL`).
3. Replace unbounded p2p channels with bounded channels + overflow handling/metrics.
4. Add bounded/expiring cache for `seen_message_ids`.
5. Introduce structured reject reason enum surfaced in:
   - RPC response `error.details.reason`
   - runtime counters (per reason)
   - runtime events
6. Add anti-spam test suite covering flood, saturation, and false-positive edge cases.
