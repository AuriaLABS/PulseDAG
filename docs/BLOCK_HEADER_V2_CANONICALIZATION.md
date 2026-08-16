# PulseDAG v2.4.0 Block Header v2 Canonicalization

Status: **Task 25/28 consensus serialization contract**

Related issues: `#867`, `#870`

This document refines the Task 22 activation contract for the separate v2 block-header/hash and mining-preimage paths. It does not modify or reinterpret v1 header bytes.

## Scope

The activated v2.4.0 header path uses:

- `BlockHeader.version = 2`;
- header domain `PulseDAG:block-header:v2`;
- the exact canonical `chain_id` as a length-prefixed consensus-domain field;
- deterministic GHOSTDAG parent-set serialization;
- activated GHOSTDAG `blue_score` semantics only after selection/classification metadata is complete.

The v1 `PulseDAG:block-header:v1` and v1 mining-preimage paths remain immutable historical behavior.

## Canonical parent-set contract

For an activated v2 header, `BlockHeader.parents` is a consensus **set represented canonically as a vector**.

The canonical representation is frozen as follows:

1. Parent hashes are compared by their exact stored UTF-8 bytes. No case folding, Unicode normalization, trimming, decoding, or textual rewriting is performed.
2. The canonical vector is strictly ascending lexicographic byte order.
3. Duplicate parent hashes are invalid. A serializer/validator must fail closed instead of deduplicating them silently.
4. Empty parent hashes are invalid.
5. The parent count must not exceed `GHOSTDAG_V1_MAX_PARENTS` (currently 64).
6. A non-genesis activated block must contain at least one parent. Genesis identity is frozen separately and is not produced by the ordinary mining-template path.
7. The selected parent is derived from the complete parent set by the GHOSTDAG selection rule. It is **not** encoded by placing it first, and parent-vector order must never influence selected-parent choice.
8. Header-v2 hashing and mining-preimage-v2 generation must consume the same canonical parent vector.

Nodes may canonicalize a locally assembled parent set before constructing a candidate header. Once a v2 header is presented as a consensus object, a non-canonical parent vector is malformed and must be rejected; validators must not mutate the received object into a different canonical header.

## Chain-domain placement

The high-level header-v2 byte order is frozen as:

```text
len("PulseDAG:block-header:v2")
"PulseDAG:block-header:v2"
len(chain_id)
chain_id
header_version = 2
parent_count
canonical_parent_hashes...
timestamp
difficulty
nonce
merkle_root
state_root
blue_score
height
```

Integer widths, length-prefix widths, and little-endian conventions must be frozen by the implementing core PR and covered by golden vectors. The implementation must not reuse v1 bytes under a different version number.

The mining-preimage-v2 path must use the same domain identity and canonical parent vector, while excluding only the miner-controlled nonce material required by the PoW engine contract. It must not silently call the v1 mining-preimage encoder.

## Core implementation surface

The non-activating core implementation freezes the header-v2 byte encoding as follows:

- string and byte-field length prefixes are unsigned 32-bit little-endian integers;
- parent-vector count is an unsigned 32-bit little-endian integer;
- `header_version` and `difficulty` are unsigned 32-bit little-endian integers;
- `timestamp`, `nonce`, `blue_score`, and `height` are unsigned 64-bit little-endian integers;
- `chain_id`, parent hashes, `merkle_root`, and `state_root` are encoded as exact length-prefixed UTF-8 bytes without normalization;
- `canonicalize_block_parents_v2` is the local assembly helper for deterministic parent ordering;
- `validate_block_header_v2_shape` fail-closes received v2 headers that violate the version, chain-domain, parent-count, uniqueness, non-empty, or canonical-order requirements;
- `canonical_block_header_bytes_v2` is the separate v2 canonical serialization path;
- `compute_block_hash_v2` hashes only that v2 canonical byte sequence and does not reinterpret the historical v1 hash path.

The v2 mining-preimage serializer remains a separate follow-up surface. It must reuse the same frozen domain, widths, chain identity, and canonical parent vector while excluding only nonce material. No PoW engine, miner, block-validation, or activation path is switched by the header-hash implementation slice.

## Fail-closed requirements

Header-v2/hash or mining-preimage-v2 APIs must reject at least:

- header version other than `2`;
- empty `chain_id`;
- empty parent hash;
- duplicate parent hash;
- parent count above the frozen maximum;
- non-canonical parent-vector order when validating a received activated header;
- any missing activation identity required by the caller.

A wrong `chain_id` must produce a different canonical header hash/preimage domain. Distinct networks must not be able to reuse the same v2 header as the same consensus object.

## Required tests before mining integration

The implementing core slice must include:

- v1 block-hash golden-vector regression proving no v1 byte change;
- v2 golden vectors for at least two distinct `chain_id` values;
- parent-order permutation tests proving local canonical assembly produces one canonical vector;
- rejection tests for duplicate, empty, over-limit and non-canonical received parent vectors;
- proof that selected-parent result is independent of parent input order;
- cross-chain hash/preimage separation.

## Activation guardrail

This contract does not activate header v2, `ghostdag_v1`, high cadence, public-testnet launch, or smart contracts. Mining/mempool/wallet integration must remain non-activating until storage/snapshot activation identity, P2P compatibility, deterministic state/finality and Task 30 evidence satisfy the v2.4 activation contract.
