# Codex task: fix 5N/2M missing-parent drain

Priority: P0 before v2.3.0 start decision or public-testnet readiness.

## Evidence

A 5N/2M intermediate rehearsal on commit `9b6d22b5972037e379dacf3a3bdf8843253e7620` failed after quiescence:

```text
result: FAIL
node_count: 5
miner_count: 2
convergence after quiescence: FAIL
worst lag after quiescence from max height: 6
distinct final tips after quiescence: 2
missing parent backlog clear: FAIL
```

Final state:

```text
n1 height=404 tip=1b18df6cfaf40d3e366e91ab09702d4413c5ee3b86e5cc3b70f80adaec1a21f9 orphan_count=337 pending_missing_parents=337
n2 height=398 tip=ffad339c4c5c1abae33cb5d45f6b0ca57d83fc1c8345a22a25599b85e6784a98 orphan_count=343 pending_missing_parents=343
n3 height=398 tip=ffad339c4c5c1abae33cb5d45f6b0ca57d83fc1c8345a22a25599b85e6784a98 orphan_count=343 pending_missing_parents=343
n4 height=398 tip=ffad339c4c5c1abae33cb5d45f6b0ca57d83fc1c8345a22a25599b85e6784a98 orphan_count=343 pending_missing_parents=343
n5 height=398 tip=ffad339c4c5c1abae33cb5d45f6b0ca57d83fc1c8345a22a25599b85e6784a98 orphan_count=343 pending_missing_parents=343
```

Readiness metrics show requests are created but not drained:

```text
pending_block_requests ~= orphan_count
inflight_block_requests ~= orphan_count
missing_parent_requests_sent > 0
orphan_blocks_retried = 0
orphan_blocks_resolved = 0
inv_hashes_requested = 0
```

This means the 5N/1M baseline has passed previously, but 5N/2M exposes a real convergence/recovery failure: missing-parent requests remain inflight forever or are not answered/retried/resolved, and dependent orphans are not reprocessed.

## Required investigation

Inspect and fix the missing-parent recovery path across:

- `apps/pulsedagd/src/main.rs`
- `crates/pulsedag-p2p/src/messages.rs`
- `crates/pulsedag-p2p/src/lib.rs`
- `crates/pulsedag-core/src/orphans.rs`
- `crates/pulsedag-core/src/sync_pipeline.rs`
- readiness/sync RPC metrics

Focus areas:

1. `GetBlock` / `BlockData` handling for missing-parent hashes.
2. Whether nodes answer `GetBlock` for historical accepted blocks, not only current tip.
3. Whether `BlockData` clears `pending_block_requests` and `inflight_block_requests` on success and on not-found/timeout.
4. Whether failed/inflight requests are retried with backoff or moved to another peer.
5. Whether parent arrival triggers `adopt_ready_orphans_with_result` and increments `orphan_blocks_retried` / `orphan_blocks_resolved`.
6. Whether `missing_parent_entries=0` while `pending_missing_parents>0` is a metric/index bug or real orphan-index corruption.
7. Whether duplicate suppression is too strict and prevents fallback recovery.
8. Whether quiescence should keep the recovery loop alive long enough after miners stop.

## Required behavior

For 5N/2M after miners stop and quiescence completes:

- all five nodes converge to one final tip;
- worst height lag is 0;
- orphan_count is 0 or formally bounded/waived with evidence;
- pending_missing_parents is 0;
- pending/inflight block requests drain or are clearly classified;
- `orphan_blocks_retried` and/or `orphan_blocks_resolved` are non-zero when recovery happened;
- readiness is not degraded by missing-parent backlog.

## Acceptance

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/v2_2_19_preflight_check.sh
```

Then rerun staged gates:

```bash
DURATION_SECS=600 \
QUIESCENCE_WAIT_SECS=120 \
GLOBAL_DEADLINE_SECS=1800 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/baseline_5n_1m \
bash scripts/v2_2_19_private_5n_1m_rehearsal.sh

DURATION_SECS=600 \
QUIESCENCE_WAIT_SECS=120 \
GLOBAL_DEADLINE_SECS=2400 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/intermediate_5n_2m \
bash scripts/v2_2_19_private_5n_2m_rehearsal.sh
```

Do not proceed to 5N/4M stress until 5N/2M passes cleanly.

## Guardrails

- No supply, reward, PoW, difficulty, or consensus-rule changes.
- No smart-contract runtime enablement.
- Keep miner as external application.
- Do not set `public_testnet_ready=true`.
- Do not weaken the 5N/1M or 5N/2M mandatory gates.
- Do not convert this failure into PASS by changing the harness threshold.
- Do not copy Kaspa code verbatim; implement original PulseDAG-compatible recovery logic.
