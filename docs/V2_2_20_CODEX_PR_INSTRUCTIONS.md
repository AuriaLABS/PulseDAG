# v2.2.20 Codex PR Instructions

Status: READY_FOR_CODEX

This file is the source of truth for the next Codex work on `v2.2.20`.

## Current evidence

Latest known evidence commit: `85c3b521cb79`.

| Gate | Result | Meaning |
|---|---:|---|
| `5N/1M baseline` | PASS | Baseline is healthy. Keep this green. |
| `5N/2M intermediate` | FAIL, improved | Peers now remain visible, but final tips diverge and backlog remains. This is the next blocker. |
| `5N/4M stress observe` | OBSERVE_FAIL | Do not fix this first. Use it only after `5N/2M` is recovered. |

Latest `5N/2M` failure signature:

- node count: `5`
- miner count: `2`
- peer count: non-zero on all nodes
- heights: all nodes reach the same height
- final tips: `2` distinct tips after quiescence
- divergent node: `n2`
- orphan backlog: `66` per node
- pending missing parents: `66` per node
- `missing_parent_entries=0`
- `inv_hashes_requested=0`
- important signal: `peer_health:rate_limited`

Latest `5N/4M` stress signature:

- node count: `5`
- miner count: `4`
- peer count network non-zero: PASS
- final tips: `3` distinct tips after quiescence
- `n3` has RPC listener alive but RPC calls time out
- orphan backlog saturates to `512` on four nodes
- pending missing parents saturate to `512` on four nodes
- heavy `peer_health:rate_limited` counters

## PR 1 — recover 5N/2M backlog and final-tip convergence

Title:

```text
p2p/sync: reprocess stale orphan backlog after rate-limited parent recovery
```

### Goal

Make `5N/2M intermediate` pass again without weakening consensus or PoW semantics.

A successful run should end with:

- `distinct final tips after quiescence = 1`
- `orphan_count = 0` or a clearly classified non-actionable count
- `pending_missing_parents = 0` or a clearly classified non-actionable count
- `peer_count > 0` on the network
- `5N/1M baseline` still PASS

### Files to inspect first

Start with these areas. Do not make a broad rewrite.

```text
apps/pulsedagd/src/block_request.rs
apps/pulsedagd/src/main.rs
crates/pulsedag-p2p/src/**
crates/pulsedag-storage/src/**
crates/pulsedag-rpc/src/**
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

### Required investigation

Codex must answer these questions in the PR body:

1. Why can `pending_missing_parents > 0` while `missing_parent_entries = 0`?
2. Why does the orphan backlog stay at `66` after quiescence?
3. Why does `n2` retain a different final tip although height matches the other nodes?
4. Does `peer_health:rate_limited` suppress parent recovery long enough to prevent orphan reprocess?
5. Is an orphan reprocess triggered after parent fetch rate limiting clears?

### Acceptable changes

Allowed:

- Add bounded orphan reprocess scheduling after rate-limited parent recovery.
- Add deterministic stale/retryable/evictable classification for orphan backlog entries.
- Add metrics explaining why a pending parent is no longer requestable.
- Add bounded retry/backoff reset once peers are healthy again.
- Add tests around backlog reprocess and stale missing-parent classification.
- Improve evidence output if it helps explain remaining failures.

Not allowed:

- No consensus-rule breaking changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Do not move pool logic into the miner.
- Do not hide failures by changing the rehearsal pass/fail criteria.
- Do not declare public-testnet readiness.
- Do not bump to `v2.3.0`.

### Required validation

Run at minimum:

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

Expected:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS, or FAIL with a new deterministic reason that is better than the current ambiguous backlog stall

### PR body must include

```text
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
inv_hashes_requested:
rate_limited counters:
why this does not change consensus:
```

## PR 2 — document 5N/2M recovery evidence

Title:

```text
docs: record v2.2.20 5N/2M recovery evidence
```

Do this only after PR 1 has produced a new `5N/2M` run.

Update or create:

```text
docs/V2_2_20_5N_2M_INTERMEDIATE_EVIDENCE.md
```

The document must include:

- commit tested
- result
- runtime
- node count
- miner count
- final heights
- final tips
- peer counts
- orphan counts
- pending missing parents
- miner template/submit/accepted/rejected stats
- artifact checksum
- whether this supersedes the previous `85c3b521cb79` failure

## PR 3 — isolate RPC under 5N/4M stress

Title:

```text
rpc: isolate final capture endpoints from sync/orphan stress
```

Do this after `5N/2M` is recovered or clearly improved.

### Goal

In the latest `5N/4M` artifact, `n3` stayed process-alive and listener-present, but RPC calls timed out. This PR should make final evidence endpoints responsive under sync/orphan pressure.

Target endpoints:

```text
/status
/p2p/status
/readiness
/sync/status
/sync/missing
/orphans
```

### Acceptable changes

Allowed:

- Add bounded internal timeouts for final-capture endpoints.
- Return degraded/stale-but-fast responses instead of blocking indefinitely.
- Add metrics showing when an RPC response is degraded.
- Avoid locking RPC handlers behind long sync/orphan operations.

Not allowed:

- Do not mask node unhealthy state as healthy.
- Do not make `/readiness` optimistic.
- Do not change consensus.
- Do not change PoW.
- Do not change the rehearsal criteria to hide failures.

### Required validation

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
scripts/v2_2_20_private_5n_4m_rehearsal.sh
```

Expected:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS or no regression from the latest recovered state
- `5N/4M stress`: may still OBSERVE_FAIL, but final-capture RPC endpoints should not starve on a live listener

## PR 4 — document updated 5N/4M stress evidence

Title:

```text
docs: update v2.2.20 5N/4M stress evidence
```

Do this after PR 3.

Update:

```text
docs/V2_2_20_FIRST_STRESS_EVIDENCE.md
```

The document must compare:

- old 5N/4M result at `6633962c07bb`
- new 5N/4M result at `85c3b521cb79`
- post-fix result

Track:

- final tips
- RPC timeout behavior
- peer counts
- orphan counts
- pending missing parents
- `peer_health:rate_limited`
- accepted/rejected blocks
- whether the stress result is PASS, OBSERVE_FAIL_BOUNDED, or OBSERVE_FAIL_UNBOUNDED

## Recommended order

```text
1. PR 1: recover 5N/2M backlog and convergence
2. PR 2: document 5N/2M recovery evidence
3. PR 3: isolate RPC final-capture endpoints under 5N/4M stress
4. PR 4: document updated 5N/4M stress evidence
```

## Global guardrails

Every PR must preserve:

- no consensus-rule breaking changes
- no PoW semantic changes
- no smart contracts
- no pool logic
- miner remains external
- `public_testnet_ready=false`
- no `v2.3.0` readiness claim
- no `v3.0` readiness claim
