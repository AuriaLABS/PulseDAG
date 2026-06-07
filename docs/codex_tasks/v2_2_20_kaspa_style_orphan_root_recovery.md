# Codex task: Kaspa-style orphan-root recovery for v2.2.20

Status: READY_FOR_CODEX

## Context

Current `v2.2.20` evidence shows:

- `5N/1M baseline`: PASS.
- `5N/2M intermediate`: FAIL.
- `5N/4M stress observe`: OBSERVE_FAIL.

The latest `5N/2M` regression at commit `b6950201cd24` shows:

- `n3` RPC/status unavailable with `chain_id=unknown`.
- backlog grows to hundreds of orphans/missing parents.
- `pending_missing_parents > 0` while `missing_parent_entries=0` and `inv_hashes_requested=0`.
- rate limiting counters become extremely high.

The previous better `5N/2M` run at commit `85c3b521cb79` showed:

- peer visibility recovered;
- backlog reduced to `66` per node;
- only one node diverged by final tip;
- the remaining issue was stale orphan/missing-parent recovery.

## Final intended solution

Implement a bounded **Kaspa-style orphan-root recovery path**.

Do **not** copy consensus rules or GHOSTDAG semantics. Copy only the networking/sync pattern:

1. Keep an orphan pool with indexed missing roots.
2. When a block is orphaned, compute its missing root ancestors by walking through known orphan ancestors and consensus-known blocks.
3. Request missing roots explicitly from peers.
4. When a root arrives, process the root and then process descendants that become ready.
5. Periodically revalidate the orphan pool after recovery/IBD/quiescence so stale entries become retryable or evictable.
6. If backlog exists but no missing roots are indexed, rebuild the root index from actual block parents instead of leaving the node stuck.

## Kaspa reference pattern

Kaspa's Rust implementation has the relevant pattern in:

- `protocol/flows/src/flowcontext/orphans.rs`
- `protocol/flows/src/v7/blockrelay/flow.rs`

Important concepts to mirror, adapted to PulseDAG names:

- `OrphanBlocksPool` stores orphans and child links.
- `get_orphan_roots` walks orphan ancestors and consensus state to find roots that are not known or are header-only.
- `unorphan_blocks` processes descendants once roots are available.
- `revalidate_orphans` cleans already processed blocks and retries roots that became processable.
- block relay enqueues orphan roots as indirect inventory and requests those roots before retrying the original orphan.

Do not vendor or paste large Kaspa files. Implement an equivalent small PulseDAG-native version using PulseDAG's existing `ChainState`, `orphan_blocks`, `orphan_missing_parents`, `orphan_parent_index`, and `accept_block_with_result`.

## PulseDAG files to inspect first

