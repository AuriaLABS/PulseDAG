# PulseDAG P2P Specification v2.2.11

v2.2.11 is the PulseDAG **P2P completion** documentation target. It describes the real libp2p flow required for a small private-testnet rehearsal. It does **not** claim v2.3.0 readiness, public mainnet readiness, smart-contract support, pool logic, or an embedded miner. Mining remains external through `pulsedag-miner` and the node mining RPCs.

## Architecture

### Runtime mode

Operators should use `libp2p-real` for real multi-host networking. In this mode `connected_peers` in `GET /p2p/status` represents real network peers, not simulated observations.

Node configuration for a rehearsal should set:

```bash
PULSEDAG_P2P_ENABLED=true
PULSEDAG_P2P_MODE=libp2p-real
PULSEDAG_P2P_MDNS=false
PULSEDAG_CHAIN_ID=pulsedag-rehearsal-v2-2-11
```

Use explicit bootnodes for cross-host or deterministic local testing:

```bash
pulsedagd \
  --network private \
  --rpc-listen 127.0.0.1:18080 \
  --p2p-listen /ip4/0.0.0.0/tcp/18181

pulsedagd \
  --network private \
  --rpc-listen 127.0.0.1:18081 \
  --p2p-listen /ip4/0.0.0.0/tcp/18182 \
  --bootnode /ip4/<NODE_A_IP>/tcp/18181
```

`--peer` is accepted as an alias of `--bootnode`.

### Chain-id isolation

Every P2P message carries `chain_id`. Nodes publish, subscribe, and accept messages only for their configured chain id. A mismatch is dropped before node acceptance and increments mismatch diagnostics such as `inbound_chain_mismatch_dropped` in `GET /p2p/status`.

All nodes in a rehearsal must use the same `PULSEDAG_CHAIN_ID` or `PULSEDAG_REHEARSAL_CHAIN_ID`; do not reuse data directories between different chain ids.

### Topics

Topic names are chain-id scoped:

| Topic             | Format              | Purpose                                             |
| ----------------- | ------------------- | --------------------------------------------------- |
| Block topic       | `<chain_id>-blocks` | Block announcements and block payload relay.        |
| Transaction topic | `<chain_id>-txs`    | Mempool transaction gossip.                         |
| Sync topic        | `<chain_id>-sync`   | Tip exchange and targeted block requests/responses. |

Check active topics with:

```bash
curl -fsS http://127.0.0.1:18080/p2p/topics
```

## Wire message types used by v2.2.11

| Message                                    | Main use                                            |
| ------------------------------------------ | --------------------------------------------------- |
| `BlockAnnounce { chain_id, hash }`         | Announces that a peer has a block.                  |
| `GetBlock { chain_id, hash }`              | Requests a full block by hash.                      |
| `BlockData { chain_id, block }`            | Returns a block, or `null`/`None` if not available. |
| `NewTransaction { chain_id, transaction }` | Gossips a full transaction for mempool acceptance.  |
| `GetTips { chain_id }`                     | Requests peer tips.                                 |
| `Tips { chain_id, tips }`                  | Returns known tips.                                 |
| `Reject` / `Error`                         | Diagnostic rejection/error surface.                 |

Additional compatibility variants (`NewBlock`, `NewBlockHash`, `InvBlock`, `Block`) can appear internally or in tests, but the operator-facing v2.2.11 completion path is announce/fetch/validate/relay for blocks, full transaction gossip for txs, and tip/block request exchange for sync.

## Block propagation

### Happy path

1. A node accepts a locally mined or peer-provided block.
2. The node publishes a `BlockAnnounce` on the block topic.
3. Peers that do not already have the block issue `GetBlock` on the sync path.
4. The holder replies with `BlockData`.
5. The receiving node validates the block before applying it.
6. If valid and new, the block is persisted/applied and eligible for rebroadcast.
7. Rebroadcast excludes duplicate/loop cases through seen-message and relay suppression.

### Validation

Receiving nodes validate block structure, parent availability, DAG consistency, and active PoW acceptance. If validation fails, the block is rejected and `last_rejected_peer_block_reason` is exposed in `GET /sync/status`. A PoW failure commonly appears as `invalid_pow` or another validation reason surfaced by the node.

