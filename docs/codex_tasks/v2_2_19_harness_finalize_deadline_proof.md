# Codex task: make private rehearsal finalization deadline-proof

Priority: P0 before any further 5N/4M stress run.

## Evidence

After PR #558, a short packaging probe on commit `09b215f3289debb94e46984c30d5fd61714dc31a` still failed to leave the expected root evidence pointers/artifacts.

Command used:

```bash
DURATION_SECS=60 \
QUIESCENCE_WAIT_SECS=10 \
GLOBAL_DEADLINE_SECS=120 \
OUT_DIR=artifacts/v2_2_19/harness_packaging_probe \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh || true

RUN_DIR="$(cat artifacts/v2_2_19/harness_packaging_probe/current-run-dir.txt)"
ls -lah "$RUN_DIR"
test -f "$RUN_DIR/evidence-summary.md"
test -f "$RUN_DIR/evidence.tar.gz"
test -f "$RUN_DIR/evidence.tar.gz.sha256"
sha256sum -c "$RUN_DIR/evidence.tar.gz.sha256"
```

Observed output:

```text
FATAL: global deadline exceeded after 120s
FAIL[GLOBAL_DEADLINE_TIMEOUT]: global deadline exceeded after 120s
FATAL: global deadline exhausted before curl: n1:/status final
FAIL[GLOBAL_DEADLINE_TIMEOUT]: global deadline exhausted before curl: n1:/status final
cat: artifacts/v2_2_19/harness_packaging_probe/current-run-dir.txt: No such file or directory
ls: cannot access '': No such file or directory
sha256sum: /evidence.tar.gz.sha256: No such file or directory
FATAL: private rehearsal global deadline 120s reached; terminating script
```

The script had created a run directory visible in launch lines:

```text
artifacts/v2_2_19/harness_packaging_probe/20260602T210154Z
```

But it did not publish `current-run-dir.txt` at the OUT_DIR root before the deadline path, and it did not complete packaging. Also the watchdog printed after the shell prompt, suggesting an asynchronous watchdog/SIGTERM path can outlive or interrupt finalization.

## Root cause candidates

Audit these exact areas:

- `current-run-dir.txt` is only written from summary/package functions late in execution. It must be written immediately after run-dir creation.
- `cleanup()` calls `collect_final_state cleanup-pre-quiescence`, which calls `safe_curl_optional`. `safe_curl_optional` still checks the global deadline and can `exit 124` during cleanup/finalization.
- The global watchdog can `kill -TERM $$` while cleanup is already running, because it is killed only late in cleanup.
- `stop_pids()` calls `sleep_with_deadline`, which can consult the global deadline unless cleanup handling fully bypasses it.
- The finalization path should never depend on curl, jq, tar, grep, optional endpoint capture, or the global deadline being still available.

## Required behavior

Fix `scripts/v2_2_19_private_5n_4m_rehearsal.sh` so evidence finalization is deadline-proof:

1. Immediately after `RUN_DIR`/`OUT_DIR` creation, write:
   - `$OUT_DIR_ROOT/current-run-dir.txt`
   - optionally `$OUT_DIR/current-run-dir.txt`
2. In cleanup/finalization, disable deadline enforcement before any final endpoint capture:
   - set `IN_CLEANUP=1` before calling any function that may curl/sleep;
   - make `safe_curl_json` never `exit` during cleanup; it should write a JSON failure stub and return non-zero instead.
3. Kill/disable the watchdog at the start of cleanup, not near the end.
4. Make signal handling idempotent and avoid recursive cleanup.
5. Ensure cleanup always reaches `package_evidence`, even if final endpoint capture fails or times out.
6. If endpoint capture is impossible because the deadline is exhausted, skip endpoint capture and package whatever already exists.
7. Print `RUN_DIR=...`, `FINAL_EXIT_CODE=...`, and `FINAL_RESULT=...` before exit on every path.
8. Leave root-level copies where the caller expects them:
   - `$OUT_DIR_ROOT/evidence-summary.md`
   - `$OUT_DIR_ROOT/evidence.tar.gz`
   - `$OUT_DIR_ROOT/evidence.tar.gz.sha256`
   - `$OUT_DIR_ROOT/current-run-dir.txt`

## Acceptance

Syntax/preflight:

```bash
bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=1 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=2 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
MINER_COUNT=4 bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
bash scripts/v2_2_19_preflight_check.sh
```

Deadline packaging probe:

```bash
DURATION_SECS=60 QUIESCENCE_WAIT_SECS=10 GLOBAL_DEADLINE_SECS=120 \
OUT_DIR=artifacts/v2_2_19/harness_packaging_probe \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh || true

RUN_DIR="$(cat artifacts/v2_2_19/harness_packaging_probe/current-run-dir.txt)"
echo "RUN_DIR=$RUN_DIR"
ls -lah "$RUN_DIR"
test -f "$RUN_DIR/evidence-summary.md"
test -f "$RUN_DIR/evidence.tar.gz"
test -f "$RUN_DIR/evidence.tar.gz.sha256"
test -f artifacts/v2_2_19/harness_packaging_probe/evidence-summary.md
test -f artifacts/v2_2_19/harness_packaging_probe/evidence.tar.gz
test -f artifacts/v2_2_19/harness_packaging_probe/evidence.tar.gz.sha256
sha256sum -c "$RUN_DIR/evidence.tar.gz.sha256"
sha256sum -c artifacts/v2_2_19/harness_packaging_probe/evidence.tar.gz.sha256
```

The probe may fail as a rehearsal because the deadline is intentionally short, but it must package evidence and checksum must verify.

## Guardrails

- Harness-only change.
- No consensus changes.
- No P2P protocol changes.
- No mining changes.
- No version bump.
- Do not set `public_testnet_ready=true`.
- Do not weaken 5N/1M, 5N/2M, or 5N/4M gates.
