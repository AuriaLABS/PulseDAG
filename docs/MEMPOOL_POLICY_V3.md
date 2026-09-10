# Mempool Policy v3

Issue: #1036. Launch authority remains #781 / #794.

## Status

This document records the deterministic mempool-policy contract and the active production mempool/relay fee safety bounds. It does not complete #1036: the final production resource/eviction/expiry policy remains open.

Replacement semantics for the currently frozen transaction protocol are explicit: **automatic RBF/replacement is disabled**. This is a final behavior for the current protocol, not a placeholder that may be activated by fee, fee rate, nonce, arrival order, submission identity, or the `replacement_enabled` policy field.

The fee bounds in this document are mempool/relay policy, not consensus monetary validity. An otherwise-valid zero-fee transaction remains valid at the direct consensus transaction-validation layer even though the active public mempool/relay policy does not accept it.

## Policy identity

The policy has `version = 1` under the fingerprint domain `PulseDAG:mempool-policy:v3`.

The fingerprint commits, in order, to:

1. policy version (`u32`, little-endian),
2. minimum relay fee rate per 1000 canonical transaction bytes (`u64`, little-endian),
3. maximum transaction fee safety value (`u64`, little-endian),
4. maximum tracked transactions (`u64`, little-endian),
5. replacement-enabled flag (`u8`, currently false).

### Compatibility vector

The pre-freeze compatibility vector remains available for legacy/regression evidence and for tests that deliberately exercise custom policy values:

- minimum relay fee rate: `0`,
- maximum transaction fee: `u64::MAX`,
- maximum transactions: `4096`,
- replacement enabled: `false`,
- SHA-256 fingerprint: `5bda9d47ff368e28e0f9e258e6a9b41e7cb9642b7798b3f7e86769b975ad4efe`.

`MempoolPolicyV3::default()` continues to equal `compatibility_default()` so old regression construction is not silently reinterpreted as production policy.

### Active production fee-bound vector

The canonical active mempool/relay constructor is `MempoolPolicyV3::production_default()`:

- minimum relay fee rate: `1` atomic unit per 1000 canonical signed transaction bytes,
- maximum transaction fee: `100000000` atomic units,
- maximum transactions: `4096`,
- replacement enabled: `false`,
- SHA-256 fingerprint: `fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8`.

The minimum is the smallest non-zero integer relay floor under the existing integer fee-rate contract. The maximum is one coin under the v3 mainnet monetary policy's approved eight-decimal precision recorded by #1014. This is a relay/mempool safety ceiling intended to bound accidental or abusive high-fee submission; it is not a consensus supply, emission, burn, reward, or transaction-validity rule.

The `4096` transaction field is intentionally unchanged in this fee-only freeze. It must not be cited as completing the separate `Resource limits, eviction and expiry` gate. A later approved change to any fingerprinted field, including capacity, creates a different policy fingerprint and invalidates evidence tied to the old identity.

Wallet spend authorization remains independent. A wallet may impose tighter absolute-fee, fee-to-amount, input-count, or user-confirmation limits than the relay ceiling.

## Fee-bound semantics

The production relay boundary is exact:

- fee rate below `1` atomic unit per 1000 canonical signed bytes is rejected with `MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE` without inserting the transaction or reserving its spent outpoints; rejection counters may still increment as policy telemetry;
- an absolute transaction fee of exactly `100000000` atomic units is within the maximum-fee bound;
- a fee of `100000001` atomic units or higher is rejected with `MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE` without inserting the transaction or reserving its spent outpoints; rejection counters may still increment as policy telemetry.

Because fee rate uses integer floor division, a nominal fee of one atomic unit is not automatically sufficient for every transaction shape: the canonical signed size still determines whether the resulting integer fee rate reaches the relay floor.

Consensus validation remains separate. The production fee policy does not alter transaction serialization, signatures, txids, input/output conservation, block validation, miner reward accounting, monetary emission, fee disposition, burn, genesis, or programmability economics.

Inbound P2P transaction admission uses the same production policy constructor as RPC admission and fee estimation. Activated-v2 nodes route P2P admission through the protocol-aware wrapper with the startup-restored protocol identity; legacy-mode nodes use the non-protocol wrapper. This changes no P2P wire format or consensus transaction validity.

## Canonical fee rate

Fee rate is integer-only:

`floor(fee * 1000 / canonical_signed_transaction_size_bytes)`

The multiplication and retained fee-rate value use `u128`, so a valid `u64::MAX` fee can still be assessed by compatibility/custom-policy tests without arithmetic wrap. Canonical size is derived from the already-frozen signed transaction serialization for the transaction version:

- v1: legacy canonical signed bytes,
- v2: chain-bound canonical signed bytes,
- v3: chain-bound hybrid-PQC canonical signed bytes.

Unsupported transaction versions fail closed. This policy layer does not alter txid, signing, validation, or consensus serialization.

## Stable rejection codes

The policy reserves these machine-readable codes:

- `MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE`
- `MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE`
- `MEMPOOL_V3_CAPACITY_BACKPRESSURE`
- `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`
- `MEMPOOL_V3_POLICY_IDENTITY_MISMATCH`

Conflicting transactions fail closed with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED` because the frozen transaction protocol explicitly disables automatic RBF/replacement. A caller-supplied `replacement_enabled=true` does not override the transaction protocol and is not an activation switch.

Any future protocol that introduces RBF must define a new explicit replacement contract and rerun affected transaction, mempool, wallet, P2P, restart, and launch evidence. Fee-bump rules and actual replacement eviction are therefore future-protocol work, not unresolved authorization rules for the current protocol.

## Deterministic ordering

The policy preserves the existing first-seen ordering contract with txid as the deterministic tie-breaker. It does not silently switch template/admission order to fee priority.

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

## Live RPC admission and estimator binding

The active production policy is evaluated by the protocol-aware RPC transaction admission path before durable mempool mutation. The same `production_default()` identity is exposed through `/policy` and supplied to the deterministic fee estimator, so wallet-visible bounds and estimator fingerprint cannot drift from live RPC admission by using separate literal policy construction.

RPC responses preserve the typed `classification` field alongside the stable `MEMPOOL_V3_*` machine code. The fee-estimator remains observational and does not change admission ordering, package/conflict policy, or eviction behavior.

## Remaining #1036 work

Before #1036 can close, the project still needs at least:

- final bounded production resource/eviction/expiry policy rather than compatibility-only capacity and caller-driven expiry primitives;
- integrated golden vectors/evidence for the final production resource decision and resulting policy identity;
- exact-candidate policy identity carried into the #781/#794 launch evidence bundle.

No #781/#794 launch GO is implied by this fee-bound freeze.
