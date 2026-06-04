# Codex task: fix RPC busy diagnostics and P2P status pressure

Priority: P0 after hard-stop watchdog.

## Evidence

A WSL-safe 5N/4M stress run on commit `cd27e0ee7b61c1d5fe1b5617c307394296b5cd0c` completed and packaged evidence, but failed with node `n3` becoming unresponsive to RPC status calls.

Key result:

```text
result: FAIL
stage: 5N/4M stress
runtime: 659s
commit: cd27e0ee7b61
n3: height=0, chain_id=unknown, repeated curl timeout on /status, /p2p/status, /sync/status, /sync/missing, /orphans
```

However, diagnostics misclassified n3 as exited:

```text
RPC_DIAGNOSTIC[RPC_PROCESS_EXITED]: label=n3:/status node=n3 pid=unknown alive=0 listener=1 curl_exit=28
```

The process actually existed and the listener was present:

```text
process-pids.txt: 1217 node-n3
ss: LISTEN 34/128 127.0.0.1:28547 users: pulsedagd pid=1217
```

The ready status file for n3 also showed:

```json
{
  "ok": false,
  "error": {
    "code": "P2P_STATUS_BUSY",
    "message": "/status could not complete p2p status snapshot within 750ms; p2p shared state is busy and peer-collapse diagnostics should inspect long-running p2p critical sections"
  }
}
```

Final stress state:

```text
n1 height=279 orphan_count=476 pending_missing_parents=476 peer_count=0
n2 height=280 orphan_count=475 pending_missing_parents=475 peer_count=0
n3 RPC busy/unavailable, height=0
n4 height=275 orphan_count=483 pending_missing_parents=482 peer_count=0
n5 height=279 orphan_count=461 pending_missing_parents=461 peer_count=0
convergence after quiescence: FAIL
distinct final tips: 4
post total_orphan_count: 1895
post total_missing_parent_count: 1894
peer count network non-zero: FAIL
```

## Required fixes

### 1. Fix harness PID lookup/misclassification

`capture_rpc_failure_diagnostics` must correctly resolve node labels from `process-pids.txt`.

Current evidence suggests `node_pid_for_label` compares against the wrong token. `process-pids.txt` uses entries like:

```text
1217 node-n3
```

but the diagnostic produced `pid=unknown` for label `n3:/status`.

Fix so:

- label `n3:/status` maps to `node-n3` in `process-pids.txt`;
- if process is alive and listener is present but curl times out, classify as `RPC_ALIVE_LISTENER_TIMEOUT`, not `RPC_PROCESS_EXITED`;
- if process is alive but no listener, classify `RPC_LISTENER_DOWN`;
- only classify `RPC_PROCESS_EXITED` if the PID is known or resolvable and not alive, or no listener exists and no process can be found;
- include process command/ps output in diagnostics.

### 2. Fix /status dependency on busy P2P status

`/status` currently can fail with `P2P_STATUS_BUSY`. A basic status endpoint should remain responsive under P2P stress.

Required behavior:

- `/status` must not block on a long P2P shared-state snapshot;
- if P2P snapshot is busy, return core chain status plus a degraded P2P status marker instead of failing the whole endpoint;
- reserve heavy P2P details for `/p2p/status`;
- add metrics for p2p snapshot busy/timeout counts.

### 3. Reduce P2P status lock pressure

Investigate why `/p2p/status` and peer count collapse to zero under 5N/4M pressure.

Focus:

- long-held P2P shared-state locks;
- expensive cloning/scanning while holding locks;
- status handlers competing with block propagation/fetch scheduling;
- peer map being reported as zero because status snapshot times out or stale fallback is unavailable.

Implement safe fixes:

- keep last-known cheap P2P snapshot and return it with `stale=true` if live lock is busy;
- avoid holding locks while formatting large peer/block diagnostics;
- make status endpoints bounded and non-blocking.

### 4. Keep recovery metrics actionable

The same run showed `missing_parent_entries=0` while `pending_missing_parents>0`. Either explain and document the distinction or fix the metric/index mismatch.

## Acceptance

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/v2_2_19_preflight_check.sh
```

Then regression gates:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=1800 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/baseline_5n_1m \
bash scripts/v2_2_19_private_5n_1m_rehearsal.sh

DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=2400 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/intermediate_5n_2m \
bash scripts/v2_2_19_private_5n_2m_rehearsal.sh
```

Then WSL-safe stress:

```bash
DURATION_SECS=300 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=1200 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/stress_5n_4m_wsl_safe \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh
```

Expected improvement:

- no false `RPC_PROCESS_EXITED` when PID/listener exist;
- `/status` remains responsive even if P2P status is busy;
- if 5N/4M still fails, failure is classified as peer collapse/backpressure/missing-parent backlog, not black-box RPC timeout;
- 5N/1M and 5N/2M remain PASS.

## Guardrails

- No consensus-rule, supply, reward, PoW, or difficulty changes.
- No smart-contract runtime enablement.
- Miner remains external.
- Do not set `public_testnet_ready=true`.
- Do not weaken 5N/1M or 5N/2M mandatory gates.
- Do not lower 5N/4M thresholds just to pass.
