# Mempool Policy v3 foundation

Issue: #1036. Launch authority remains #781 / #794.

## Status

This document records the deterministic mempool-policy contract as of the production fee-bound freeze. It does not complete #1036 and it does not freeze finite production expiry or wallet custody UX.

Replacement semantics for the currently frozen transaction protocol are explicit: **automatic RBF/replacement is disabled**. This is a final behavior for the current protocol, not a placeholder that may be activated by fee, fee rate, nonce, arrival order, submission identity, or the `replacement_enabled` policy field.

## Policy identity

The foundation policy has `version = 1` under the fingerprint domain `PulseDAG:mempool-policy:v3`.

The fingerprint commits, in order, to:

1. policy version (`u32`, little-endian),
2. minimum relay fee rate per 1000 canonical transaction bytes (`u64`, little-endian),
3. maximum transaction fee safety value (`u64`, little-endian),
4. maximum tracked transactions (`u64`, little-endian),
5. replacement-enabled flag (`u8`, currently false).

The compatibility regression vector remains:

- minimum relay fee rate: `0`,
- maximum transaction fee: `u64::MAX`,
- maximum transactions: `4096`,
- replacement enabled: `false`,
- SHA-256 fingerprint: `5bda9d47ff368e28e0f9e258e6a9b41e7cb9642b7798b3f7e86769b975ad4efe`.

The active production vector is:

- minimum relay fee rate: `1`,
- maximum transaction fee: `100000000`,
- maximum transactions: `4096`,
- replacement enabled: `false`,
- SHA-256 fingerprint: `fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8`.

The compatibility vector is preserved only for legacy/regression evidence. The active production constructor now freezes the public mempool fee bounds at the smallest non-zero relay floor and a one-coin absolute safety ceiling under the approved v3 8-decimal precision recorded in #1014. Any later change to a fingerprinted field changes policy identity and must be treated as a new policy freeze.

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

Conflicting transactions fail closed with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED` because the frozen transaction protocol explicitly disables automatic RBF/replacement. A caller-supplied `replacement_enabled=true` does not override the transaction protocol and is not an activation switch.

Any future protocol that introduces RBF must define a new explicit replacement contract and rerun affected transaction, mempool, wallet, P2P, restart, and launch evidence. Fee-bump rules and actual replacement eviction are therefore future-protocol work, not unresolved authorization rules for the current protocol.

## Deterministic ordering

The foundation preserves the existing first-seen ordering contract with txid as the deterministic tie-breaker. It does not silently switch template/admission order to fee priority.

## Frozen non-RBF conflict-package contract

The v3 admission layer uses one canonical read-only conflict classifier before legacy admission can mutate the mempool:

- `direct_conflict_txids` are the unique live mempool txids that spend any `OutPoint` also spent by the incoming transaction;
- direct conflicts are returned in lexicographic txid order, independent of `HashMap` or insertion order;
- `conflict_package_txids` is the direct set plus every live in-mempool descendant reachable recursively from any direct conflict;
- the conflict package is unique and lexicographically sorted, including shared descendants and fan-in/fan-out graphs without duplicate entries;
- unrelated ancestors or descendants are not added to the package;
- for retries that pass v3 policy preflight, an exact duplicate txid is excluded from replacement-conflict classification so legacy `Duplicate` handling remains unchanged after preflight;
- equivalent in-memory states, states rebuilt by mempool reconciliation, and states restored through the persisted RocksDB chain-state boundary must produce the same direct-conflict vector, conflict-package vector, and canonical mempool order;
- ordinary and protocol-aware v3 admission reject the same true conflict with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED` before mempool mutation, apart from intentional rejection accounting.

These rules classify conflicts deterministically and enforce the current protocol's no-RBF rule. They do not authorize fee bumping, conflict eviction, replacement-set mutation, or descendant replacement.

## Frozen transaction-protocol replacement semantics

The transaction protocol is authoritative over mempool policy:

- an exact canonical txid retry that passes v3 policy preflight remains `Duplicate` and is idempotent;
- a distinct transaction spending an already-reserved live mempool outpoint is a conflict and is rejected;
- a higher absolute fee does not authorize replacement;
- a higher canonical fee rate does not authorize replacement;
- a different or larger nonce does not authorize replacement;
- arrival/first-seen ordering does not authorize replacement;
- retry/submission identity does not authorize replacement;
- `replacement_enabled=true` does not authorize replacement;
- ordinary and protocol-aware v3 admission apply the same rejection before mempool mutation;
- read-only replacement assessment values are observations only and never an authorization bit.

