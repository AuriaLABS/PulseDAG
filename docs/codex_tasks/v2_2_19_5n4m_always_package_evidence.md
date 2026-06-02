# Codex task: make private rehearsal always package evidence

Priority: P0 before the next 5N/4M run.

Problem: the private rehearsal can finish without a clear final PASS/FAIL line and without `evidence-summary.md` or `evidence.tar.gz` in the expected run directory. Any run, including timeout or Ctrl-C, must leave evidence.

Required script target: `scripts/v2_2_19_private_5n_4m_rehearsal.sh`.

Required behavior on every exit path:

- print the run directory;
- print the final exit code;
- write or update `evidence-summary.md`;
- write `evidence.tar.gz`;
- write `evidence.tar.gz.sha256`;
- kill spawned node and miner processes idempotently;
- do not skip packaging because of missing optional files or failed optional commands.

Implementation guidance:

- audit traps, cleanup, `record_fail`, and final packaging;
- add one guarded `finalize_run()` called from `EXIT`;
- make finalization safe under `set -euo pipefail`;
- avoid recursive finalization;
- if full summary generation fails, write a minimal summary with result, exit code, commit, run dir, and command-log tail;
- keep PASS behavior unchanged.

Acceptance:

```bash
bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=1 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=2 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=4 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
bash scripts/v2_2_19_preflight_check.sh
```

Manual validation after merge: run a short 5N/4M probe and verify `evidence-summary.md`, `evidence.tar.gz`, and `evidence.tar.gz.sha256` exist and checksum passes.

Guardrails: harness-only, no consensus changes, no protocol changes, no mining changes, no version bump, do not set public-testnet ready, and do not weaken gates.
