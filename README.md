# PulseDAG v2.0.0-rc-final

Consolidated release-candidate snapshot before public testnet.

## Frozen decisions
- Miner remains an external standalone application.
- No pool logic inside the miner.
- Smart contracts remain disabled until the network completes **30 days of stable testnet burn-in**.
- Current block target is **60 seconds per block**.
- Node RPC keeps mining-template + submit APIs for the **external miner app**, but pool/accounting/payout surfaces are out of scope in this phase.

## Current priority order
1. pruning + snapshot
2. miner multi-thread
3. fine-grained difficulty retarget
4. bounded mempool
5. P2P optimization
6. pre-burn-in hardening
7. public testnet checklist
8. 30-day burn-in
9. only then contracts

## Quick start
```powershell
cargo build
cargo run -p pulsedagd
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address YOUR_ADDRESS --threads 4 --loop --sleep-ms 1500 --max-tries 50000
```

## P2P mode honesty note

PulseDAG currently exposes three distinct P2P intent levels in code and status:

- `memory-simulated`: pure in-process simulation with no libp2p swarm and no real network transport.
- `libp2p-dev-loopback-skeleton`: libp2p identity/topics are initialized, but message flow still uses an in-process loopback dispatcher plus synthetic swarm/bootstrap events. This is **not** a fully wired remote libp2p network.
- `libp2p-real`: reserved name for future real Swarm transport mode. At the moment, requesting it should fail fast instead of pretending the current skeleton is already real networking.

### Current operational limitation

The current libp2p work is still a development skeleton. Status and startup logs are intentionally explicit so operators do not mistake synthetic/bootstrap loopback events for verified remote peers or real swarm connectivity.

### What the node should and should not claim today

The node may report:
- effective mode labels such as `memory-simulated` or `libp2p-dev-loopback-skeleton`
- initialized topics, peer IDs, listen address intent, queued publishes, and synthetic loopback/bootstrap events

The node should **not** imply:
- fully wired remote peer sessions
- real libp2p swarm connectivity
- validated remote gossip propagation
- production-ready peer counts derived from fake bootstrap events alone
