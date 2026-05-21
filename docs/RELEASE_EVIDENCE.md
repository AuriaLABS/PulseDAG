# Release Evidence Policy (v2.2.18 private-testnet RC preparation)

## v2.2.17 gate
v2.2.18 may proceed only when v2.2.17 is:
- **CLOSED_WITH_EVIDENCE**, or
- **WAIVED_WITH_REASON**.

Current v2.2.17 state: **WAIVED_WITH_REASON**.
Reference: `docs/CLOSING_CHECKLIST_V2_2_17.md`.

## v2.2.18 evidence location
- `artifacts/v2_2_18_private_rc/local-3n-1m/<run_id>/`

## Required outputs
- preflight summary and version captures
- node/miner logs
- endpoint captures
- smoke summary
- evidence tarball + sha256

## Guardrails
- Do not claim PASS without evidence path.
- Do not claim consensus/PoW changes, smart contracts, pool logic, v2.3.0 readiness, or v3.0 readiness.

## Release smoke verification hardening
- `scripts/release/verify_release_artifacts.py --smoke` now enforces bounded command runtime via `--smoke-timeout-secs` (default: 10s).
- `pulsedagd --help/-h` and `pulsedagd --version/-V` are immediate-exit commands and are safe for release smoke checks.
- `release-binaries` workflow now applies job-level timeouts and smoke-step command timeouts, so verification fails fast instead of hanging indefinitely.
