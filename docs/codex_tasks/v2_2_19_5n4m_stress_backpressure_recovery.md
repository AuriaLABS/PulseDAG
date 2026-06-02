# Codex task: investigate 5N/4M stress backpressure and recovery

Priority: P1 after 5N/1M and 5N/2M have passed.

## Evidence

A 5N/4M stress rehearsal on commit `afc95b6c18f7ffae8d30ee403a5a2bc5ed321d0e` failed after quiescence.

Summary:

```text
result: FAIL
node_count: 5
miner_count: 4
convergence after quiescence: FAIL
worst lag after quiescence from max height: 184
distinct final tips after quiescence: 5
total_orphan_count after quiescence: 2531
total_missing_parent_count after quiescence: 2528
```

Final representative state:

```text
n1 height=394 orphan_count=608 pending_missing_parents=608
n2 height=211 orphan_count=644 pending_missing_parents=644
n3 height=309 orphan_count=644 pending_missing_parents=644
n4 height=289 orphan_count=644 pending_missing_parents=644
n5 height=393 orphan_count=257 pending_missing_parents=257
```

The independent 5N/1M and 5N/2M gates passed before this stress run. Therefore this is not a baseline failure; it is 4-miner stress/backpressure/recovery behavior.

## Investigation focus

Do not weaken the 5N/4M stress gate by changing thresholds. Instead, investigate why recovery does not drain under four concurrent external miners.

Inspect:

- missing-parent request scheduling under high orphan volume;
- duplicate suppression that may keep requests inflight forever;
- request timeout/backoff/fallback to alternate peers;
- whether `GetBlock` responses are served for historical accepted blocks under load;
- whether `BlockData` not-found/timeout paths clear pending and inflight state;
- whether the fetch scheduler has bounded concurrency and fair peer selection;
- whether full-block broadcast plus inventory/header fetch creates self-induced request storms;
- whether quiescence recovery should keep actively retrying after miners stop;
- whether accepted block rate from 4 miners exceeds current private-profile recovery throughput.

## Required evidence improvements

Add or repair metrics so the next 5N/4M failure is actionable:

- missing_parent_requests_sent by node and peer;
- missing_parent_responses_received;
- missing_parent_request_timeouts;
- missing_parent_request_retries;
- missing_parent_request_fallbacks;
- blockdata_not_found;
- fetch_scheduler_queue_depth;
- fetch_scheduler_inflight_by_peer;
- orphan_reprocess_attempts;
- orphan_reprocess_successes;
- orphan_reprocess_failures_by_reason;
- max orphan age and oldest missing parent age.

## Possible fixes

Implement only fixes supported by the code/evidence:

1. bounded retry loop for stale inflight missing-parent requests;
2. peer fallback when source peer does not answer;
3. request-window limits so one peer/orphan storm does not monopolize fetches;
4. periodic recovery tick during quiescence and normal runtime;
5. historical block serving improvements for `GetBlock`;
6. inventory/header dependency ordering fixes if parents are known but not fetched.

## Acceptance

Required:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/v2_2_19_preflight_check.sh
```

Regression gates:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=1800 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/baseline_5n_1m \
bash scripts/v2_2_19_private_5n_1m_rehearsal.sh

DURATION_SECS=600 QUIESCENCE_WAIT_SECS=120 GLOBAL_DEADLINE_SECS=2400 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/intermediate_5n_2m \
bash scripts/v2_2_19_private_5n_2m_rehearsal.sh
```

Stress evidence:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=3000 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/stress_5n_4m \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh
```

5N/4M may remain a stress/diagnostic gate for v2.2.19, but it must no longer be a black box. If it still fails, evidence must clearly show whether the limit is request timeout, peer fallback, orphan adoption, historical block serving, or throughput saturation.

## Guardrails

- No consensus-rule, supply, reward, PoW, or difficulty changes.
- No smart-contract runtime enablement.
- Miner remains external.
- Do not set `public_testnet_ready=true`.
- Do not weaken 5N/1M or 5N/2M gates.
- Do not convert 5N/4M stress failure into PASS by lowering criteria.
- Do not copy Kaspa code verbatim; adapt concepts only.
