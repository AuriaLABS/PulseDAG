# Current Proof-of-Work Path (Audit Guide)

Status date: **2026-08-01**

This guide describes the PoW path on the `release/2.4.0` branch. The branch contains an active consensus-change candidate and does not authorize a release, public-testnet launch, or mixed-version deployment.

## 1) End-to-end flow

1. Miner calls `POST /mining/template` with `miner_address`.
2. Node builds a candidate block and returns:
   - canonical header fields;
   - `template_id`, compact bits, 256-bit target metadata, and TTL;
   - `pow_preimage_hex` for audit parity.
3. External miner searches nonce space with the canonical kHeavyHash adapter.
4. Miner submits `POST /mining/submit` with the solved block and template id.
5. Node validates template lifecycle, expected compact bits, full-width PoW, block state, persistence, and broadcast behavior.

The miner remains external and standalone.

## 2) Canonical header and PoW acceptance

The canonical pre-PoW layout is implemented by `PowHeaderPreimage` and versioned by `POW_HEADER_PREIMAGE_VERSION`.

Current preimage version: `1`.

Active hashing path:

- canonical PulseDAG header serialization;
- Keccak-256 pre-PoW hash;
- Kaspa-derived kHeavyHash finalization with the candidate nonce;
- full 256-bit big-endian comparison.

Consensus accepts a candidate only when:

```text
pow_hash_256 <= target_256
```

The leading `u64` score and `target_u64` fields are telemetry projections. They are not the final acceptance rule.

Normative PoW identity and acceptance requirements remain in [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md).

## 3) Compact target representation

On the v2.4.0 branch, the header field historically named `difficulty` carries canonical compact target bits.

The compatibility name remains in existing structures and APIs, but consensus meaning is:

```text
header.difficulty == compact_target_bits
```

The v2.4.0 candidate defines:

```text
POW_LIMIT_BITS = 0x207fffff
MIN_TARGET_BITS = 0x01010000
```

The PoW limit is the easiest target permitted by these private-testnet rules.

A clean v2.4.0 chain uses `POW_LIMIT_BITS` in the genesis header, intentionally changing the genesis hash from the v2.3.0 line.

## 4) Consensus retarget

The active v2.4.0 candidate targets one selected-chain block every 60 seconds.

Fixed consensus parameters:

- selected-chain observation window: 20 blocks;
- target interval: 60 seconds;
- neutral deadband: ±8%;
- damping: half of raw deviation;
- bounded work multiplier: `[8000, 12500]` basis points;
- no floating-point arithmetic.

Consensus calculates a bounded work multiplier and applies its reciprocal to the full 256-bit target:

```text
work_multiplier = bounded(target_interval / observed_interval)
target_multiplier = 1 / work_multiplier
next_target = clamp(current_target * target_multiplier, MIN_TARGET, POW_LIMIT)
next_bits = compact(next_target)
```

Fast blocks lower the target. Slow blocks raise it without crossing the PoW limit.

The window:

- follows `dag.selected_chain`;
- excludes genesis height `0`;
- excludes timestamp `0`;
- ignores side branches that are not selected.

Full design and migration requirements are in [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md) and issue `#786`.

## 5) Consensus parameters are not operator settings

Environment variables exposed by legacy `dev_*` helpers are not consensus controls.

In particular, operators must not expect these variables to change v2.4.0 block validity:

```text
PULSEDAG_RETARGET_DEADBAND_BPS
PULSEDAG_RETARGET_DAMPING_DIVISOR
PULSEDAG_RETARGET_MIN_BPS
PULSEDAG_RETARGET_MAX_BPS
PULSEDAG_DIFFICULTY_WINDOW
PULSEDAG_DIFFICULTY_USE_MEDIAN
```

Changing consensus parameters per node would cause deterministic disagreement and chain splits.

The v2.4.0 fixed policy lives in `crates/pulsedag-core/src/retarget.rs`.

## 6) Single source of truth

The following surfaces consume `consensus_difficulty_snapshot`:

- mining-template construction;
- expected-bits validation in `validate_block`;
- `/pow` diagnostics.

`/pow` reports:

- current and suggested compact bits;
- current, suggested, and PoW-limit target hex;
- observed selected-chain interval;
- work and target multipliers;
- clamp rationale and signal quality.

The template, diagnostics, and validator must agree byte-for-byte on the next target.

## 7) Submit-path validation

`/mining/submit` enforces:

1. required template identity;
2. early stale-height detection;
3. bounded submit-actor admission;
4. expected compact bits and full-width PoW;
5. template height, parents, selected tip, target, mempool fingerprint, transaction set, and TTL;
6. canonical block validation and acceptance;
7. persistence before broadcast.

The 24-hour burn-in also showed that a client-facing submit timeout can occur after eventual block acceptance. A timeout is therefore not sufficient evidence of rejection. Task 17 requires unknown-finality reconciliation by block hash.

## 8) Compatibility boundary

The target-based retarget and compact genesis bits are consensus changes.

Preferred private-testnet migration:

- preserve the v2.3.0 burn-in data as defect evidence;
- start a clean v2.4.0 chain;
- require every node and miner to use the same exact candidate.

Preserving an old chain requires an explicit activation height and separately reviewed old/new validation boundary. No implicit mixed-version transition is permitted.

## 9) Required release evidence

Before v2.4.0 approval, evidence must show:

- easiest-target escape under fast blocks;
- stable 60-second target preservation;
- slow-block relaxation bounded by the PoW limit;
- selected-chain-only history;
- identical target calculation after restart, snapshot, pruning, and replay;
- node/miner/template/metrics agreement;
- no cadence dependence on miner sleep;
- multi-node convergence on the same target history.

Consensus test fixtures that construct acceptable blocks must derive compact bits from the same selected-parent state context used by validation, mine a valid nonce, and only then refresh the canonical block identity. Legacy `difficulty=1` fixtures are invalid on the v2.4.0 candidate and must not be used to bypass the target-based acceptance path.

## 10) Non-goals

- replacing kHeavyHash;
- embedded pool accounting or payout logic;
- public-testnet launch;
- smart-contract activation;
- operator-configurable consensus retarget parameters.
