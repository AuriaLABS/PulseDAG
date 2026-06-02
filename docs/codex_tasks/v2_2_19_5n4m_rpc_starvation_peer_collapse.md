# Codex task: diagnose 5N/4M RPC starvation and peer collapse

Priority: P0 for 5N/4M stress evidence after PR #556.

## Evidence

A 5N/4M stress rehearsal on commit `203641e2bb05fa597fab77715d7a0795843043b1` still failed after PR #556.

Important difference from the previous 5N/4M failure: node `n3` became RPC-unavailable during the run. The harness repeatedly logged:

```text
curl: (28) Operation timed out after 10002 milliseconds with 0 bytes received
FAIL[RPC_UNAVAILABLE]: required endpoint failed: http://127.0.0.1:28547/status
```

Final endpoint capture for n3:

```json
{
  "ok": false,
  "error": "curl failed",
  "label": "n3:/status final",
  "url": "http://127.0.0.1:28547/status",
  "exit_code": 28
}
```

Final stress summary:

```text
result: FAIL
runtime duration: 1733s
node_count: 5
miner_count: 4
n1 healthy=1 ready=0
n2 healthy=1 ready=0
n3 healthy=0 ready=0 rpc unavailable
n4 healthy=1 ready=0
n5 healthy=1 ready=0
convergence after quiescence: FAIL
worst_lag_from_max_height post: 463
distinct_tips post: 4
total_orphan_count post: 1964
total_missing_parent_count post: 1964
```

Final representative state from available nodes:

```text
n1 height=702 orphan_count=381 pending_missing_parents=381
n2 height=348 orphan_count=475 pending_missing_parents=475
n3 unavailable
n4 height=653 orphan_count=608 pending_missing_parents=608
n5 height=811 orphan_count=500 pending_missing_parents=500
```

The run also recorded:

```text
restart_rejoin_status=NOT_EXECUTED
lag_improved_during_quiescence=false
```

5N/1M and 5N/2M regression gates passed on the same commit before this stress run, so this is a 4-miner stress failure, now including RPC starvation or event-loop blocking on at least one node.

## Required investigation

Do not weaken gates. Investigate why a node can become RPC-unresponsive under 5N/4M pressure and why recovery backlog grows instead of draining.

Focus areas:

1. RPC starvation / blocking
   - Ensure RPC handlers are not blocked by long-held locks in consensus/P2P/storage paths.
   - Audit shared state locks used by `/status`, `/readiness`, `/p2p/status`, `/sync/status`, `/sync/missing`, and `/orphans`.
   - Avoid holding global locks while doing expensive orphan scans, block fetch scheduling, RocksDB reads/writes, or network sends.

2. P2P/runtime backpressure
   - Bound inbound block processing queues.
   - Bound missing-parent request queues and per-peer inflight windows.
   - Add fair scheduling so one peer/miner/orphan storm cannot monopolize runtime work.
   - Ensure request timeout/retry/fallback continues after miners stop.

3. Historical block serving and request lifecycle
   - Verify `GetBlock` serves historical accepted blocks under load.
   - Clear pending/inflight requests on success, not-found, disconnect, and timeout.
   - Retry stale requests against alternate peers.

4. Orphan adoption and metrics
   - Parent arrival must trigger orphan reprocess attempts.
   - Add metrics for orphan oldest age, max age, reprocess attempts/success/failure, request retries/fallbacks, and RPC handler latency if feasible.

5. Harness evidence
   - Add enough diagnostics to distinguish node process death, RPC listener starvation, lock starvation, and P2P partition.
   - Capture process aliveness, listener socket availability if portable, and recent node log tail on RPC timeout.

## Acceptance

Required code checks:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/v2_2_19_preflight_check.sh
```

Regression gates must remain green:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=1800 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/baseline_5n_1m \
bash scripts/v2_2_19_private_5n_1m_rehearsal.sh

DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=2400 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/intermediate_5n_2m \
bash scripts/v2_2_19_private_5n_2m_rehearsal.sh
```

Stress gate:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=3000 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/stress_5n_4m \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh
```

For 5N/4M, success is preferred. If still failing, evidence must clearly classify whether the dominant cause is RPC starvation, peer collapse, lock contention, request timeout/fallback failure, or throughput saturation. A black-box `RPC_UNAVAILABLE` failure is not acceptable for v2.3.0 readiness.

## Guardrails

- No consensus-rule, supply, reward, PoW, or difficulty changes.
- No smart-contract runtime enablement.
- Miner remains external.
- Do not set `public_testnet_ready=true`.
- Do not weaken 5N/1M or 5N/2M mandatory gates.
- Do not lower 5N/4M thresholds just to pass.
- Do not copy Kaspa code verbatim; adapt concepts only.
