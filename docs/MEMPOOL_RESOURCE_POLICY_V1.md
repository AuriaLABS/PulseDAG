# Mempool Resource Policy V1

`MempoolResourcePolicyV1` is a deterministic **non-consensus** resource/retention contract. It is intentionally separate from `MempoolPolicyV3`, so the production fee-policy identity and fingerprint remain unchanged.

## Frozen production values

| Field | Value |
| --- | ---: |
| policy version | `1` |
| live transactions | `4096` |
| spent outpoints | `8192` |
| orphan transactions | `512` |
| canonical transaction bytes | `65536` (64 KiB) |
| live max age | `1440` best-height blocks |
| orphan max age | `1440` best-height blocks |
| expiry boundary version | `1` |
| fingerprint | `2fb3ff1fb784703fa197a362d1a42e9312a3b2ea6a1b3b9c9406ce1c2c588b3f` |

The canonical-size limit is 64 KiB rather than the earlier 128 KiB planning ceiling because the real transaction P2P carrier already rejects messages larger than `MAX_TX_MESSAGE_BYTES = 64 * 1024`. The mempool limit therefore never advertises a looser transaction resource contract than relay transport.

## Logical expiry clock

Expiry uses only `ChainState.dag.best_height`; wall-clock time is never an input. With the current 60-second target block interval, 1440 blocks is a nominal 24-hour retention window. Exact V1 boundary semantics are:

`current_best_height >= admission_height + max_age_blocks`

The addition is checked. Overflow cannot make an entry expire. A live parent expiry removes its in-mempool descendants as one package so no child is stranded.

## Restore and migration

Admission heights are operational metadata and are excluded from the serialized `CHAIN_STATE` layout. Live ages remain in `mempool_admission_height_v1`; orphan ages use the separate backward-compatible `mempool_orphan_admission_height_v1` sidecar. `STORAGE_SCHEMA_VERSION` remains `1`.

A missing age for an otherwise valid restored live/orphan transaction is seeded at the current `best_height` before expiry is evaluated. This is deliberately conservative: old snapshots are not expired merely because age metadata was absent. Stale sidecar txids are ignored. Corrupt optional age sidecars are ignored independently, leave the valid `CHAIN_STATE` usable with an empty corresponding age map, and emit a runtime warning event; startup production normalization then seeds current logical ages.

## Capacity and eviction

Fresh and restored production state is normalized to the frozen live/spent/orphan caps. Existing live package-aware pressure semantics remain in place. Resource normalization removes descendants with any live victim and uses deterministic fee/txid tie-breaking when a restored state is already above a hard cap. Orphan overflow is also deterministic and independent of HashMap or arrival iteration.

All removals clean the associated live/orphan indexes and age metadata, and the spent-outpoint index is rebuilt from the retained live set.

## Oversize transactions

Canonical transaction size is a mempool/relay resource rule, **not consensus validity**. Production admission rejects an oversized transaction before insertion with machine-readable code `MEMPOOL_RESOURCE_V1_TRANSACTION_TOO_LARGE`. RPC preserves that code and does not relabel it as `mempool_full`. Ordinary and activated-v2 admission share the same production wrapper.

## Mining visibility

Production normalization runs on startup reconciliation, production admission, best-height block progress, and immediately before mining/template mempool reads. Consequently expired or oversize entries cannot be selected into a new template after they become ineligible.

## Fee-policy separation

This contract adds no fields to `MempoolPolicyV3`, changes no fee vector, and changes no fee fingerprint. The production fee fingerprint remains:

`fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8`
