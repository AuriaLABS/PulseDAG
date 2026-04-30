# PoW Private Testnet Prep (v2.3.0)

## Scope

This document records the current PoW implementation and verification evidence for the v2.3.0 **private testnet** prep cycle.

- **Consensus PoW algorithm identifier**: `kHeavyHash`
- **Implementation hash function**: `blake3::hash(preimage)`
- **Acceptance rule**: `score_u64 <= target_u64`
- **No algorithm change** in this prep. Any PoW algorithm changes require an explicit network reset process.

## Canonical PoW algorithm and acceptance

The canonical engine is `CanonicalPowEngine`, with:

1. `hash_hex = blake3(preimage)` (hex encoded)
2. `score_u64 = first_8_bytes_of_hash_as_big_endian_u64`
3. `target_u64 = u64::MAX / max(difficulty, 1)`
4. Accepted iff `score_u64 <= target_u64`

## Canonical preimage format (versioned)

`POW_HEADER_PREIMAGE_VERSION = 1` and serialized bytes are frozen in this order:

1. preimage version (`u8`)
2. `header.version` (`u32`, little-endian)
3. parent count (`u16`, little-endian)
4. each parent hash string as (`u16` byte length LE + UTF-8 bytes)
5. `header.timestamp` (`u64`, little-endian)
6. `header.difficulty` (`u32`, little-endian)
7. `header.nonce` (`u64`, little-endian)
8. `header.merkle_root` (`u16` length LE + UTF-8 bytes)
9. `header.state_root` (`u16` length LE + UTF-8 bytes)
10. `header.blue_score` (`u64`, little-endian)
11. `header.height` (`u64`, little-endian)

## Test vector evidence (official fixture)

Official vectors are stored in `fixtures/pow/official_vectors.json` and enforced by tests in:

- `crates/pulsedag-core/tests/pow_official_vectors.rs`
- `apps/pulsedag-miner/tests/pow_vectors.rs`

Vector coverage includes all required fields:

- preimage (`preimage_hex`)
- hash (`pow_hash_hex`)
- score (`pow_score_u64`)
- target (`target_u64`)
- accepted/rejected (`accepts`)

### Valid vectors

- `genesis-like-low-difficulty`
- `single-parent-mid-difficulty`
- `two-parents-high-difficulty`

### Invalid (tamper) vectors

- `tampered-hash` (must fail `pow_hash_hex`)
- `tampered-acceptance` (must fail `accepts`)

## Evidence summary

Current vectors demonstrate:

- Deterministic preimage byte encoding across header shapes (0, 1, and 2 parents)
- Stable hash + score derivation from the same canonical preimage
- Difficulty-derived target behavior from trivial to high-difficulty bounds
- Rejection behavior under high difficulty where score exceeds target
- Tamper detection in invalid fixtures (hash mismatch and acceptance mismatch)

## Private testnet readiness notes

- PoW implementation and vectors are already checked in and exercised by workspace tests.
- Private testnet nodes/miners should use these exact semantics with no local divergences.
- Any future changes to preimage schema or scoring semantics must be versioned and treated as consensus-impacting.
