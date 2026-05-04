# PulseDAG PoW Audit (v2.2.10 freeze)

## Scope
This document audits current PoW references and freezes a single implementation truth before final kHeavyHash integration.

Date: 2026-05-04  
Branch target: `consensus/v2.2.10-pow-truth-audit`

---

## Frozen source of truth for v2.2.10

**Intended consensus target (for next PR):**
- Active consensus PoW = **Kaspa-based kHeavyHash** identity.
- Acceptance rule = **256-bit target comparison** (full-width hash/target validation), not a `u64` surrogate score.

**Current implementation reality (this PR audit only):**
- Algorithm identity is publicly exposed as `kHeavyHash`.
- Active hash path in `pulsedag-core` currently computes **Keccak256(preimage)** and derives a **`score_u64`** from the first 8 bytes, then checks `score_u64 <= target_u64`.
- Multiple RPC responses explicitly call this path a development/surrogate mode.

No consensus-engine behavior changes are introduced in this audit PR.

---

## Current active PoW code path

1. **Declared algorithm identity**
   - `selected_pow_algorithm()` returns `PowAlgorithm::KHeavyHash`.
   - `selected_pow_name()` returns `"kHeavyHash"`.

2. **Hashing and scoring path actually used now**
   - `CanonicalPowEngine::hash_preimage_hex` uses `sha3::Keccak256::digest(preimage)`.
   - `CanonicalPowEngine::score_preimage_u64` uses first 8 bytes of Keccak256 digest as big-endian `u64`.
   - `pow_accepts()` compares `pow_hash_score_u64(header) <= pow_target_u64(difficulty)`.
   - `pow_target_u64(difficulty)` is `u64::MAX / max(difficulty, 1)` through the engine default.

3. **Validation path used by external mining submit**
   - `post_mining_submit` calls `pow_validation_result(&req.block.header)` and rejects block when `pow.accepted == false`.

---

## Current `/pow` endpoint wording

`GET /pow` currently reports:
- `algorithm = selected_pow_name()` (thus `kHeavyHash`)
- `status = "active-devnet"`
- note: "Kaspa-based kHeavyHash-style PoW hashing adapter in the canonical PoW engine."

This wording signals kHeavyHash identity while implementation remains non-final.

---

## Current mining template fields

`post_mining_template` emits `MiningTemplateData` including:
- `algorithm`
- `target_u64`
- `compact_target`
- `pow_preimage_hex`
- `pow_preimage_nonce_offset`
- `pow_header_preimage_version`
- `mutable_header_fields`
- plus template lifecycle fields (`template_id`, `selected_tip`, `parent_tips`, TTL/grace metadata, etc.)

Template invalidation currently keys on height/parents/tip/difficulty/`target_u64`/mempool fingerprint/time freshness.

---

## Current mining submit validation path

- Endpoint: `post_mining_submit`.
- Step 1: `pow_validation_result(header)`.
- Step 2: enforce `pow.accepted` (`score_u64 <= target_u64` in current engine).
- Step 3: stale-template checks (height/parents/selected tip/TTL/lifecycle).
- Step 4: submit/accept block.

So PoW acceptance in submit is currently tied to surrogate `u64` score semantics, not 256-bit target comparison.

---

## Current miner hashing path

`apps/pulsedag-miner` directly reuses core PoW functions:
- `miner_pow_hash_hex -> pow_hash_hex`
- `miner_pow_score_u64 -> pow_hash_score_u64`
- `miner_pow_accepts -> pow_accepts`

Therefore miner/runtime node currently share the same surrogate path.

---

## Inconsistencies found

1. **Identity vs implementation mismatch**
   - Public algorithm identity and fixtures/docs label PoW as `kHeavyHash`.
   - Active engine hashing implementation is Keccak256 + `u64` scoring.

2. **Docs conflict on effective hash**
   - Some docs/specs describe BLAKE3-based surrogate path.
   - Current core code path uses Keccak256.

3. **Consensus-shape mismatch**
   - v2.2.10 target requires full 256-bit target comparison.
   - Active code uses reduced `score_u64` / `target_u64` rule.

4. **RPC transparency but mixed messaging**
   - `/pow_hash_header` and `/pow_check_header` explicitly flag development/surrogate mode.
   - `/pow` presents kHeavyHash-style adapter wording that can be read as already-finalized consensus behavior.

---

## TODO markers for follow-up PR (minimal and targeted)

- TODO(v2.2.10-final-pow): replace canonical `CanonicalPowEngine` surrogate Keccak+`u64` scoring path with final Kaspa-based kHeavyHash implementation and full 256-bit target compare.
- TODO(v2.2.10-final-pow): align `/pow`, `/pow_hash_header`, `/pow_check_header` wording to final consensus truth once implementation lands.
- TODO(v2.2.10-final-pow): update/normalize PoW docs to remove stale BLAKE3/Keccak placeholder references and keep one canonical spec.

