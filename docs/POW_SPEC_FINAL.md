# PulseDAG Public Testnet PoW Specification (Frozen)

Status: **FINAL / CANONICAL**

This document freezes the single Proof-of-Work path used by PulseDAG public testnet.
It is intentionally explicit so a miner or node can be implemented independently and still interoperate.

## 1) Scope and architecture boundaries

- The **node** validates headers and blocks, emits mining templates, and accepts/rejects submitted solved blocks.
- The **miner** is **external and standalone**. It requests templates, searches nonce space, and submits candidate blocks.
- **Pool logic is out of scope** for this specification and is not part of the official miner flow.
- **Smart contracts are not activated by this PoW spec**.
- There is **one PoW path only** (no runtime algorithm switching).

## 2) Algorithm identity

- The active algorithm identifier remains **`kHeavyHash`**.
- For this public testnet spec, `kHeavyHash` is concretely defined as:
  1. build the canonical serialized header preimage bytes in §3,
  2. compute `BLAKE3(preimage_bytes)` (32 bytes),
  3. evaluate acceptance via §5.

No alternate hashing path is valid for public testnet.

## 3) Canonical header preimage (exact bytes)

### 3.1 Field order (MUST be exact)

1. `preimage_version` (`u8`) = `1`
2. `header.version` (`u32`, little-endian)
3. `parent_count` (`u16`, little-endian)
4. each `parent` string in list order as:
   - `parent_len` (`u16`, little-endian, byte length)
   - UTF-8 bytes of parent string (no null terminator)
5. `header.timestamp` (`u64`, little-endian)
6. `header.difficulty` (`u32`, little-endian)
7. `header.nonce` (`u64`, little-endian)
8. `header.merkle_root` as (`u16` LE len + UTF-8 bytes)
9. `header.state_root` as (`u16` LE len + UTF-8 bytes)
10. `header.blue_score` (`u64`, little-endian)
11. `header.height` (`u64`, little-endian)

### 3.2 Serialization rules

- Strings are serialized exactly as UTF-8 bytes, prefixed by a 16-bit little-endian byte length.
- No separators, delimiters, whitespace normalization, or JSON encoding are used in canonical bytes.
- Parent list order is consensus-relevant and must match the header order exactly.

## 4) Nonce handling

- Nonce field width is `u64`.
- Miner mutates only `header.nonce` while searching work from a template.
- Node validates the submitted header nonce as part of PoW acceptance.
- Official miner thread partitioning (reference behavior): each worker starts at `tid` and increments by `thread_count`.

## 5) Target encoding and acceptance rule

Let `D = max(header.difficulty, 1)` interpreted as unsigned integer.

- Target scalar:
  - `target_u64 = floor((2^64 - 1) / D)`
- Hash score extraction:
  - `hash32 = BLAKE3(preimage_bytes)`
  - `score_u64 = big_endian_u64(hash32[0..8])`
- Acceptance:
  - header is PoW-valid iff `score_u64 <= target_u64`

This is the only acceptance rule for public testnet PoW.

## 6) Difficulty relationship

- Difficulty is inversely proportional to acceptance target:
  - higher `difficulty` -> lower `target_u64` -> fewer acceptable nonces.
- `difficulty = 0` is normalized to `1` for PoW math.

## 7) Node/miner responsibility split (no ambiguity)

### Node responsibilities

- Build and serve templates (`/mining/template`).
- Enforce canonical PoW rule on submitted blocks.
- Accept or reject submissions (`/mining/submit`) and maintain chain state.

### Miner responsibilities

- Fetch template from node.
- Search nonces externally using this spec.
- Submit solved block back to node.

### Explicit non-responsibilities of miner

- No pool coordinator behavior.
- No share accounting, payout logic, or worker orchestration service.
- No server-side block validation authority.

## 8) Canonical references in source tree

- PoW serialization/hash/acceptance: `crates/pulsedag-core/src/pow.rs`
- Header field definitions: `crates/pulsedag-core/src/types.rs`
- External miner behavior: `apps/pulsedag-miner/src/main.rs`
- Miner scope statement: `docs/MINER_FINAL.md`, `apps/pulsedag-miner/README.md`

Any conflicting old notes should be considered non-canonical.
