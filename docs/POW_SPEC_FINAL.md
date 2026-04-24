# PulseDAG Public Testnet PoW Specification (Current Canonical Path)

Status: **CURRENT / CANONICAL FOR PUBLIC TESTNET**

This document defines the PoW behavior that nodes validate **today** on public testnet, and marks which nearby surfaces are provisional/dev-oriented.

## 1) Scope and architecture boundaries

- The **node** validates blocks and headers, serves mining templates, and accepts/rejects submitted mined blocks.
- The **miner** remains **external and standalone**. It fetches templates, searches nonce space, and submits candidate blocks.
- **Pool logic is not part of the official miner flow** in this phase.
- There is **one active PoW path** for validation (`kHeavyHash` identity; §2 + §5).
- This document does **not** change contract/smart-contract activation scope.

## 2) Algorithm identity (what nodes enforce today)

- Active algorithm name: **`kHeavyHash`**.
- Current concrete definition:
  1. Build canonical header preimage bytes exactly as in §3.
  2. Compute `BLAKE3(preimage_bytes)` (32 bytes).
  3. Evaluate acceptance with §5 (`score_u64 <= target_u64`).

No alternate hashing path is valid for the node validation path.

## 3) Canonical header preimage (exact bytes)

### 3.1 Field order (MUST match exactly)

1. `preimage_version` (`u8`) = `1`
2. `header.version` (`u32`, little-endian)
3. `parent_count` (`u16`, little-endian)
4. each `parent` string in list order as:
   - `parent_len` (`u16`, little-endian, byte length)
   - UTF-8 bytes of parent string
5. `header.timestamp` (`u64`, little-endian)
6. `header.difficulty` (`u32`, little-endian)
7. `header.nonce` (`u64`, little-endian)
8. `header.merkle_root` as (`u16` little-endian length + UTF-8 bytes)
9. `header.state_root` as (`u16` little-endian length + UTF-8 bytes)
10. `header.blue_score` (`u64`, little-endian)
11. `header.height` (`u64`, little-endian)

### 3.2 Serialization rules

- Strings are serialized as raw UTF-8 bytes with 16-bit little-endian byte lengths.
- No JSON canonicalization, separators, whitespace normalization, or null terminators are used.
- Parent list order is consensus-relevant because it is hashed as-provided.

## 4) Nonce handling

- Nonce width is `u64`.
- Miner mutates `header.nonce` while searching.
- Node validates the submitted nonce via PoW acceptance.
- Reference miner thread partitioning: worker `tid` starts at nonce `tid` and steps by `thread_count`.

## 5) Target encoding and acceptance rule

Let `D = max(header.difficulty, 1)`.

- `target_u64 = floor((2^64 - 1) / D)`
- `hash32 = BLAKE3(preimage_bytes)`
- `score_u64 = big_endian_u64(hash32[0..8])`
- Accepted iff `score_u64 <= target_u64`

`difficulty = 0` is normalized to `1` for PoW arithmetic.

## 6) What the node validates today on `/mining/submit`

Beyond PoW acceptance itself, node submit handling currently rejects stale/invalid work if template lifecycle state no longer matches (height, parents, preferred tip, difficulty/target, mempool fingerprint, TTL, and template transaction set), and then runs normal block acceptance.

See `docs/POW_CURRENT_PATH.md` for the step-by-step request/validation flow and code pointers.

## 7) Current vs provisional/dev-oriented surfaces

The following names are retained for compatibility and operator visibility, but do not represent an alternate consensus PoW algorithm:

- helper function aliases prefixed with `dev_*` that currently delegate to the active PoW path,
- response fields such as `pow_accepted_dev`.

Interpretation: these are **naming/operational surfaces**, not a second PoW rule.

## 8) Upgrade boundaries for future final-PoW changes

To avoid node/miner divergence, future PoW upgrades should be introduced only through explicit, coordinated changes to:

1. canonical preimage versioning and serialization in `pow.rs`,
2. deterministic vectors in `fixtures/pow/official_vectors.json`,
3. miner implementation consuming the same preimage/acceptance rules,
4. any RPC schema changes (if ever needed) behind explicit versioning/migration notes.

Until such a coordinated upgrade lands, implementers should treat §2–§5 as normative behavior.

## 9) Canonical references

- PoW serialization/hash/acceptance: `crates/pulsedag-core/src/pow.rs`
- Header fields: `crates/pulsedag-core/src/types.rs`
- Template flow: `crates/pulsedag-rpc/src/handlers/mining_template.rs`
- Submit validation path: `crates/pulsedag-rpc/src/handlers/mining_submit.rs`
- External miner: `apps/pulsedag-miner/src/main.rs`
- Flow explainer: `docs/POW_CURRENT_PATH.md`

## 10) Deterministic test vectors

Canonical PoW vectors:

- `fixtures/pow/official_vectors.json`

Extension rule: append vectors without mutating existing vector IDs/expected outputs.
