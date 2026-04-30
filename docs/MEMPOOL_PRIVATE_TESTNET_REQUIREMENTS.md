# Mempool requirements for private testnet (v2.3.0)

This document defines the **minimum mempool behavior requirements** for the v2.3.0 private testnet milestone.

Scope: private testnet readiness only (bounded operation, deterministic behavior, and operator visibility), not fee-market optimization.

## 1) Capacity requirements

- **Max mempool transactions:** `4096`.
  - Rationale: keep memory pressure bounded and predictable during burn-in.
- **Max tracked spent outpoints:** `8192`.
  - Rationale: maintain duplicate/spend-conflict defenses with bounded in-memory indexes.

Nodes MUST reject new transactions when mempool capacity is reached unless an explicit eviction policy is later introduced.

## 2) Per-address policy (feasible baseline)

For v2.3.0 private testnet, per-address limits are **optional** and may be deferred if not yet implemented.

If enabled, the recommended baseline policy is:
- soft limit: `64` in-mempool transactions per source address;
- hard limit: `128` in-mempool transactions per source address (new tx rejected above this cap).

If not enabled, nodes MUST still enforce global mempool bounds and duplicate/spend-conflict protections.

## 3) Transaction admission and rejection requirements

At minimum, mempool admission MUST reject:
- duplicate transaction IDs (already present in mempool);
- already-confirmed transactions;
- transactions with missing inputs;
- transactions with duplicate inputs;
- transactions with zero-value outputs;
- structurally or cryptographically invalid transactions.

Duplicate and invalid transaction rejections SHOULD be surfaced to operators through runtime counters and/or logs.

## 4) Confirmed transaction cleanup

When a block is accepted, nodes MUST remove from mempool:
- every transaction included in the accepted block;
- any mempool entries that become invalid because their spend assumptions are no longer satisfiable after chain-state update.

A sanitize pass (`/mempool/sanitize`) MAY be used as a reconciliation mechanism, but confirmed-transaction cleanup MUST not depend solely on manual operator action.

## 5) Deterministic ordering requirements

For v2.3.0, mempool ordering MUST be deterministic for equivalent node state.

Minimum deterministic ordering rule:
1. sort by first-seen logical sequence (ascending), then
2. tie-break by txid (lexicographic ascending).

If first-seen metadata is unavailable, txid lexical ordering MUST be used as fallback to preserve deterministic behavior.

## 6) P2P propagation requirements (v2.3.0)

Using existing message types (`NewTransaction`, `GetBlock`, `GetBlocksFromHeight`, etc.), v2.3.0 nodes MUST:
- relay newly accepted transactions to connected peers once per acceptance event;
- suppress redundant relay for already-known txids;
- ignore or reject invalid relayed transactions without disconnecting healthy peers by default;
- avoid relay storms by deduplicating tx relay per peer/txid within a short suppression window;
- continue block-first correctness (block acceptance/reorg handling can invalidate previously relayed mempool entries).

Propagation SHOULD be best-effort and resilient to transient peer failures.

## 7) Required private testnet metrics

Private testnet validation MUST include, at minimum:
- `mempool_size` (current count);
- `mempool_capacity_max` (configured cap);
- `mempool_reject_total` split by reason (`duplicate`, `confirmed`, `missing_inputs`, `duplicate_inputs`, `zero_value_output`, `invalid_format_or_signature`, `capacity`);
- `mempool_admit_total`;
- `mempool_relay_total`;
- `mempool_relay_deduplicated_total`;
- `mempool_sanitize_runs`;
- `mempool_sanitize_removed_total`;
- `mempool_sanitize_removed_last_run`;
- `last_mempool_sanitize_unix`;
- `last_mempool_sanitize_ok`.

At least one burn-in evidence artifact SHOULD capture mempool pressure and rejection taxonomy over time.

## 8) Acceptance criteria for v2.3.0 private testnet

A build satisfies mempool readiness when all are true:
- global mempool bounds are enforced (`4096` tx cap);
- duplicate + invalid transaction rejection is active;
- confirmed transactions are cleaned from mempool after block acceptance;
- deterministic mempool ordering is observed for equal state;
- transaction relay is deduplicated and stable across peers;
- required mempool metrics are exposed and observable during burn-in.
