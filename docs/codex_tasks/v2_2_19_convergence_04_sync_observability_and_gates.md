# Codex task: v2.2.19 sync observability and staged convergence gates

## Context

Current v2.2.19 status after PR #532:

- 3N/1M post-merge PASS.
- 5N/4M private rehearsal FAILS with divergent tips, sync degraded, orphan/missing-parent pressure.
- The network can mine and propagate some blocks, but does not reliably converge under 4-miner stress.

This PR is for observability and staged gates. It should make failures easier to classify and avoid treating 5N/4M stress as the only readiness signal.

## Goal

Add staged convergence gates and rich evidence so Codex and operators can distinguish:

- baseline network health;
- single-miner convergence;
- two-miner moderate fork pressure;
- four-miner stress behavior;
- orphan recovery failures;
- script/harness failures.

## Required changes

1. Add staged rehearsal scripts or modes:
   - `5N/1M baseline` mandatory readiness gate;
   - `5N/2M intermediate` mandatory or warning depending on results;
   - `5N/4M stress` diagnostic/stress gate until orphan recovery is fixed.

2. Add quiescence evaluation to every staged run:
   - stop miners;
   - wait configurable `QUIESCENCE_SECS`;
   - resample height/tip/readiness/sync/orphans;
   - evaluate convergence after quiescence.

3. Expand evidence summary:
   - per-node final height/tip;
   - per-node peer count;
   - per-node orphan count;
   - per-node missing parent count;
   - per-node sync status;
   - per-miner templates/submits/accepted/rejected;
   - convergence before quiescence;
   - convergence after quiescence;
   - worst lag from max height;
   - number of distinct final tips;
   - whether lag improved during quiescence.

4. Add failure classification:
   - `HARNESS_TIMEOUT`;
   - `RPC_UNAVAILABLE`;
   - `P2P_NOT_CONNECTED`;
   - `MINER_NO_TEMPLATE`;
   - `MINER_NO_ACCEPTED_BLOCKS`;
   - `SYNC_DIVERGED`;
   - `MISSING_PARENT_BACKLOG`;
   - `READINESS_SCHEMA_MISMATCH`;
   - `CLEANUP_HANG`.

5. Add docs:
   - document that 3N/1M PASS is not enough for public readiness;
   - document current known limitation: 5N/4M stress divergence until recovery PRs land;
   - preserve `public_testnet_ready=false`.

## Acceptance criteria

- 3N/1M still PASS.
- 5N/1M should PASS or produce actionable evidence.
- 5N/4M should not block forever and should classify failures accurately.
- Evidence tarballs must be generated on both PASS and FAIL.
- No false public-testnet readiness claims.

## Guardrails

- No consensus-rule changes in this PR.
- No smart contracts.
- External miner architecture preserved.
- Do not copy Kaspa code verbatim.