# Mempool Policy v3 foundation

Issue: #1036. Launch authority remains #781 / #794.

## Status

This document records the first **non-activating** deterministic mempool-policy foundation. It does not complete #1036 and it does not freeze final mainnet numeric fee policy, RBF semantics, expiry, fee estimation, or wallet UX.

## Policy identity

The foundation policy has `version = 1` under the fingerprint domain `PulseDAG:mempool-policy:v3`.

The fingerprint commits, in order, to:

1. policy version (`u32`, little-endian),
2. minimum relay fee rate per 1000 canonical transaction bytes (`u64`, little-endian),
3. maximum transaction fee safety value (`u64`, little-endian),
4. maximum tracked transactions (`u64`, little-endian),
5. replacement-enabled flag (`u8`, currently false).

The compatibility-first vector is:

- minimum relay fee rate: `0`,
- maximum transaction fee: `u64::MAX`,
- maximum transactions: `4096`,
- replacement enabled: `false`,
- SHA-256 fingerprint: `5bda9d47ff368e28e0f9e258e6a9b41e7cb9642b7798b3f7e86769b975ad4efe`.

These defaults intentionally avoid changing current admission behavior. A later exact-candidate freeze must explicitly choose and record any finite production fee bounds.

## Canonical fee rate

Fee rate is integer-only:

`floor(fee * 1000 / canonical_signed_transaction_size_bytes)`

The multiplication and retained fee-rate value use `u128`, so a valid `u64::MAX` fee cannot wrap or be rejected merely because the scaled rate exceeds `u64`. Canonical size is derived from the already-frozen signed transaction serialization for the transaction version:

- v1: legacy canonical signed bytes,
- v2: chain-bound canonical signed bytes,
- v3: chain-bound hybrid-PQC canonical signed bytes.

Unsupported transaction versions fail closed. This policy layer does not alter txid, signing, validation, or consensus serialization.

## Stable rejection codes

The foundation reserves these machine-readable codes:

- `MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE`
- `MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE`
- `MEMPOOL_V3_CAPACITY_BACKPRESSURE`
- `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`
- `MEMPOOL_V3_POLICY_IDENTITY_MISMATCH`

Conflicting transactions remain fail-closed even if a caller supplies a policy object with `replacement_enabled=true`. The non-RBF conflict-package classification is frozen below, but replacement is not authorized until fee-bump requirements, replacement-set eviction, descendant replacement semantics, restart behavior for an actual replacement, and transaction-protocol interaction are separately frozen.

## Deterministic ordering

The foundation preserves the existing first-seen ordering contract with txid as the deterministic tie-breaker. It does not silently switch template/admission order to fee priority.

## Frozen non-RBF conflict-package contract

The v3 admission layer uses one canonical read-only conflict classifier before legacy admission can mutate the mempool:

- `direct_conflict_txids` are the unique live mempool txids that spend any `OutPoint` also spent by the incoming transaction;
- direct conflicts are returned in lexicographic txid order, independent of `HashMap` or insertion order;
- `conflict_package_txids` is the direct set plus every live in-mempool descendant reachable recursively from any direct conflict;
- the conflict package is unique and lexicographically sorted, including shared descendants and fan-in/fan-out graphs without duplicate entries;
- unrelated ancestors or descendants are not added to the package;
- an exact duplicate txid is excluded from replacement-conflict classification so the historical `Duplicate` precedence remains unchanged;
- equivalent in-memory states, states rebuilt by mempool reconciliation, and states restored through the persisted RocksDB chain-state boundary must produce the same direct-conflict vector, conflict-package vector, and canonical mempool order;
- ordinary and protocol-aware v3 admission must reject the same true conflict with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED` before mempool mutation, apart from intentional rejection accounting.

These rules classify conflicts only. They do **not** authorize fee bumping, conflict eviction, replacement-set mutation, descendant replacement, or RBF. `replacement_enabled=true` remains fail-closed under this contract.

## Remaining #1036 work

Before #1036 can close, the project still needs at least:

- exact production minimum/maximum fee policy and fee estimation,
- bounded expiry semantics without nondeterministic restart behavior,
- explicit RBF/replacement semantics, including fee-bump and authorized replacement/eviction rules,
- deterministic restart reconstruction and DAG-reordering reconciliation evidence,
- wallet/RPC integration of stable reason codes,
- integrated golden vectors for final production fee/replacement decisions beyond the frozen conflict/reconcile/restart and equivalent-state package-eviction vectors,
- exact-candidate policy identity recorded in #781/#794 evidence.

No launch GO is implied by this foundation.

## Live RPC admission bridge

The compatibility policy is now evaluated by the protocol-aware RPC transaction
admission path before durable mempool mutation. Default numeric values preserve the
existing fee behavior and existing package-aware eviction engine. Explicit stricter
policies fail closed with stable `MEMPOOL_V3_*` codes. Existing capacity/backpressure
and mempool-conflict rejections are translated to the same machine-readable policy
namespace; replacement remains unauthorized and no RBF semantics are activated.

RPC responses preserve the pre-existing typed `classification` field (for example
`conflict` and `mempool_full`) alongside the v3 `MEMPOOL_V3_*` machine code, so
existing clients keep their rejection category while newer clients can consume the
versioned policy code.

Exact-head validation for this bridge must run on top of the current `main` integration
baseline so unrelated launch gates, including the fast-sync restore/rejoin regression,
are present rather than silently skipped by an outdated branch base.

This bridge does not freeze production fee numbers, change consensus validation,
replace package-aware eviction ordering, or complete restart/reorder/RBF/estimation
scope tracked by #1036.
