# PoW Spec — Private Testnet V1

- Algorithm id/name: `kHeavyHash`.
- Canonical header preimage order: `preimage_version(u8)`, `version(u32 LE)`, `parent_count(u16 LE)`, each `parent(len u16 LE + utf8 bytes)`, `timestamp(u64 LE)`, `difficulty(u32 LE)`, `nonce(u64 LE)`, `merkle_root(len u16 LE + utf8 bytes)`, `state_root(len u16 LE + utf8 bytes)`, `blue_score(u64 LE)`, `height(u64 LE)`.
- Length prefix rules: reject before hashing if parent count or any length-prefixed field exceeds `u16::MAX` bytes.
- Target/score rule: `target_u64 = u64::MAX / max(difficulty, 1)` and block is valid iff `score_u64 <= target_u64`.
- Rejection codes:
  - `parent_count_too_large`
  - `parent_hash_too_long`
  - `merkle_root_too_long`
  - `state_root_too_long`
  - `score_above_target`
- Official vectors: `fixtures/pow/official_vectors.json`.
