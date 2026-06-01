# Codex task: make local 3N/1M smoke diagnostics portable without rg

Priority: P0 before running the next v2.2.19/v2.3.0 evidence gates.

## Motivation

The local 3-node / 1-miner smoke harness still calls `rg` directly in failure-evidence diagnostics. On environments without ripgrep this produces `rg: command not found` during a real failure path and contaminates the evidence bundle.

This is not a consensus or P2P behavior change. It is a harness portability fix so failing gates produce clean, classified evidence.

## Scope

Update only harness/scripts/docs needed for this portability fix.

Required script target:

- `scripts/v2_2_19_local_3n_1m_smoke.sh`

Required changes:

1. Add a portable text search helper near the existing helpers, for example:

```bash
text_first_matches(){
  local pattern="$1" file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -n "$pattern" "$file" 2>/dev/null || true
  else
    grep -nE "$pattern" "$file" 2>/dev/null || true
  fi
}
```

2. Replace every direct `rg ...` invocation in `scripts/v2_2_19_local_3n_1m_smoke.sh` with the helper.

Known offending lines from the current failure path include the launch-command extraction inside `capture_p2p_failure_evidence()`:

```bash
rg -n "launch node-a:" "$OUT_DIR/command-log.txt"
rg -n "launch node-b:" "$OUT_DIR/command-log.txt"
rg -n "launch node-c:" "$OUT_DIR/command-log.txt"
```

3. Keep the output useful when neither `rg` nor matching lines exist; the helper must not make the script fail under `set -euo pipefail`.

4. Do not add a fake `rg` shim in CI. The script itself must be portable.

## Acceptance

Run at minimum:

```bash
bash -n scripts/v2_2_19_local_3n_1m_smoke.sh
bash scripts/v2_2_19_preflight_check.sh
```

Recommended extra check simulating no ripgrep:

```bash
PATH="$(printf '%s' "$PATH" | tr ':' '\n' | grep -v '/.cargo/bin' | paste -sd: -)" \
  bash -n scripts/v2_2_19_local_3n_1m_smoke.sh
```

If a shell-only test is practical, add/adjust it so the diagnostic helper returns successfully when `rg` is missing.

## Guardrails

- No consensus changes.
- No protocol changes.
- No mining changes.
- No version bump.
- Do not set `public_testnet_ready=true`.
- Do not introduce a hard dependency on ripgrep, Python, or new external tools.

## After merge

Run gates in this order:

1. `cargo fmt --all -- --check`
2. `cargo check --workspace --locked`
3. `cargo test --workspace --locked`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `bash scripts/v2_2_19_preflight_check.sh`
6. `OUT_DIR=artifacts/v2_2_19/local_3n_1m_smoke bash scripts/v2_2_19_local_3n_1m_smoke.sh`
7. staged convergence gates: 5N/1M, 5N/2M, then 5N/4M stress/diagnostic.
