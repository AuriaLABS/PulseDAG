# PulseDAG v2.2.16 miner/node contract

This document defines the v2.2.16 hardening target for the contract between `pulsedagd` and the standalone external `pulsedag-miner`.

At opening, this document is a contract target. Later v2.2.16 PRs must replace placeholders with exact endpoint, field, encoding, and test evidence details from the implementation.

## Guardrails

- `pulsedag-miner` remains external and standalone.
- `pulsedagd` must not gain embedded miner or pool coordination logic.
- No pool logic is added to the miner.
- No smart contracts or contract runtime are added.
- No consensus changes are made unless fixing a documented safety bug with tests.
- GPU mining, if implemented, lives only in the external miner and remains optional/experimental.

## Contract objectives

The miner/node contract must make these behaviors stable and testable:

1. Template fetch.
2. Canonical preimage construction.
3. Nonce search.
4. Target comparison.
5. Submit validation.
6. Stale template handling.
7. Error taxonomy.
8. Diagnostics.
9. Restart/reconnect recovery.

## Canonical mining template target

The canonical template should document and test these fields where supported by the current implementation:

| Field | Required by v2.2.16 | Notes |
| --- | --- | --- |
| `chain_id` | yes | Prevents accidental cross-network mining and submit. |
| network/profile | yes | Operator-visible profile context. |
| `template_id` | yes | Required for stale/unknown template handling. |
| parent/tip reference | yes | Defines the block being extended. |
| height or next height | when available | Must be documented if exposed. |
| timestamp or bounds | yes | Must define miner timestamp behavior. |
| target/difficulty | yes | Must define representation and comparison. |
| compact target | if used | Must map exactly to 256-bit target. |
| coinbase/miner address | if supported | Must define validation rules. |
| merkle root or tx commitment | if txs included | Must define ordering and commitment. |
| header/preimage fields | yes | Must be deterministic. |
| protocol/template version | recommended | Useful for future compatibility. |

## Canonical preimage requirements

Later v2.2.16 implementation PRs must document:

- exact fields included in PoW.
- field order.
- byte encoding.
- endianness.
- hash function.
- nonce field width and placement.
- timestamp encoding and update rules.
- target comparison rule.
- whether `chain_id` is included directly in the preimage or only used for submit/network validation.

The CPU miner should be treated as the reference implementation for template parsing and preimage construction.

## Submit validation target

The node should return stable results for:

| Condition | Expected class |
| --- | --- |
| Valid submit | accepted |
| Missing template id | rejected |
| Unknown template id | `TEMPLATE_UNKNOWN` |
| Stale template id | `TEMPLATE_STALE` |
| Wrong chain id | `INVALID_CHAIN_ID` |
| Wrong parent/tip | rejected |
| Invalid nonce | `INVALID_NONCE` |
| Invalid timestamp | `INVALID_TIMESTAMP` |
| Invalid target | `INVALID_TARGET` |
| PoW hash above target | `POW_TOO_HIGH` |
| Duplicate block | `DUPLICATE_BLOCK` |
| Malformed payload | `MALFORMED_SUBMIT` |
| Oversized payload | `MALFORMED_SUBMIT` or documented equivalent |
| Unsupported template version | rejected with documented code |
| Internal failure | `INTERNAL_ERROR` |

## Miner diagnostics target

Miner logs or status output should expose, where practical:

- backend: CPU or GPU.
- worker count.
- hashrate.
- current template id.
- current target.
- accepted submits.
- rejected submits.
- stale submits.
- reconnect count.
- last node error.
- last submit time.

## Node diagnostics target

Node read-only diagnostics should expose, where practical:

- current mining template id.
- current target/difficulty.
- chain id.
- current tip.
- accepted mining submit count.
- rejected submit count.
- stale submit count.
- invalid PoW count.
- duplicate submit count.
- last submit error.
- last accepted block hash/time.

## Evidence target

v2.2.16 closeout should produce evidence under `evidence/v2.2.16/` showing:

- template fetch.
- valid submit or test-profile proof.
- stale template rejection.
- miner restart/reconnect.
- CPU miner reference behavior.
- optional GPU smoke status.
- release evidence summary.

## GPU policy

GPU mining is allowed in v2.2.16 only under these conditions:

- external miner only.
- optional feature flag.
- default build works without GPU dependencies.
- no pool logic.
- no consensus changes.
- CPU fallback remains available.
- GPU-found nonce/result is CPU-verified before submit.
- GPU smoke evidence skips cleanly when no GPU exists.
