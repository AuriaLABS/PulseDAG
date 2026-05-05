# Roadmap v2.2.11 — P2P Completion

## Positioning in the Release Sequence

- **v2.2.10** = final PoW completion milestone.
- **v2.2.11** = P2P completion milestone (this document).
- **v2.2.12** = full private-testnet rehearsal across operators and runbooks.
- **v2.3.0** = official "complete private-testnet readiness" milestone.

This roadmap intentionally focuses on finishing real networking, propagation, and sync behavior needed before rehearsal. It does **not** redefine v2.3.0.

## Objective

Complete PulseDAG's real peer-to-peer networking and sync foundation so that a small private network can reliably discover peers, propagate blocks/transactions, recover from gaps, and catch up after restarts.

## In-Scope Deliverables (v2.2.11)

### 1) Real libp2p Multi-Node Connectivity

- Stable listen/advertise identity per node.
- Verified direct multi-node connectivity (minimum three nodes in rehearsal topology).
- Connection lifecycle handling (connect, disconnect, reconnect).
- Basic peer scoring/health signals exposed to operators.

### 2) Block Propagation Pipeline Completion

Implement and validate the end-to-end flow:

1. `block announcement`
2. `getblock` request from peers missing the block
3. `blockdata` response
4. local validation (including PoW validation as currently defined)
5. rebroadcast to eligible peers

Required outcomes:

- Idempotent handling of duplicate announcements.
- No infinite rebroadcast loops.
- Rejection path for invalid data with clear diagnostics.

### 3) Transaction Propagation

- Gossip/broadcast transactions across connected peers.
- Dedup handling for repeated transaction messages.
- Admission validation before relay.
- Observable metrics/logs for accepted vs rejected propagated transactions.

### 4) Tip Exchange

- Peer tip advertisement/request exchange.
- Selection of candidate better tip when remote chain state is ahead.
- Consistent local decision path for whether sync is needed.

### 5) Missing Block Resolution

- Detect parent/height/hash gaps during sync or propagation.
- Trigger targeted fetch for missing ancestors/segments.
- Resolve partial-order arrival (child before parent) safely.
- Abort/resume behavior with bounded retries and diagnostics.

### 6) Restart Catch-Up Basics

- On node restart, compare local head vs peer tips.
- Initiate catch-up from known gap point.
- Reach steady synchronized state without manual DB intervention in normal scenarios.

### 7) Peer Health, Reconnect, and Backoff

- Peer health status model (healthy/degraded/unreachable).
- Reconnect scheduling with bounded exponential backoff + jitter.
- Protection against rapid reconnect storms.
- Logging/metrics for connection churn and retry outcomes.

### 8) P2P Diagnostics and Operator Runbook

- Troubleshooting guide for common P2P failure modes:
  - peer not discoverable,
  - peer connected but no sync,
  - block propagation stalls,
  - persistent missing-block loops.
- Required commands/endpoints/log fields for diagnosis.
- "Known-good" baseline checks prior to rehearsal.

### 9) Three-Node Rehearsal Scripts

- Scripts/config profiles to run deterministic 3-node scenarios.
- Scenarios include:
  - normal block/tx propagation,
  - one node temporarily offline then catch-up,
  - missing-block fetch recovery,
  - restart catch-up behavior.
- Script outputs must produce artifacts usable in release validation.

## Out-of-Scope / Guardrails

The following are explicitly outside v2.2.11:

- No smart contracts.
- No smart-contract runtime.
- No pool logic inside miner.
- Miner remains external.
- Do not change PoW semantics unless strictly required by P2P validation correctness.
- Do not claim public mainnet readiness.
- Do not treat v2.2.11 as the official private testnet milestone.

## Exit Criteria for v2.2.11

v2.2.11 is complete when:

- Real multi-node libp2p connectivity is demonstrably stable.
- Block announcement -> fetch -> validation -> rebroadcast path works end-to-end.
- Transaction propagation is functional and observable.
- Tip exchange and missing-block resolution are operational.
- Restart catch-up basics work in three-node rehearsal.
- Peer reconnect/backoff behavior is implemented and measurable.
- Diagnostics + runbook + three-node rehearsal scripts are available and usable.

## Relationship to v2.2.12 and v2.3.0

- **v2.2.12** consumes v2.2.11 outputs and executes the **full private-testnet rehearsal** (operator flow, sustained scenarios, and reliability validation).
- **v2.3.0** remains the only milestone that represents **official complete private-testnet readiness**.

In short: **v2.2.11 builds the P2P foundation; v2.2.12 rehearses it fully; v2.3.0 declares readiness.**
