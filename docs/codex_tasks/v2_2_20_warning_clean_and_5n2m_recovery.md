# Codex Task: v2.2.20 warning-clean + 5N/2M recovery

Status: READY_FOR_CODEX

## Context

Current v2.2.20 evidence:

- `5N/1M baseline`: PASS at commit `6633962c07bb`.
- `5N/2M intermediate`: FAIL with `peer_count=0`, orphan backlog, pending-missing-parent backlog, and divergent final tips.
- `5N/4M stress observe`: OBSERVE_FAIL with the same failure axis amplified.

The next work must be split into two small PRs.

## PR A: keep strict warning builds clean

Title:

```text
fix: keep block request helpers warning-clean
```

Scope:

- File: `apps/pulsedagd/src/block_request.rs` only.
- Do not delete scheduler methods.
- Do not delete tests.
- Do not change runtime behavior.

Observed strict-build warnings:

```text
DEFAULT_MAX_PENDING_BLOCK_REQUESTS is never used
DEFAULT_MAX_PENDING_BLOCK_REQUESTS_PER_PEER is never used
BlockRequestTracker::new is never used
BlockRequestTracker::with_limit is never used
BlockRequestTracker::with_max_pending is never used
BlockRequestTracker::take_backpressure_suppressed is never used
BlockRequestTracker::pending_capacity_remaining is never used
```

Apply the minimal targeted patch:

```rust
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS: usize = 128;
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS_PER_PEER: usize = 16;
```

And add the same attribute directly above only these helper methods:

```rust
#[cfg_attr(not(test), allow(dead_code))]
pub fn new(timeout_secs: u64, retry_limit: u8) -> Self { ... }

#[cfg_attr(not(test), allow(dead_code))]
pub fn with_limit(timeout_secs: u64, retry_limit: u8, max_pending: usize) -> Self { ... }

#[cfg_attr(not(test), allow(dead_code))]
pub fn with_max_pending(timeout_secs: u64, retry_limit: u8, max_pending: usize) -> Self { ... }

#[cfg_attr(not(test), allow(dead_code))]
pub fn take_backpressure_suppressed(&mut self) -> u64 { ... }

#[cfg_attr(not(test), allow(dead_code))]
pub fn pending_capacity_remaining(&self) -> usize { ... }
```

Validation:

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Expected PR diff:

- only a few added attributes;
- no deleted functions;
- no deleted tests;
- no scheduler behavior changes.

## PR B: recover v2.2.20 5N/2M convergence

Title:

```text
p2p/sync: reconcile peer accounting and orphan backlog recovery
```

Evidence symptoms from `5N/2M intermediate`:

```text
5 nodes healthy
2 miners active
accepted_blocks > 0
all nodes reach height 405
peer_count=0 on all nodes
orphan_count=391 on all nodes
pending_missing_parents=391 on all nodes
distinct final tips=2
orphan_reprocess_attempts=0
```

Evidence symptoms from `5N/4M stress observe`:

```text
peer_count=0 on all nodes
orphan_count=512 on all nodes
pending_missing_parents=512 on all nodes
distinct final tips=4
disconnect_reason_counts={}
last_error_by_peer={}
lifecycle shows connections/recovering but effective peer_count is zero
```

Required investigation points:

1. Why `/p2p/status.peer_count` reaches zero when lifecycle/connection counters still show established or recovering peers.
2. Why peers can enter `recovering` or cooldown without `disconnect_reason_counts` or `last_error_by_peer` being populated.
3. Why `orphan_reprocess_attempts` remains zero despite non-zero orphan and pending-missing-parent backlog.
4. Whether missing-parent backlog entries are actionable, stale, evictable, or waiting for parent fetch.

Expected implementation direction:

- align effective peer count with active connection/lifecycle state, or expose explicit reason why connected peers are not sync-eligible;
- record deterministic reason when a peer transitions to recovering/cooldown without last error;
- trigger orphan reprocess when a missing parent resolves, and during quiescence when backlog remains non-zero;
- classify saturated orphan backlog instead of leaving it as opaque `512`/`391` stuck state.

Validation sequence:

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Then run evidence:

```bash
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

Exit criteria:

- `5N/1M baseline` remains PASS.
- `5N/2M intermediate` reaches PASS, or fails with a deterministic new reason and non-zero recovery metrics.
- No regression to RPC starvation.
- No all-zero peer-count ambiguity without explicit reason.

## Guardrails

- No consensus-rule breaking changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- No public-testnet readiness claim.
- No v2.3.0 readiness claim.
