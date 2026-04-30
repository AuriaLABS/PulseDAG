# PoW Difficulty Retarget Evidence (v2.3.0 private testnet)

## Policy definition

The current retarget policy (operator/configurable, bounded in code) is:

- **Target interval**: `60` seconds (`DEV_TARGET_BLOCK_INTERVAL_SECS`)
- **Window**: `20` blocks (`DEV_DIFFICULTY_WINDOW`, minimum effective window behavior guarded in code)
- **Deadband**: `±800 bps` around neutral `10000` bps
- **Damping**: deviation divided by `2` (`DEV_RETARGET_DAMPING_DIVISOR`)
- **Clamp**: `[8000, 12500]` bps defaults, with safety bounds for env overrides

Multiplier computation:

- `raw_bps = target_interval * 10000 / avg_interval`
- if `raw_bps` in `[10000-deadband, 10000+deadband]`, use `10000` (no move)
- else `damped_bps = 10000 + (raw_bps - 10000) / damping_divisor`
- clamp `damped_bps` to `[min_bps, max_bps]`

Difficulty adjustment:

- `adjusted = round_half_up(current * multiplier_bps / 10000)`
- minimum difficulty floor is `1`

## Signal and diagnostics policy

`dev_difficulty_snapshot` exports evidence fields used by operators/tests:

- `retarget_multiplier_bps`, `retarget_min_bps`, `retarget_max_bps`
- `retarget_was_clamped`
- `retarget_rationale` (`insufficient_signal`, `within_deadband`, `clamped_to_min`, `clamped_to_max`, `adjusted`)
- `retarget_signal_quality` (`low` / `normal`)

Low-signal windows (`observed_intervals < 2`) are explicitly marked and return neutral multiplier (`10000`).

## Test evidence in repository

Retarget behavior is covered by unit tests in `crates/pulsedag-core/src/pow.rs`, including:

- bounded one-step difficulty adjustment behavior
- deterministic snapshot output
- multiplier staying within clamp bounds
- low-signal labeling and rationale

Additionally, official PoW vectors validate stable `target_u64` and acceptance semantics in:

- `crates/pulsedag-core/tests/pow_official_vectors.rs`
- `fixtures/pow/official_vectors.json`

## Conclusion for private testnet prep

For v2.3.0 private testnet:

- Retarget policy is explicitly documented and bounded.
- Neutral-zone, damping, and clamp controls are in place.
- Diagnostics capture rationale and signal quality for operator troubleshooting.
- No PoW algorithm changes are introduced in this prep.
