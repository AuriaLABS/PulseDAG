# PulseDAG Mining Protocol v3

Status: launch-candidate external miner contract for issue #1037. This document freezes the wire identity implemented by the RPC facade; it does not authorize mainnet launch. Launch authority remains #781 / #794.

## Stable endpoints

The endpoint paths and request shapes remain compatible:

- `POST /mining/template`
- `POST /mining/submit`

The template request remains:

```json
{"miner_address":"pulse1..."}
```

No new required request field is introduced, so existing CPU/GPU miner clients keep compiling and older JSON remains valid.

## Protocol identity

`protocol_version` is `3` at the external facade.

The externally visible template identity is:

```text
v3:<internal-template-id>
```

The internal suffix is preserved byte-for-byte so the facade can recover the lower-layer durable template identity after reconnect/restart without a lossy lookup table.

`job_id` is stable for a template:

```text
v3-job-<SHA3-256("pulsedag:mining:v3:job|" + template_id)>
```

`submit_id` is stable for an exact candidate:

```text
v3-submit-<SHA3-256("pulsedag:mining:v3:submit|" + template_id + "|" + block_hash)>
```

Golden vectors are frozen in `mining_submit_v3::tests::task37_protocol_identity_golden_vectors_are_frozen`.

## Bounded new-work notification / invalidation contract

Every template response is itself the latest bounded notification snapshot and includes:

- `work_sequence`: monotonic process-local work revision;
- `work_token`: SHA3-256 digest of network, height, selected tip, parents, target/difficulty, mempool fingerprint/count and programmable-fee-hook activation state;
- `work_change_reasons`: explicit lifecycle causes such as height, selected tip, parent set, target/difficulty, mempool or programmable-fee-hook state changes;
- `new_work_notification`: polling guidance;
- `invalidation`: the current selected tip, parent set, target/difficulty, height and token.

The notification contract is deliberately pull-based and bounded. The node retains no per-miner subscriber queue: `max_outstanding_snapshots` is exactly `1`, and miners may poll according to `poll_after_ms`. This prevents reconnecting or slow miners from accumulating unbounded new-work notifications.

## Submit finality and deterministic reconciliation

The v3 `finality` field is one of exactly:

- `accepted`
- `rejected`
- `stale`
- `unknown_finality`

`duplicate_block` maps to `accepted` finality because chain membership is authoritative even when the legacy compatibility field `accepted` reports a duplicate outcome.

The node checks chain membership for `block_hash` before any rebroadcast. If the block is already present, the response is `accepted_reconciled`. Within a process, completed submit responses are retained in a bounded reconciliation registry keyed by `submit_id`; replaying the same candidate returns the cached finality with `reconciled=true` and does not rebroadcast it. An `unknown_finality` replay therefore reconciles chain state first instead of blindly resubmitting.

Because the external `template_id` embeds the lower-layer template ID after `v3:`, reconnect/restart does not require a transient map merely to recover template identity. Activated protocol bindings still obey the lower-layer Task 28 identity rules; a miner must refresh work when that lower layer explicitly requires a fresh binding.

## Resource bounds

- maximum concurrent v3 submits: 64;
- maximum cached job observations: 4096;
- maximum cached submit reconciliation entries: 4096;
- maximum outstanding new-work notification snapshots per polling request: 1.

When the submit concurrency bound is reached, the response is a deterministic `submit_overloaded` rejection and the miner may retry the same `submit_id`.

## Template-to-submit telemetry

Template issuance records `external_mining_v3_job_issued` runtime events. Submit completion records `external_mining_v3_submit` with `submit_id`, `job_id`, `template_id`, `block_hash`, finality and `template_to_submit_ms` when the issuing job is still present in the bounded registry.

## Programmable-fee inclusion hook

The v3 template response publishes `programmable_fee_inclusion_hook` with the contract activation state and freezes the ordering policy as:

```text
topological-first-seen-with-txid-tiebreak-v1
```

Contract activation does not switch the mining RPC to a non-deterministic inclusion order; the hook's activation behavior is `preserve-ordering-policy`.

## Compatibility

The facade delegates actual block-template construction, protocol activation identity, PoW evaluation, atomic acceptance, persistence and P2P broadcast to the already-existing Task 28/29 handlers. This keeps consensus behavior out of the v3 wire-contract layer.

Legacy request JSON remains valid. The template ID returned by v3 must be submitted unchanged; the facade removes only the `v3:` transport prefix before handing the request to the lower protocol layer.

## Completion evidence

Focused tests are named with the `task37_` prefix and cover:

- versioned template ID round-trip;
- golden job/submit identity vectors;
- finality-state freeze;
- bounded reconciliation storage;
- retained legacy template request compatibility;
- selected-tip / parent / target / mempool invalidation reasons;
- deterministic work tokens;
- strict one-snapshot notification bounds.

CI entry point: `.github/workflows/task37-mining-protocol-v3.yml`.
