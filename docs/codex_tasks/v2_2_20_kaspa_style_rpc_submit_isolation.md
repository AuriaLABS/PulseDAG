# Codex task: Kaspa-style RPC submit isolation for v2.2.20

Date: 2026-06-07

## Current PulseDAG state

`v2.2.20` has already merged Kaspa-style orphan-root recovery in PR #600.

Post-#600 evidence shows:

- `5N/1M baseline`: PASS.
- `5N/2M intermediate`: orphan/missing-parent backlog is clean, but the node serving the second miner can become RPC-unresponsive.
- `5N/4M stress`: one miner-facing node can become RPC-unresponsive, while other nodes can still saturate orphan/missing-parent backlog under heavy rate limiting.

The next blocking issue is not the original orphan-root recovery problem. It is RPC/mining submit starvation.

Observed symptoms:

- process is alive;
- RPC listener is present;
- `/status`, `/p2p/status`, `/readiness`, `/sync/status`, and `/orphans` time out;
- miner receives a template and submits work, but the submit response does not complete;
- socket `Recv-Q` grows on the miner-facing RPC port.

## Kaspa pattern to copy

Do **not** copy Kaspa consensus or GHOSTDAG logic.

Copy the RPC/mining control-flow pattern:

1. `submit_block_call` performs cheap preflight/rejection checks before heavy block processing.
2. Unstable IBD/sync state is rejected before attempting block submission.
3. Block parsing and obvious stale/invalid checks happen before consensus insertion.
4. The heavy block insertion is delegated into the protocol flow context.
5. The flow validates/inserts the block, broadcasts after insertion, then runs post-processing separately.
6. New-block post-processing handles unorphaning, mempool updates, and rebroadcast, but this must not starve health/status RPCs.

The architectural goal for PulseDAG is: **mining submit must be bounded, observable, and must not monopolize the same locks/executor path used by health/readiness/status endpoints.**

## PulseDAG files to inspect first

Primary files:

- `crates/pulsedag-rpc/src/handlers/mining_submit.rs`
- `crates/pulsedag-rpc/src/handlers/runtime.rs`
- `crates/pulsedag-rpc/src/api.rs`
- `apps/pulsedagd/src/main.rs`

Likely related core/storage paths:

- `crates/pulsedag-core/src/state.rs`
- `crates/pulsedag-core/src/accept.rs` or equivalent block acceptance module
- storage persistence code used by `persist_block_and_chain_state`

## Problem in current PulseDAG submit path

`post_mining_submit` currently takes the chain write lock at the top of the handler and keeps it across many operations:

- PoW validation;
- duplicate checks;
- template lookup and freshness checks;
- multiple stale-template checks;
- expected difficulty calculation;
- `accept_block_atomically`;
- storage persistence through callback;
- P2P broadcast through callback;
- orphan adoption;
- runtime/storage event writes;
- response construction.

This means a miner-facing submit can block other endpoints if they need the same state or executor path.

The failure evidence shows exactly this starvation mode: process alive, listener alive, RPC endpoints time out.

## Required implementation approach

### PR title

`rpc/mining: isolate submit_block from status/readiness RPC paths`

### Required behavior

1. **Do cheap rejects before taking the chain write lock whenever possible.**

   Examples:

   - malformed request / missing `template_id`;
   - basic PoW hash calculation;
   - block hash extraction;
   - obvious duplicate fast path if a read lock or nonblocking snapshot can be used;
   - template lookup from template store;
   - hard template expiry if it does not require mutable chain state.

2. **Keep the chain write lock critical section short.**

   The critical section should cover only:

   - current chain snapshot needed for final template validation;
   - acceptance mutation;
   - minimal in-memory state update.

   It must not include long P2P broadcast, long runtime event append, or slow storage if those can be moved out safely.

