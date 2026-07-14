# Fix pulsedagd workspace regressions

Candidate commit `4e82bd40e0830e66605405f6cb31ff3a630cde55` failed only in `pulsedagd`; every other workspace package, fmt, check, test compilation, and Clippy passed.

## Failure 1: stale fast-cadence test poisons the environment lock

`config::tests::experimental_flags_unlock_millisecond_cadence_and_limits` expects `experimental_fast_cadence=true` and a 250 ms target, but `Config::apply_experimental_guards()` deliberately forces fast cadence off and clamps the interval to the consensus target. The first assertion panics while holding the global test environment mutex, after which every later environment-based config test fails with `PoisonError`.

Required changes in `apps/pulsedagd/src/config.rs`:

1. Add a test-only helper that acquires the environment mutex and recovers a poisoned guard with `PoisonError::into_inner()`.
2. Replace every direct `env_lock().lock().expect("env lock")` use with that helper, so one assertion failure cannot turn the remainder of the config suite into cascading lock failures.
3. Rename/update the stale fast-cadence test to reflect the current guardrail:
   - GhostDAG dev mode is selected.
   - `experimental_fast_cadence` is false after guards.
   - `target_block_interval_ms` is the consensus interval in milliseconds.
   - `target_block_interval_secs` equals the consensus interval in seconds.
   - The explicitly supplied non-cadence experimental limits remain as configured where current semantics allow them.
4. Do not re-enable fast cadence and do not alter production consensus timing.

## Failure 2: lag-injection diagnostic precedence

`tests::lag_injection_evidence_rejects_broadcast_or_gap_mismatch_shortcuts` supplies observed gap 96, canonical gap 95, and configured minimum 96. Both a mismatch and a below-minimum canonical gap are true. The test intentionally expects the more specific integrity error `canonical_gap_disagrees_with_harness_gap`, but validation currently returns `network_gap_below_configured_minimum` first.

Required changes in `apps/pulsedagd/src/main.rs`:

1. After checking the absolute configured floor, check observed-vs-canonical equality before checking whether either gap is below the configured minimum.
2. Preserve all acceptance/rejection semantics; this only changes deterministic error classification when multiple invalid conditions overlap.
3. Add focused assertions covering:
   - mismatched gaps return `canonical_gap_disagrees_with_harness_gap` even when one is also below the configured minimum;
   - equal gaps below the configured minimum return `network_gap_below_configured_minimum`;
   - mismatched gaps both above the configured minimum still return the mismatch error.

## Validation

Run all of the following:

```bash
cargo fmt --all -- --check
cargo test -p pulsedagd config::tests --locked -- --nocapture --test-threads=1
cargo test -p pulsedagd lag_injection_evidence --locked -- --nocapture --test-threads=1
cargo test -p pulsedagd --locked -- --nocapture --test-threads=1
cargo check --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Delete this task file before the final commit. Keep `VERSION=v2.2.20`, `public_testnet_ready=false`, and do not change consensus or PoW semantics.
