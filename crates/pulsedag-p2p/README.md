# pulsedag-p2p

Peer-to-peer networking and distributed consensus for PulseDAG.

## Purpose

This crate provides:
- **libp2p** integration for P2P communication
- **Gossipsub** protocol for block/transaction propagation
- **Kademlia DHT** for peer discovery
- **mDNS** for local network discovery
- **Message serialization** (`serde`, `serde_json`)
- **Tokio** async runtime integration

## Dependencies

- `pulsedag-core` — core data structures
- `libp2p` — P2P networking library with protocols:
  - `gossipsub` — pubsub messaging
  - `identify` — peer identification
  - `kad` — Kademlia DHT
  - `mdns` — multicast DNS discovery
  - `tcp` — TCP transport
  - `noise` — encryption protocol
  - `yamux` — stream multiplexing
  - `ping` — heartbeat
- `tokio` — async runtime (sync, rt, macros, time)
- `serde`, `serde_json` — message serialization

## Key Modules

- `network` — P2P node initialization and control
- `protocol` — Custom message protocols
- `discovery` — Peer discovery and DHT operations
- `gossip` — Block/transaction propagation
- `errors` — Network-specific errors

## Usage Example

```rust
use pulsedag_p2p::PeerNetwork;

let peer = PeerNetwork::new()?;
peer.start().await?;
peer.subscribe_gossip("blocks").await?;
```

## Configuration

libp2p protocols are configured with hardened defaults:
- **TCP port:** Configurable (default: 30333)
- **Gossipsub flood:** Limited to prevent amplification
- **DHT replication:** Standard Kademlia settings

## Tests

Run with:
```bash
cargo test -p pulsedag-p2p
```

## Warnings

- **Firewall rules:** Ensure libp2p ports (TCP) are open/firewalled appropriately.
- **Network partition:** Kademlia DHT recovery is eventual; expect temporary partitions.
- **Message size limits:** Gossipsub and custom protocols have max message sizes; chunk large data.
- **Connection limits:** Monitor active peer connections to prevent resource exhaustion.