3. **Separate accept/persist/broadcast/post-process phases.**

   Target shape:

   ```text
   submit_start
   cheap_preflight_without_chain_write
   acquire_chain_write
   validate_again_against_current_state
   accept_mutation
   capture accepted block + follow-up work
   release_chain_write
   persist / broadcast / runtime_event / follow-up adoption bounded
   response_sent
   ```

   If atomic accept+persist currently requires holding the chain lock, keep that atomicity, but still move non-required broadcast/event work outside the critical section.

4. **Add bounded submit timeout / guardrails.**

   Add internal timing metrics and optionally a timeout around the expensive part. If timeout is used, it must return a deterministic `submit_timeout` rejection or degraded response, not hang the RPC worker indefinitely.

5. **Make status/readiness independent of submit progress.**

   `/status`, `/p2p/status`, `/readiness`, `/sync/status`, `/sync/missing`, and `/orphans` must return quickly while a mining submit is running.

   If they cannot get a fresh chain read immediately, they should return the most recent cached/snapshot state with a degraded/stale marker rather than block until the submit completes.

6. **Add metrics and runtime fields.**

   Add enough fields to diagnose stuck submits:

   - `external_mining_submit_inflight`
   - `external_mining_submit_started_total`
   - `external_mining_submit_completed_total`
   - `external_mining_submit_timeout_total`
   - `external_mining_last_submit_started_at_unix`
   - `external_mining_last_submit_completed_at_unix`
   - `external_mining_last_submit_duration_ms`
   - `external_mining_last_submit_phase`
   - `external_mining_max_submit_duration_ms`

   Suggested phases:

   - `preflight`
   - `chain_lock_wait`
   - `template_validation`
   - `accept_block`
   - `persist`
   - `broadcast`
   - `orphan_adoption`
   - `response_sent`
   - `timeout`
   - `error`

7. **Add tests.**

   Minimum tests:

   - submit rejection paths do not require long chain write lock;
   - accepted submit increments start/completion counters;
   - simulated slow broadcast does not keep chain write lock held;
   - status/readiness snapshot path returns while submit is in progress;
   - timeout/degraded path is deterministic if a submit phase stalls.

## Explicit non-goals

Do not change:

- consensus rules;
- PoW semantics;
- block validity rules;
- smart-contract state;
- miner architecture;
- pool logic;
- public-testnet readiness.

The miner must remain an external application.

## Validation commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Then run evidence:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=2700 bash scripts/v2_2_20_private_5n_1m_rehearsal.sh
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=2700 bash scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

Only after `5N/2M` is stable, rerun:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=2700 bash scripts/v2_2_20_private_5n_4m_rehearsal.sh
```

## Acceptance criteria

### Mandatory

- `5N/1M baseline` remains PASS.
- `5N/2M intermediate` keeps all 5 nodes RPC-responsive.
- No node ends with `chain_id=unknown`.
- No miner-facing node shows `RPC_ALIVE_LISTENER_TIMEOUT`.
- `accepted_blocks > 0`.
- `orphan_count=0` and `pending_missing_parents=0` in `5N/2M`.
- `distinct final tips after quiescence=1` in `5N/2M`.

### Diagnostic improvement for 5N/4M

If `5N/4M` still fails, it must fail better:

- no permanently unresponsive RPC node;
- all submit attempts either complete, reject, or timeout deterministically;
- evidence includes submit phase/duration counters;
- orphan backlog must be classified as retryable/stale/evictable/rate-limited instead of opaque saturation.

## Evidence note

The next artifact must compare against post-#600 evidence:

- `5N/1M post-#600`: PASS.
- `5N/2M post-#600`: backlog clean but miner-facing n2 RPC starvation.
- `5N/4M post-#600`: n3 RPC starvation plus 512 backlog saturation on remaining nodes.

This PR should primarily eliminate the RPC starvation class. A follow-up PR can focus on 5N/4M rate-limit-aware orphan-root scheduling if stress backlog remains after RPC isolation.
