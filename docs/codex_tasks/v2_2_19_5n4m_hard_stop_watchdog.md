# Codex task: make 5N/4M stress hard-stop and evidence-safe

Priority: P0 before any further v2.3.0 readiness decision.

## Problem

The short packaging probe now produces evidence, but the real 5N/4M stress can still run indefinitely or fail to finish in a useful way on WSL/Ubuntu. This means the rehearsal is still not trustworthy for long stress runs.

Current known state:

- 3N/1M PASS.
- 5N/1M PASS.
- 5N/2M PASS.
- Harness packaging probe PASS.
- Real 5N/4M stress still hangs or does not complete reliably.

## Goal

Make `scripts/v2_2_19_private_5n_4m_rehearsal.sh` impossible to hang indefinitely. It must always hard-stop, collect whatever evidence exists, package it, and exit with a classified result.

## Required behavior

1. Add a hard wall-clock supervisor independent from normal cleanup.
2. On `GLOBAL_DEADLINE_SECS`, force finalization even if the main flow is blocked.
3. Bound final endpoint collection with a small cleanup budget, for example `FINAL_CAPTURE_BUDGET_SECS`.
4. During cleanup, use shorter curl timeouts than the normal run.
5. If final endpoint collection exceeds budget, skip remaining endpoints and package existing data.
6. Kill miners first, then nodes, with SIGTERM then SIGKILL fallback.
7. Ensure process cleanup cannot leave `pulsedagd` or `pulsedag-miner` alive on the test ports.
8. Add a stall classifier, for example `HARNESS_STALL_TIMEOUT`, when no progress is observed before hard stop.
9. Always print `RUN_DIR`, `FINAL_EXIT_CODE`, and `FINAL_RESULT`.
10. Always create root and run-dir evidence artifacts if `RUN_DIR` exists.

## Diagnostics to add

At hard stop, capture process list, listening sockets if `ss` exists, tails of node/miner logs, command-log tail, hard-stop reason marker, and whether each child process was alive before and after SIGTERM/SIGKILL.

## Acceptance

Syntax and preflight:

```bash
bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=1 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=2 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=4 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
bash scripts/v2_2_19_preflight_check.sh
```

Hard-stop probe:

```bash
DURATION_SECS=600 QUIESCENCE_WAIT_SECS=180 GLOBAL_DEADLINE_SECS=180 \
OUT_DIR=artifacts/v2_2_19/hard_stop_probe \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh || true

RUN_DIR="$(cat artifacts/v2_2_19/hard_stop_probe/current-run-dir.txt)"
test -f "$RUN_DIR/evidence-summary.md"
test -f "$RUN_DIR/evidence.tar.gz"
test -f "$RUN_DIR/evidence.tar.gz.sha256"
sha256sum -c "$RUN_DIR/evidence.tar.gz.sha256"
ss -ltnp | grep -E ':(28545|28546|28547|28548|28549|32303|32304|32305|32306|32307)\b' && exit 1 || true
```

The hard-stop probe is expected to fail as a rehearsal, but it must finish, package evidence, and leave ports clean.

## Guardrails

- Harness-only unless minimal tests/docs are needed.
- No consensus changes.
- No P2P protocol changes.
- No mining changes.
- No version bump.
- Do not set `public_testnet_ready=true`.
- Do not weaken 5N/1M, 5N/2M, or 5N/4M gates.
