# PulseDAG v2.4.0 Consensus Difficulty Retarget

Status: **ACTIVE DESIGN + IMPLEMENTATION ON `release/2.4.0`**

Tracks: issue `#786`

## 1. Burn-in finding

A 24-hour single-node Windows/Docker burn-in produced more than 3,000 blocks while every template continued to report `difficulty=1`.

The apparent 60–70 second cadence after tuning the miner was created by miner polling delay. Consensus itself was not regulating block production.

The previous integer adjustment could not escape the minimum value:

```text
round(1 * 1.25) = 1
```

This made `difficulty=1` an absorbing state.

## 2. Consensus objective

The v2.4.0 retarget targets:

- 60 seconds per selected-chain block;
- a 20-block observation window;
- an 8% neutral deadband;
- half-deviation damping;
- bounded work adjustment between `0.80x` and `1.25x` per step;
- deterministic compact-target output shared by every node.

The miner loop delay is polling/backoff only. It is not a consensus clock.

## 3. Canonical representation

v2.4.0 treats the header field historically named `difficulty` as canonical compact target bits.

Consensus defines:

```text
POW_LIMIT_BITS = 0x207fffff
MIN_TARGET_BITS = 0x01010000
```

`POW_LIMIT_BITS` is the easiest target allowed by the v2.4.0 private-testnet rules. A calculated target may never become easier than this limit.

The compatibility field name `expected_difficulty` remains in existing response structures, but its value is compact target bits. New diagnostics expose the explicit `current_bits` and `expected_bits` names.

## 4. Retarget calculation

Consensus first derives the observed selected-chain interval. Genesis height `0` and timestamp `0` are excluded.

When at least one real interval exists but second-level timestamp averaging produces `0`, consensus clamps the observed interval to `1` second so an ultra-fast chain still hardens. The 60-second target fallback is used only when no interval has been observed.

The existing work multiplier remains:

```text
raw_work_multiplier = target_interval / observed_interval
damped_work_multiplier = 1 + (raw_work_multiplier - 1) / 2
bounded_work_multiplier = clamp(damped_work_multiplier, 0.80, 1.25)
```

Target movement is the reciprocal:

```text
target_multiplier = 1 / bounded_work_multiplier
next_target = current_target * target_multiplier
next_target = clamp(next_target, MIN_TARGET, POW_LIMIT)
next_bits = compact(next_target)
```

Therefore:

- blocks faster than 60 seconds reduce the target and increase required work;
- blocks slower than 60 seconds increase the target and reduce required work;
- the easiest target can harden immediately;
- no integer-difficulty floor can trap the retarget.

The 256-bit target is scaled deterministically using fixed-width limb arithmetic. Consensus does not use floating point.

## 5. Chain-history rules

Retarget history is taken only from `dag.selected_chain`.

Side-branch blocks must not influence the next target merely because they have a greater height or timestamp in the in-memory DAG.

After pruning or snapshot recovery, every node must derive the same retained selected-chain window and next compact bits.

Block validation derives expected bits from the canonical state at the selected parent. This keeps valid side branches independent from a newer preferred tip while preserving identical rules for templates, peer blocks, and locally mined blocks.

## 6. Consensus/RPC single source of truth

These surfaces consume `consensus_difficulty_snapshot`:

- block-template construction;
- block validation through `expected_difficulty`;
- `/pow/metrics`;
- `/pow/policy`;
- `/pow/dashboard`;
- `/pow/metrics/capture`.

Environment variables used by older development diagnostics do not alter v2.4.0 consensus parameters.

The `/pow/*` diagnostics expose the canonical consensus view, including:

- current and suggested compact bits;
- current, suggested, and PoW-limit targets;
- observed interval and sample count;
- work multiplier and target multiplier;
- clamp and signal-quality diagnostics.

## 7. Compatibility and activation

This changes consensus rules.

The preferred private-testnet rollout is a clean v2.4.0 chain restart because:

- no public testnet has launched;
- the 30-day public-testnet clock has not started;
- the existing burn-in chain is evidence for the defect, not release state.

Preserving an existing chain would require an explicit activation height and exact old/new rule boundary. No implicit mixed-version transition is permitted.

## 8. Required tests

The release gate must prove:

- the easiest target hardens under one-second blocks;
- real zero-second observations clamp to one second instead of falling back to the target interval;
- a legacy `difficulty=1` tip cannot remain trapped;
- stable 60-second intervals preserve compact bits;
- slow blocks relax target without crossing the PoW limit;
- genesis timestamp zero is excluded;
- side branches do not affect selected-chain retarget;
- fixed-width target scaling is deterministic and bounded;
- template, `/pow`, and validation report identical next bits;
- snapshot/restart/pruning preserve the next target;
- two nodes with identical selected-chain history produce byte-identical target hex.

Mining fixtures must use the expected compact bits for their parent context, produce timestamps valid for that context, mine a valid nonce, and then recompute the canonical block hash and state-dependent consensus identifiers. This prevents tests from passing through legacy `difficulty=1` assumptions or submitting a header whose identity changed after nonce selection.

## 9. Release block

Task 18 and issue `#786` are consensus release blockers for v2.4.0.

No v2.4.0 release decision may be approved until the exact candidate passes the deterministic unit tests, multi-node agreement tests, restart/pruning tests, and a new burn-in without miner-delay-based cadence control.
