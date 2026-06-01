# Codex task: fix private rehearsal miner metrics collector

Priority: P0 before rerunning 5N/1M, 5N/2M, or 5N/4M staged convergence gates.

## Evidence

A 5N/1M baseline rehearsal on commit `100bca3bf95ed35afb97bab7e8d4b89ccc61513e` failed with exit code `127` during quiescence/final-state collection:

```text
scripts/v2_2_19_private_5n_4m_rehearsal.sh: line 268: collect_miner_metrics: command not found
FAIL[HARNESS_TIMEOUT]: script exited non-zero before classified failure: 127
```

The run launched 5 nodes and 1 miner, all nodes had peers, no orphan/missing-parent backlog remained, and most nodes converged, but the evidence cannot be accepted because the harness crashed before completing quiescence and final gate classification.

## Root cause

`scripts/v2_2_19_private_5n_4m_rehearsal.sh` calls `collect_miner_metrics` inside `collect_final_state`, but no such function is defined in the script.

Current nearby helpers include:

- `text_has_match`
- `count_matches_file`
- `count_matches_in_logs`

Current miner arrays/counters include:

- `miner_template[$i]`
- `miner_submit[$i]`
- `miner_accept[$i]`
- `miner_reject[$i]`
- `ACCEPTED_BLOCKS`
- `REJECTED_BLOCKS`
- `TEMPLATES_OK`

## Required fix

Update `scripts/v2_2_19_private_5n_4m_rehearsal.sh` only, unless a minimal shell test already exists and is appropriate.

Add a portable, non-fatal `collect_miner_metrics()` helper before `collect_final_state()`.

Suggested implementation shape:

```bash
collect_miner_metrics(){
  local i
  for i in $(seq 1 "$MINER_COUNT"); do
    text_has_match "template" "$OUT_DIR/logs/miner-${i}.log" && miner_template[$i]=1 || true
    text_has_match "submit" "$OUT_DIR/logs/miner-${i}.log" && miner_submit[$i]=1 || true
    text_has_match "accepted" "$OUT_DIR/logs/miner-${i}.log" && miner_accept[$i]=1 || true
    text_has_match "reject" "$OUT_DIR/logs/miner-${i}.log" && miner_reject[$i]=1 || true
  done
  ACCEPTED_BLOCKS=$(count_matches_in_logs "accepted")
  REJECTED_BLOCKS=$(count_matches_in_logs "reject")
  (( ACCEPTED_BLOCKS > 0 )) && TEMPLATES_OK=1 || true
}
```

Then replace the duplicated inline miner-log scanning inside the sampling loop with a call to `collect_miner_metrics`, so final collection and live sampling use the same logic.

## Acceptance

Required:

```bash
bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=1 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=2 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=4 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
bash scripts/v2_2_19_preflight_check.sh
```

Recommended after merge:

```bash
DURATION_SECS=600 \
QUIESCENCE_WAIT_SECS=120 \
GLOBAL_DEADLINE_SECS=1800 \
OUT_DIR=artifacts/v2_2_19/staged_convergence_gates/baseline_5n_1m \
bash scripts/v2_2_19_private_5n_1m_rehearsal.sh
```

## Guardrails

- No consensus changes.
- No P2P protocol changes.
- No mining backend changes.
- No version bump.
- Do not set `public_testnet_ready=true`.
- Do not weaken the mandatory 5N/1M or 5N/2M readiness gates.
- Do not convert real convergence failure into PASS. This PR only fixes harness classification so the next run can prove the real status.
