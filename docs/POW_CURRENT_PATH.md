# Current Proof-of-Work Path (Audit Guide)

Status date: **2026-04-24**

This guide explains the exact PoW flow the node enforces today, identifies provisional/dev-oriented naming, and defines where future final-PoW upgrades should land.

## 1) End-to-end flow (today)

1. Miner calls `POST /mining/template` with `miner_address`.
2. Node builds a candidate block and returns:
   - canonical header fields,
   - `template_id`, `target_u64`, TTL metadata,
   - `pow_preimage_hex` (for audit/debug parity).
3. External miner searches nonce space against the canonical preimage and acceptance rule.
4. Miner submits `POST /mining/submit` with the solved block (+ optional template id).
5. Node validates PoW and template lifecycle freshness; only then accepts/rejects block via normal chain acceptance.

## 2) Canonical header hashing/preimage construction

The canonical preimage byte layout is implemented in `pow_preimage_bytes` and versioned by `POW_HEADER_PREIMAGE_VERSION`.

Current version: `1`.

Hashing/acceptance path today:

- `pow_hash_hex` => `BLAKE3(preimage_bytes)`
- `pow_hash_score_u64` => big-endian first 8 bytes of the BLAKE3 hash
- `pow_target_u64` => `u64::MAX / max(difficulty, 1)`
- `pow_accepts` => `score_u64 <= target_u64`

Normative spec text is in `docs/POW_SPEC_FINAL.md`.

## 3) What the node validates today (submit path)

`/mining/submit` currently enforces, in order:

1. PoW validity for submitted header (`dev_pow_accepts`, same active rule).
2. Height must be greater than current best height.
3. If template id provided, the stored template must still match current lifecycle:
   - next height,
   - parent set,
   - selected tip,
   - difficulty + target,
   - mempool fingerprint,
   - TTL,
   - template transaction ID set.
4. Submitted parent set must still match current tip set.
5. Block is passed into `accept_block` and orphan adoption/persistence/broadcast handling.

This is the operational anti-stale-work boundary used by node/miner integration today.

## 4) Current constraints (explicit)

- Miner is external and standalone (no in-node miner runtime requirement).
- Official miner flow is template fetch -> nonce search -> submit.
- No pool coordinator/accounting/share/payout behavior is part of official miner scope.
- Public RPC payloads expose legacy/dev-oriented naming (for example `pow_accepted_dev`) that should be interpreted as current active-path status, not a separate algorithm.

### Retarget control and diagnostics notes (v1 granularity update)

- Retarget remains bounded and conservative (no broad consensus redesign):
  - deadband defaults to ±8% around neutral (`10000` bps),
  - damped response defaults to half of raw deviation,
  - hard clamp defaults to `[8000, 12500]` bps.
- Operators can tune bounds/damping with environment variables:
  - `PULSEDAG_RETARGET_DEADBAND_BPS`
  - `PULSEDAG_RETARGET_DAMPING_DIVISOR`
  - `PULSEDAG_RETARGET_MIN_BPS`
  - `PULSEDAG_RETARGET_MAX_BPS`
- `GET /runtime/status` now exposes explicit retarget diagnostics:
  - `retarget_min_bps`, `retarget_max_bps`, `retarget_was_clamped`
  - `retarget_rationale` (`insufficient_signal`, `within_deadband`, `clamped_to_min`, `clamped_to_max`, `damped_increase`, `damped_decrease`)
  - `retarget_signal_quality` (`low` when too little interval history is present)

## 5) Provisional/dev-oriented surfaces (what they mean)

Several APIs/functions are named with `dev_*` because this public testnet path was staged incrementally. Today these names are compatibility surfaces, not alternate consensus rules.

Examples:
- `dev_pow_accepts`, `dev_hash_score_u64`, `dev_target_u64`
- difficulty policy helpers (`dev_difficulty_snapshot`, etc.)
- response field `pow_accepted_dev`

Implementation guidance: do not infer multiple PoW validation algorithms from these names.

## 6) Future final-PoW upgrade boundaries

When final-PoW upgrades are introduced, expected change boundaries are:

1. **Core PoW serialization/hash acceptance** in `crates/pulsedag-core/src/pow.rs` (including preimage versioning).
2. **Deterministic vectors** in `fixtures/pow/official_vectors.json`.
3. **External miner behavior** in `apps/pulsedag-miner` to keep exact parity.
4. **Node RPC contract updates** only via explicit versioning/migration docs if schemas change.

Anything outside these boundaries risks accidental node/miner divergence.

## 7) Non-goals for this clarification

- No consensus-rule changes.
- No public RPC format changes.
- No pool logic introduction.
- No change to miner external/standalone architecture.
