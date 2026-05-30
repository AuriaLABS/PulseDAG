# Codex task: v2.2.19 private 5N/4M rehearsal hardening

## Context

Post-merge v2.2.19 evidence:

- 3N/1M local smoke on `main` passes.
- 5N/4M private rehearsal can hang before miners because private rehearsal curls do not have per-request deadlines.
- With an external curl timeout wrapper, 5N/4M completes but fails convergence/readiness with sync degraded and missing-parent/orphan pressure.

This PR is intentionally scoped to the rehearsal harness only. It must not change consensus rules.

## Goal

Make `scripts/v2_2_19_private_5n_4m_rehearsal.sh` deterministic, time-bounded, and useful as evidence even when the network fails under stress.

## Required changes

1. Add per-request curl deadlines to the private rehearsal helper:
   - `--connect-timeout ${PULSEDAG_CURL_CONNECT_TIMEOUT:-2}`
   - `--max-time ${PULSEDAG_CURL_MAX_TIME:-5}`
   - preserve `-fsS` behavior.

2. Add a global deadline:
   - env: `SMOKE_TOTAL_DEADLINE_SECS`, default no more than 1800 seconds.
   - all peer/readiness/miner/convergence loops must exit when the deadline expires.
   - on deadline, package evidence and fail with a clear diagnostic, not hang.

3. Make cleanup idempotent:
   - avoid repeated cleanup on `EXIT INT TERM`.
   - clear traps inside cleanup.
   - always try to package evidence.

4. Add explicit phase markers to `command-log.txt`:
   - preflight complete
   - binaries built
   - nodes launched
   - pre-mining RPC gate
   - pre-mining P2P gate
   - miners launched
   - mining window start/end
   - miners stopped
   - quiescence start/end
   - final evidence collection

5. Add a quiescence phase after mining:
   - stop miners first;
   - wait `QUIESCENCE_SECS` default 90 seconds;
   - sample status/readiness/sync again;
   - evaluate final convergence after quiescence, not while miners are still racing.

6. Keep `public_testnet_ready=false`. Do not change readiness semantics to force public readiness.

## Acceptance criteria

- Running with external timeout should no longer be required.
- A bad run must end with an evidence tarball and a structured FAIL, never a hang.
- Existing 3N/1M smoke remains PASS.
- 5N/4M may still FAIL due to real sync issues, but it must finish and explain why.

## Guardrails

- No smart-contract changes.
- No miner-in-node logic.
- No consensus-rule changes in this PR.
- Do not copy Kaspa code verbatim. Kaspa can be used as architectural inspiration only; respect licensing and adapt concepts.