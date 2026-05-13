# PulseDAG external mining protocol v1

PulseDAG mining protocol v1 is the stable node-to-standalone-miner surface for the v3.0.0 foundation. It intentionally covers only direct template and submit RPCs. Pool logic and Stratum are **not** part of v1; Stratum may be documented separately as future work.

## Versioning

Every mining template and submit result includes:

- `protocol_version`: `1`

Miners must treat unknown future protocol versions as incompatible unless they explicitly implement them.

## Template request

`POST /mining/template`

```json
{
  "miner_address": "kaspa:..."
}
```

### Template response fields

The success response is wrapped in the standard `ApiResponse` shape and returns these mining fields in `data`:

| Field | Description |
| --- | --- |
| `protocol_version` | Mining protocol version. v1 responses return `1`. |
| `mode` | Template mode string (`external-miner-template`). |
| `algorithm` | PoW algorithm (`kHeavyHash`). |
| `pow_engine` | Canonical engine identifier (`kaspa-kheavyhash`). |
| `miner_address` | Address used to build the coinbase transaction. |
| `template_id` | Required opaque identifier for the exact template lifecycle state. Submit must echo it unchanged. |
| `selected_tip` | Node preferred tip when the template was created. |
| `parent_tips` | Parent hash set committed by the template block header. |
| `created_at_unix` | Template creation time in Unix seconds. |
| `expires_at_unix` | Soft template expiry in Unix seconds. |
| `freshness_ttl_secs` | Template TTL before grace. |
| `freshness_grace_secs` | Additional node-side grace window. |
| `block` | Candidate block. Miners may mutate only the header `nonce` field for v1. |
| `target_u64` | Leading 64-bit compatibility target view. |
| `target_hex` | Canonical 256-bit target as 64 lowercase hex characters. |
| `bits` / `compact_target` | Compact target/difficulty field used by the header. |
| `difficulty` | Expected header difficulty for this template. |
| `network_id` | Chain/network identifier. |
| `nonce_range` | Supported nonce range. |
| `timestamp_min_unix` / `timestamp_max_unix` | Informational timestamp bounds. |
| `next_height` | Height committed by the candidate block. |
| `blue_score` | Candidate blue score. |
| `mempool_tx_count` | Number of mempool transactions included after coinbase. |
| `pow_preimage_hex` | Canonical pre-PoW header bytes with nonce excluded. |
| `pre_pow_hash` | Keccak hash of `pow_preimage_hex` bytes. |
| `pow_preimage_nonce_offset` | Nonce offset hint for miners. |
| `pow_header_preimage_version` | Canonical PoW preimage encoding version. |
| `mutable_header_fields` | v1 returns only `["nonce"]`. |

## Submit request

`POST /mining/submit`

```json
{
  "template_id": "<template_id from /mining/template>",
  "block": { "header": { "nonce": 123 }, "transactions": [] }
}
```

`template_id` is mandatory in v1. A missing `template_id` fails with `reason_code: "missing_template_id"`.

## Submit validation rules

The node enforces these rules before accepting a mined block:

1. `template_id` must be present and known.
2. The stored template must still match the current next height, parent set, selected tip, difficulty/target, mempool fingerprint, and freshness window.
3. Submitted header parents must match the template/current parent set.
4. Submitted header difficulty must match the template difficulty and current consensus difficulty.
5. Submitted header merkle root must match the template merkle root.
6. Submitted transactions must match the template transaction ids.
7. Submitted PoW must satisfy the canonical 256-bit target derived from the header difficulty.
8. The block must pass normal node block acceptance.

## Submit response fields

Accepted and rejected v1 submit responses use `ApiResponse.ok(data)` so miners can inspect `data.reason_code` without relying on transport errors.

Important `data` fields:

| Field | Description |
| --- | --- |
| `protocol_version` | Mining protocol version (`1`). |
| `accepted` | `true` only when the node accepted the block. |
| `reason_code` | Stable machine-readable result/rejection code. |
| `reason` | Human-readable detail. |
| `block_hash` / `block_id` | Submitted/accepted block identifier when available. |
| `height` | Submitted height when available. |
| `pow_algorithm` | PoW algorithm used by the node. |
| `pow_accepted` / `pow_accepted_dev` | PoW validation result. |
| `target_u64` | Leading 64-bit compatibility target view. |
| `target_hex` | Canonical 256-bit target as 64 lowercase hex characters. |
| `pow_hash` | Canonical PoW hash when available. |
| `template_id` | Echoed template id on accepted submits. |
| `invalid_pow` | `true` for invalid PoW rejections. |
| `stale_template` / `stale` | `true` for stale template rejections. |
| `duplicate` | Reserved duplicate indicator. |
| `selected_tip` | Node selected tip after acceptance. |
| `adopted_orphans` | Orphans adopted after acceptance. |
| `pow_hash_score_u64` | Leading 64-bit score view of the PoW hash. |
| `pow_rejection_code` | Low-level PoW rejection code when applicable. |
| `pow_rejection_reason` | Human-readable rejection detail. |

## Stable `reason_code` values

| `reason_code` | Meaning |
| --- | --- |
| `accepted` | Block accepted. |
| `missing_template_id` | Submit omitted mandatory `template_id`. |
| `unknown_template` | Submitted `template_id` is not known to the node. |
| `stale_template` | Template or submitted block no longer matches current node/template state. Inspect `reason` for embedded detail codes such as `template_expired`, `submitted_parents_mismatch`, `invalid_pow` difficulty details, or `submitted_merkle_root_mismatch`. |
| `invalid_pow` | Header hash does not satisfy the target or PoW preimage validation failed. |
| `block_rejected` | Storage or final block acceptance rejected the block. |

## Miner compatibility guidance

A standalone miner should:

1. Request a fresh template.
2. Verify `protocol_version == 1`.
3. Mine only by changing `block.header.nonce`.
4. Submit the solved block with the exact `template_id`.
5. On `missing_template_id`, `unknown_template`, or any `stale_template` result, discard work and request a fresh template.
6. On `invalid_pow`, re-check local target/PoW implementation against `target_hex`, `pow_preimage_hex`, and `pow_header_preimage_version`.