The valid signed regression in `mempool_no_rbf_protocol_v3.rs` constructs both a legacy compatibility vector and an activated-v2 chain-bound vector. In both cases the conflicting incoming transaction pays a strictly higher total fee and fee rate and uses a larger nonce, yet it still receives `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`; the activated-v2 case is admitted with `ProtocolActivationIdentity::activated_v2`.

## Remaining #1036 work

Before #1036 can close, the project still needs at least:

- final bounded resource/expiry policy rather than compatibility-only limits;
- integrated golden vectors for the remaining final production resource decisions;
- exact-candidate policy identity recorded in #781/#794 evidence.

No launch GO is implied by this foundation.

## Live RPC admission bridge

The active production policy is evaluated by the protocol-aware RPC transaction admission path before durable mempool mutation. The compatibility vector remains available for regression coverage only. The production numeric values freeze the public mempool relay floor at `1` atomic unit per 1000 canonical signed bytes and the accepted transaction-fee ceiling at `100000000` atomic units (exactly one coin at 8 decimals). Explicit stricter caller-supplied policies still fail closed with stable `MEMPOOL_V3_*` codes. Existing capacity/backpressure and mempool-conflict rejections are translated to the same machine-readable policy namespace; replacement remains unauthorized under the frozen transaction protocol.

RPC responses preserve the pre-existing typed `classification` field (for example `conflict` and `mempool_full`) alongside the v3 `MEMPOOL_V3_*` machine code, so existing clients keep their rejection category while newer clients can consume the versioned policy code.

Exact-head validation for this bridge must run on top of the current `main` integration baseline so unrelated launch gates, including the fast-sync restore/rejoin regression, are present rather than silently skipped by an outdated branch base.

This bridge does not change consensus validation, replace package-aware eviction ordering, or close the still-open `Resource limits, eviction and expiry` scope.

## Production resource, eviction and expiry contract

Production mempool resources are versioned separately from `MempoolPolicyV3`, so freezing resource/expiry values does not change the already-frozen fee-policy identity. Resource policy v1 freezes:

- live transaction ceiling: `4096`;
- tracked spent-outpoint ceiling: `8192`;
- orphan transaction ceiling: `512`;
- canonical transaction size ceiling: `32768` bytes (`32 KiB`), measured by the same v1/v2/v3 canonical encoders used by mempool fee-rate accounting;
- live transaction maximum age: `1440` accepted-height steps.
- orphan transaction maximum age: the same `1440` accepted-height steps.

Resource-policy fingerprint: `759a2820217e8b2d897634745b1fffad347f9f72f62348bb9effd5f1b79034cf` under `PulseDAG:mempool-resource-policy:v1`. The active fee-policy fingerprint remains `fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8`.

The `32 KiB` canonical ceiling is intentionally below the existing `64 KiB` full P2P `NewTransaction` carrier ceiling. Exact tests grow representative output-heavy and input-heavy v1/v2/v3 transaction shapes to the canonical boundary and require the complete serialized carrier to remain below the frozen transport ceiling. The P2P wire limit itself is unchanged.

Expiry uses only the persisted canonical DAG logical clock (`dag.best_height`) and the #1081 boundary `current_height >= admission_height + max_age_blocks`. `1440` heights is nominally 24 hours at the frozen 60-second target interval, but it is a height policy, not a wall-clock timer. Missing legacy live age metadata, future admission heights and checked-add overflow retain fail-safe rather than inventing live age. Historical orphan entries have no old age field, so their missing orphan age is seeded once at the current best height and they can expire only after a full future 1440-height window.

Fresh production RPC/P2P admission rejects transactions above the canonical-size ceiling before live/orphan mutation with stable code `MEMPOOL_RESOURCE_TX_TOO_LARGE`. Restored oversized live roots are removed with all live descendants; restored oversized orphans are dropped with their orphan metadata. Production orphan promotion re-enters the same production admission wrapper, so it cannot bypass the size ceiling.

Every ordinary or protocol-aware mempool reconciliation applies the same production resource normalization after any protocol identity has been validated. Existing stricter runtime/test caps remain stricter; persisted limits above production are clamped. Expiry and over-capacity cleanup are deterministic and package-safe. The existing live incoming package scoring/replacement behavior is unchanged and RBF remains disabled.

A corrupt optional `mempool_admission_height_v1` RocksDB sidecar no longer invalidates an otherwise valid `CHAIN_STATE_KEY`: loading records a recovery counter and leaves age metadata empty, preserving the legacy fail-safe retain rule. `STORAGE_SCHEMA_VERSION` remains `1`.

This contract changes mempool relay/resource policy only. It does not change consensus transaction validity, monetary rules, signing/txid, P2P wire identity, mining consensus, or replacement/RBF semantics. #781/#794 launch authority remains separate.
