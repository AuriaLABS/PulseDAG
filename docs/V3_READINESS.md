# PulseDAG v3.0.0 readiness gates

PulseDAG must not advance to v3.0.0 based on intuition. The `/readiness` endpoint is the measurable decision surface for v3 readiness. It combines node health, consensus, DAG, PoW, P2P, storage, mempool, mining, and replay indicators into actionable `pass`, `warn`, `fail`, or `unknown` results.

## Endpoint contract

Call either compatibility or versioned routes:

- `GET /readiness`
- `GET /api/v1/readiness`

The response includes:

- `overall_status`: aggregate status across all readiness categories.
- `ready_for_v3`: `true` only when every category is `pass`.
- `ready_for_release`: compatibility boolean retained for existing automation; it is `false` only when blockers exist.
- `categories`: per-category status plus human-readable reasons.
- `metrics`: raw decision inputs for dashboards and release evidence.
- `blockers`: flattened `fail` reasons.
- `warnings`: flattened `warn` and `unknown` reasons.

Status values are intentionally small and stable:

| Status | Meaning | v3 gate impact |
| --- | --- | --- |
| `pass` | The category is inside its v3 gate. | Allowed. |
| `warn` | The node can continue, but operators must review evidence before v3. | Blocks automatic readiness; may be waived only with documented evidence. |
| `fail` | The category violates a hard v3 gate. | Blocks v3. |
| `unknown` | The node lacks enough signal to prove readiness. | Blocks automatic readiness until observed or explicitly waived. |

## Required readiness categories

### consensus

Hard gate:

- Genesis must be present in the in-memory DAG.
- A deterministic selected tip must be available.

Warning gate:

- Rejected blocks since startup should not exceed accepted blocks.

### dag

Hard gate:

- In-memory DAG block hashes must match persisted block hashes.
- At least one active DAG tip must exist.

Warning gate:

- Orphan blocks waiting for missing parents require operator review.

### pow

Warning gate:

- Any invalid PoW block observed since startup must be explained in release evidence.

### p2p

Warning/unknown gate:

- P2P disabled is `unknown` because public/private network behavior has not been observed.
- P2P enabled with zero connected peers is `warn`.
- At least one connected peer is required for an unqualified `pass`.

### storage

Hard gate:

- Persisted block set must match the in-memory DAG.

Warning gate:

- Snapshot metadata should exist.
- Storage last commit height should be at least the node best height.

### mempool

Hard gate:

- Mempool must not be at capacity.

Warning gate:

- Mempool pressure at or above 75% requires review.

### mining

Unknown/warning gate:

- A node with no observed mining templates or submissions is `unknown` for mining readiness.
- Any mined-block rejection or external mining submit rejection is `warn` and must be explained.

### replay

Hard gate:

- Startup consistency issues or a failed self-audit block v3.

Warning gate:

- Startup replay or fallback recovery is acceptable only with documented evidence that the replay path was expected and deterministic.

## Readiness metrics

The endpoint exposes these raw metrics for dashboards and release notes:

| Metric | Meaning |
| --- | --- |
| `accepted_blocks` | Blocks accepted by the node since startup. |
| `rejected_blocks_by_reason` | Count of rejected block/submission reasons such as `invalid_pow`, `missing_parent`, `stale_template`, and `duplicate_block`. |
| `orphan_count` | Current number of queued orphan blocks. |
| `selected_tip` | Deterministic preferred tip hash, when available. |
| `best_height` | Current in-memory DAG best height. |
| `p2p_peer_count` | Current connected P2P peer count. |
| `storage_last_commit_height` | Snapshot metadata height, falling back to persisted block height. |
| `state_root` | Deterministic UTXO state root computed from current in-memory state. |

## Release decision rule

A v3.0.0 readiness decision requires all of the following:

1. `/readiness.overall_status` is `pass` on each release-candidate node used for evidence.
2. `/readiness.ready_for_v3` is `true`.
3. `rejected_blocks_by_reason` has no unexplained non-zero production/private-testnet rejection counts.
4. Storage and DAG gates pass after restart, replay, and snapshot restore drills.
5. P2P gates pass with the intended topology, not only a single-node development process.
6. Mining gates pass with the intended miner mode or are explicitly marked out-of-scope for that deployment.
7. All warnings or unknowns, if any are waived for a rehearsal, are recorded in release notes and do not become a v3.0.0 readiness claim.

v2.2.14 provides observability and scoring. It does not by itself declare v3.0.0 readiness.