```text
crates/pulsedag-core/src/orphans.rs
crates/pulsedag-core/src/state.rs
apps/pulsedagd/src/main.rs
apps/pulsedagd/src/block_request.rs
crates/pulsedag-p2p/src/**
crates/pulsedag-rpc/src/handlers/sync.rs
crates/pulsedag-rpc/src/handlers/orphans.rs
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

## Required implementation

### 1. Add orphan root discovery

Add a PulseDAG-native function, likely in `crates/pulsedag-core/src/orphans.rs`:

```rust
pub fn orphan_missing_roots(state: &ChainState, orphan_hash: &Hash) -> Vec<Hash>
```

Behavior:

- Start from the orphan's direct parents.
- Walk through `state.orphan_blocks` if a parent is also an orphan.
- If a parent is neither in `state.dag.blocks` nor in `state.orphan_blocks`, it is a missing root.
- Deduplicate and sort roots deterministically.

Also add:

```rust
pub fn rebuild_orphan_parent_index(state: &mut ChainState) -> OrphanBacklogClassification
```

Behavior:

- Clear and rebuild `orphan_parent_index` and `orphan_missing_parents` from `orphan_blocks` and actual missing parents.
- Return classification.
- This is the fix for the bad state where `pending_missing_parents > 0` but no actionable entries remain.

### 2. Add root request scheduling

In `apps/pulsedagd/src/main.rs`, recovery tick currently gets missing parents from `orphan_parent_index`.

Extend this logic:

- If `orphan_parent_index` is empty but `orphan_blocks` is non-empty, call `rebuild_orphan_parent_index`.
- Compute missing roots for a bounded number of oldest/highest-priority orphans.
- Enqueue/request those roots through the existing `BlockRequestTracker` and P2P `request_block` path.
- Do not request unlimited roots; bound per tick, for example `16` roots.
- Deduplicate against known blocks and pending block requests.
- Reset rate-limit backoff only when there are active peers.

### 3. Unorphan descendants after root arrival

Whenever a requested parent/root block is accepted:

- resolve it in `BlockRequestTracker`;
- call `adopt_ready_orphans_with_result(state, AcceptSource::P2p, Some(&accepted_hash))` first;
- then, if backlog remains, run a bounded whole-pool revalidation pass similar to Kaspa's `revalidate_orphans`.

Important: avoid holding long write locks while doing P2P/RPC work.

### 4. Make stale backlog explicit

Update runtime metrics so `/sync/status` and evidence can distinguish:

- `retryable_ready`
- `waiting_missing_parent`
- `stale_missing_parent_entries`
- `unindexed_missing_parent_entries`
- `root_requests_sent`
- `root_requests_suppressed_rate_limited`
- `root_requests_suppressed_no_peers`
- `orphan_index_rebuilds`

Do not hide failures. If backlog is non-zero, report why.

### 5. Tests required

Add tests in `crates/pulsedag-core/src/orphans.rs` or nearby:

1. Chain: child arrives before parent.
   - `orphan_missing_roots` returns parent.
   - after parent accepted, adoption clears child.

2. Multi-level orphan chain.
   - orphan `D` depends on orphan `C`, and `C` depends on missing root `B`.
   - root discovery returns `B`, not only direct parent `C`.

3. Stale index rebuild.
   - clear `orphan_parent_index` while orphans remain.
   - `rebuild_orphan_parent_index` reconstructs entries.

4. Already-known parent.
   - if parent exists in `state.dag.blocks`, root discovery does not request it.

5. Deterministic ordering.
   - roots returned sorted/deduplicated.

## Validation gates

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Then run:

```bash
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

Required:

- `5N/1M baseline`: PASS.
- `5N/2M intermediate`: PASS preferred.
- If not PASS, it must not regress below the previous best state at `85c3b521cb79`:
  - all nodes RPC responsive;
  - no `chain_id=unknown`;
  - peer count non-zero;
  - backlog not worse than `66` per node unless a deterministic reason is reported.

## PR body must include

```text
Kaspa pattern used:
Files changed:
Why this is not a consensus-rule change:
5N/1M result:
5N/2M result:
commit tested:
evidence.tar.gz sha256:
final heights:
final tips:
peer counts:
orphan counts:
pending missing parents:
missing_parent_entries:
root_requests_sent:
orphan_index_rebuilds:
rate_limited counters:
known remaining limitation:
```

## Guardrails

- No consensus-rule breaking changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Do not move pool logic into the miner.
- Miner remains external.
- Do not modify rehearsal pass/fail criteria to hide the bug.
- Do not claim public testnet readiness.
- Do not bump to `v2.3.0`.

## Why this should solve the current issue

PulseDAG already has orphan storage and adoption, but evidence shows the system can enter a bad state where orphan backlog remains while no actionable missing-parent entries or inventory requests remain. Kaspa avoids that class of stall by treating missing ancestors as explicit roots, requesting roots, and revalidating/unorphaning descendants when roots are obtained.

The goal is to make `pending_missing_parents > 0` always actionable, classified, or evicted. It must never remain an opaque stuck counter.
