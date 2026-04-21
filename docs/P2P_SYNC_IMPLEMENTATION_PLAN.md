# P2P + Sync Implementation Plan (rc2)

## Goals
- make PulseDAG usable across multiple real nodes
- keep miner external and simple
- avoid touching contracts until the 30-day burn-in is completed

## Phase 1: peer session and topology
- peer identity
- bootstrap peers
- topic subscriptions for blocks and txs
- topology snapshot endpoint
- peer connection/disconnection journal events

## Phase 2: gossip traffic
- block gossip topic
- transaction gossip topic
- dedup cache for txids and block hashes
- drop already-known blocks/txs before deeper processing

## Phase 3: request/response sync
- request missing block by hash
- request tip set
- request height/window summary
- sync lag counters in runtime

## Phase 4: orphan handling across peers
- orphan queue survives peer-delivered out-of-order traffic
- when parents arrive, reprocess blocked orphans
- journal orphan adoption and orphan expiry

## Phase 5: burn-in readiness
- two-node smoke
- three-node gossip smoke
- restart one node and verify replay + catch-up
- long-running journal counters for duplicates, invalid blocks, stale templates, orphan growth

## Must-have endpoints in this phase
- GET /p2p/topology
- GET /p2p/traffic
- GET /sync/lag
- POST /sync/request-block
- POST /sync/request-tips
- GET /runtime/events?kind=p2p

## Acceptance criteria before testnet burn-in
- block mined on node A reaches node B
- node B rejects duplicates cleanly
- node C can restart and catch up
- orphan queue remains bounded under out-of-order delivery
- runtime alerts stay low over repeated smoke runs
