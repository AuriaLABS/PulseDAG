# v2.2.20 First 5N/4M Stress Evidence Record

Date: 2026-06-06

## Scope

This record documents the first measured `v2.2.20` `5N/4M` stress evidence after the technical PRs that added bounded stress diagnostics for the private `5N/4M` rehearsal path.

This is **not** a public-testnet readiness declaration, does **not** start burn-in, and does **not** change the `v2.2.20` hardening scope.

## v2.2.19 comparison baseline

The previous `v2.2.19` private `5N/4M` stress evidence closed as `OBSERVE_FAIL` with these stress signals:

| Metric | v2.2.19 `5N/4M` value |
|---|---:|
| Peer visibility | peers collapsed to `0` |
| Orphans | `512` |
| Pending missing parents | `512` |
| Final tips after quiescence | `4` |
| Result | `OBSERVE_FAIL` |

Interpretation: the v2.2.19 stress run did not prove recoverability after quiescence. The concrete blocking symptoms were peer drop-to-zero, orphan saturation, missing-parent backlog saturation, and divergent final tips.

## v2.2.20 first measured result

Evidence source: `v2_2_20_5n_4m_stress_observe_evidence.zip`, uploaded from the v2.2.20 stress observe workflow.

| Area | v2.2.20 first measured record |
|---|---|
| Runtime result | `FAIL` / stress retained as `OBSERVE_FAIL` |
| Commit | `6633962c07bb1ccfc8c9e15b8763faf0402f45a6` |
| Version | `pulsedagd 2.2.20` |
| Stage | `5N/4M stress` |
| Runtime duration | `800s` |
| Node count | `5` |
| Miner count | `4` |
| Evidence archive sha256 | `321e260bf57daf9c25106d9eac8bf7ec01172488ee7a6a4be29b6d23771d7a2e` |
| Artifact zip sha256 | `8574e1ae856be675773e3da4d42f022cc140cadd27869fa6aa5b2906670c6939` |

## Measured stress outcome

| Metric | v2.2.20 `5N/4M` value |
|---|---:|
| Accepted blocks | `1549` |
| Rejected blocks | `7754` |
| Peer visibility | `peer_count=0` on all 5 nodes |
| Orphans | `512` on all 5 nodes |
| Pending missing parents | `512` on all 5 nodes |
| Final tips after quiescence | `4` |
| Worst lag before quiescence | `19` |
| Worst lag after quiescence | `6` |
| Lag improved during quiescence | `true` |
| Total orphan count after quiescence | `2560` |
| Total pending missing parents after quiescence | `2560` |
| `missing_parents_entries` | `0` on all nodes |
| `inv_hashes_requested` | `0` on all nodes |

## Final node table

| Node | Height | Peer count | Orphans | Pending missing parents | Tip |
|---|---:|---:|---:|---:|---|
| n1 | `392` | `0` | `512` | `512` | `8ad38c81a120026661c48a51d3816038f4bb17654add2c0c76d518e06ca6fce1` |
| n2 | `394` | `0` | `512` | `512` | `4ebd45770ef9cc67d563035795dd6a599c69611604cd1c16cf6fd0b076fe41e5` |
| n3 | `389` | `0` | `512` | `512` | `59d913ffa1f2fe2a49ecead9196a776023246cee033f17ec6c6d696630c50b61` |
| n4 | `388` | `0` | `512` | `512` | `3b6abeae6ce6ae52aacda1cfe595003064f6c852f30793db17e4619794cf8e45` |
| n5 | `392` | `0` | `512` | `512` | `8ad38c81a120026661c48a51d3816038f4bb17654add2c0c76d518e06ca6fce1` |

## Peer lifecycle diagnostics

The run records useful new peer lifecycle diagnostics, but they show a semantic inconsistency that should be fixed next:

- `disconnect_reason_counts={}` on all nodes.
- `last_error_by_peer={}` on all nodes.
- n1 records `connection_established=4` and `inbound_connected=4`.
- n2/n3/n4/n5 each record `connection_established=1` and `outbound_connected=1`.
- Final state includes connected inbound/outbound records, but `/p2p/status` still reports `peer_count=0` and `ok=0`.
- Peer recovery entries mark peers as `connected=false` with `lifecycle_tier=recovering` and `last_error=null`.

Interpretation: the next v2.2.20 fix should reconcile peer accounting between connected swarm state, recovery lifecycle state, and the exposed `peer_count` / readiness semantics. Peers appear connected in lifecycle snapshots but unavailable for sync/readiness accounting.

## Sync/orphan recovery diagnostics

The stress run also shows backlog saturation with no final recovery drain:

- `orphan_count=512` on all nodes.
- `pending_missing_parents=512` on all nodes.
- `missing_parents_entries=0` on all nodes.
- `inv_hashes_requested=0` on all nodes.
- `sync_state` is `catching_up` or `requesting_blocks`.
- `catchup_stage=degraded` on all nodes.

Interpretation: the system is not fully idle, but the backlog is no longer tied to concrete missing-parent entries or inventory requests by final capture. The next fix should make orphan backlog reprocessing deterministic after miners stop and should explain whether saturated orphan entries are still actionable, stale, or should be pruned/evicted with explicit metrics.

## Improvement decision against v2.2.19

This measured v2.2.20 run **does not yet prove stress recovery improvement** over the v2.2.19 `5N/4M` observe failure.

Measured deltas:

| Decision signal | Result |
|---|---|
| Peer visibility not all-zero | `NO` |
| Orphan count below `512` or drains during quiescence | `NO` |
| Pending missing parents below `512` or drains during quiescence | `NO` |
| Final tips improve from `4` distinct tips | `NO` |
| Failure has more deterministic diagnostics than v2.2.19 | `YES` |
| Lag improves during quiescence | `YES` |

The useful improvement is diagnostic quality: the evidence now separates peer lifecycle state, recovery counters, orphan/pending backlog, quiescence lag, and stress miner activity. The runtime outcome remains an `OBSERVE_FAIL`.

## Required next work

Recommended next PR target:

`p2p/sync: reconcile peer accounting and reprocess orphan backlog after quiescence`

Required scope:

- explain why connected lifecycle records produce `peer_count=0`;
- align `peer_count`, peer recovery tier, and readiness semantics;
- add explicit transition reasons when peers enter `recovering` with no last error;
- trigger bounded orphan reprocess after miners stop or after missing-parent fetch settles;
- expose whether saturated orphan entries are actionable, stale, evicted, or waiting for unavailable parents;
- preserve `5N/1M` PASS;
- recover `5N/2M` before treating `5N/4M` as anything more than observe evidence.

## Current conclusion

`v2.2.20` has the required metric surface to compare against the v2.2.19 `OBSERVE_FAIL`. The first measured `5N/4M` run still fails with the same high-level stress outcome: peer visibility collapses to zero, orphan/missing-parent backlogs saturate at `512`, and final tips remain divergent after quiescence.

The next mandatory target is to restore `5N/2M` PASS, then use the same diagnostics to reduce or resolve the `5N/4M` observe failure.