### Missing parents

If a child block arrives before a parent, the node queues it as an orphan and records missing parents. It should request the missing parent(s) with `GetBlock` and retry orphan promotion after parents arrive.

Operator checks:

```bash
curl -fsS http://127.0.0.1:18081/sync/status
curl -fsS http://127.0.0.1:18081/sync/missing
curl -fsS http://127.0.0.1:18081/orphans
```

### Rebroadcast and duplicate suppression

Duplicate suppression applies to inbound and outbound block messages. Operators can inspect:

- `seen_message_ids`
- `inbound_duplicates_suppressed`
- `outbound_duplicates_suppressed`
- `block_outbound_duplicates_suppressed`
- `relay_loop_prevented`
- `duplicate_suppression.p2p_blocks` from `/p2p/status`
- `/p2p/propagation`

A healthy network may show some duplicate suppression; a storm is indicated by rapidly rising duplicate counters without height convergence.

## Transaction propagation

### Happy path

1. A transaction enters a node through `POST /tx/submit` or local wallet/miner tooling.
2. The node runs mempool admission checks.
3. If accepted, it publishes `NewTransaction` on `<chain_id>-txs`.
4. Peers run their own mempool acceptance checks.
5. Accepted transactions are retained and relayed once according to relay budgets.

### Rejection and duplicates

Invalid transactions are rejected before relay. Duplicate transactions are suppressed by txid/message id and should not be rebroadcast indefinitely.

Operator checks:

```bash
curl -fsS http://127.0.0.1:18080/mempool
curl -fsS http://127.0.0.1:18081/mempool
curl -fsS http://127.0.0.1:18080/p2p/status
```

Relevant fields include `tx_inbound_received`, `tx_inbound_accepted`, `tx_inbound_duplicate`, `tx_inbound_invalid`, `tx_relayed`, `tx_relay_suppressed_duplicate`, and `tx_outbound_duplicates_suppressed`.

## Sync and catch-up

### Tip exchange

1. A lagging or restarted node asks peers with `GetTips`.
2. Peers answer with `Tips` containing candidate tip hashes.
3. The node ranks candidates and selects a sync peer where possible.
4. Missing blocks are requested by hash with `GetBlock`.
5. Returned `BlockData` is validated and applied.

### Missing parent recovery

Missing parent recovery is targeted: orphaned children remain queued while their missing parent hashes are requested. The queue must shrink as parents arrive. If `pending_missing_parents` or `orphan_count` remains non-zero, use `/sync/missing` and logs to identify the stuck hash and the peer serving or failing to serve it.

### Restart catch-up

On restart, the node rebuilds or replays persisted state as needed, compares local state against peer tips, and enters catch-up stages exposed by `GET /sync/status`:

- `discovering`
- `recovering`
- `validating`
- `steady`
- `degraded`

A successful restart catch-up ends with all nodes showing the same `best_height` and compatible `selected_tip` or converged tips.

## Operator diagnostics endpoints

| Endpoint               | Use                                                                                    |
| ---------------------- | -------------------------------------------------------------------------------------- |
| `GET /health`          | Quick liveness and current height check used by rehearsal scripts.                     |
| `GET /status`          | Node, chain, snapshot, mempool, and P2P status summary.                                |
| `GET /p2p/status`      | Real peer count, topics, duplicate counters, recovery state, selected sync peer.       |
| `GET /p2p/peers`       | Peer list.                                                                             |
| `GET /p2p/topics`      | Subscribed chain-id scoped topics.                                                     |
| `GET /p2p/propagation` | Propagation and duplicate-suppression counters.                                        |
| `GET /sync/status`     | Catch-up stage, lag, missing parents, orphan count, last accepted/rejected peer block. |
| `GET /sync/missing`    | Pending block requests and missing parent details.                                     |
| `GET /orphans`         | Queued orphan blocks.                                                                  |
| `GET /mempool`         | Accepted transaction set.                                                              |

## Guardrails

- v2.2.11 is P2P completion for private-testnet preparation only.
- v2.3.0 remains the future readiness-decision milestone.
- Do not claim public mainnet readiness from this document.
- Do not add or imply smart contracts.
- Do not add or imply pool logic.
- Keep `pulsedag-miner` external to the node.
