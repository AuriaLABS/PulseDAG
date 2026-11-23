# Native-v3 pull synchronization for the **first** PulseDAG public network

Status: **architecture implemented in dormant transport/candidate controller; NOT a consensus or launch authorization**. Scope: #1465, #1466, #1410, #1367, #781.

## What Bitcoin, Ethereum and Kaspa do differently

- Bitcoin: header-first synchronization; requests exact block bodies separately, uses a download window, detects stalled peers and reassigns work. Reference: https://developer.bitcoin.org/devguide/p2p_network.html and https://github.com/bitcoin/bitcoin/blob/master/src/net_processing.cpp
- Ethereum: the `eth` protocol exposes separate GetBlockHeaders / GetBlockBodies requests with IDs and maintains state synchronization separately via `snap`. Reference: https://github.com/ethereum/devp2p/blob/master/caps/eth.md
- Kaspa: native BlockDAG header/context/UTXO work must survive pruning. In v2.1 its large IBD proof/header payloads are chunked; a DAG cannot be reconciled with a single selected-tip announcement. Reference: https://github.com/kaspanet/rusty-kaspa/releases

**PulseDAG root failure:** Task30 Core-v2.3 isolated 5N netns reaches 4 nodes height 60 and a returning fifth at height 46, with GetTips still flowing but almost no effective GetBlock/BlockData transfer. Increasing GetTips cadence (#1513) cannot fix a data-plane delivery/ancestry/acceptance gap. Do NOT bypass this with `gossipsub.add_explicit_peer` or weaken the fail-closed sync error gate. The legacy Core rehearsal is a **diagnostic**, not a backward-compatibility requirement: PulseDAG has never launched a prior network.

## Native-v3-only data path (first public testnet)

1. **Pin chain/genesis identity externally.** Independently derive the native-v3 genesis with nanosecond identity. Accept neither a remote-selected hash nor legacy v1/v2 compatibility identity.
2. **Discover the DAG frontier, not only the tip.** Authenticate/verify headers, exact parents, header hash and KHeavyHash. Do not treat a remote tip inventory as block acceptance.
3. **Schedule missing bodies by hash.** Use `NativeV3SyncRecovery` (new #1516) for a 256-hash bounded queue, up to 32 simultaneous requests / four per Noise peer, missing-parent-first scheduling and no duplicate in-flight hash. It reserves work atomically.
4. **Fetch over authenticated Noise request-response, never broadcast GetBlock.** Use the `NativeV3GetBlockCorrelationBookV1` (new #1516) on the **same** real Swarm `OutboundRequestId` to bind peer, locally minted request ID and hash. Dedicated 4 KiB request / bounded response. **The production Swarm remains HARD OFF until explicitly authorized.**
5. **Retry and switch providers.** Eight-second per-request TTL, bounded exponential backoff, prefer an alternate authenticated connected peer on the next attempt, finite five attempts; expired or disconnected peers release reservations. Authenticated Missing is a *peer-local* miss, not proof that the block never existed. Exhaustion remains NO-GO; cannot be hidden by a fresh tip timestamp.
6. **Stage complete native v3 candidates only.** Reject wrong chain/hash/Noise peer/PoW, invalid merkle/reward-body preflight, malformed parent ordering, oversize and memory abuse. 64 staged blocks / 32 MiB local buffer, **not** a network consensus-size rule. Do not insert candidate into monetary UTXO or accepted block DB.
7. **Parent-first consensus admission.** Stage children until their parents have been **independently accepted**. Only a trusted daemon callback after verified GD-01..08, Ed25519, UTXO fees, real reward maturity, target-weighted work, committed state root and atomic DB write can invoke `consensus_committed`. The planner checks parent readiness but **does not implement those economic checks**.
8. **Pruned ancestry:** once authenticated pruning checkpoints are independently specified (#1367), download and verify signed/anchored proof/state/chunks through a separate bounded path. Without proof do not mark a missing parent as present or treat a checkpoint as trusted.

```text
Authenticated Noise frontier / verified native headers
                         |
                 bounded pending hashes
                         |
            parent-first multipeer fetch queue
                         |
     exact OutboundRequestId <--> Noise GetBlock
                         |
             native-v3 PoW/body preflight
                         |
              untrusted staged candidate
                         |
         parents already CONSENSUS accepted?
                    /              \
                  no                yes
                  |                  |
          fetch missing parents     GD-08 + UTXO + fees + rewards
                                     |
                           atomic daemon accepted store
                                     |
                       acknowledge committed to scheduler
```

## What is implemented vs missing (never conflate)

**Implemented in PR #1516, still DRAFT:**
- Native-v3 exact wire-correlation/anti-replay, peer quotas/TTL and strict candidate parser.
- Parent-first bounded queue, finite retry and alternate-peer selection, staged candidate buffer with PoW validation, no progress declaration from mere network responses; tests exercise 32 genuine native-v3 PoW envelopes with multiparent fork and a simulated missing-provider/timeout, forged peer/hash and missing-parent gating.
- Exact-head Rustfmt, tests, Clippy and production hard-OFF CI *must pass before any merge*.

**Not implemented yet; mandatory before public testnet:**
- Binding the new planner + correlation book to production libp2p Swarm and selected native-v3 header/ancestor protocol.
- Real daemon **accepted native-v3** envelope storage/read path and serving/receipt on the actual Noise substream; the existing `direct_getblock_authority.rs` is legacy `ChainState<types::Block>`, NOT acceptable.
- Frozen GD-01..08 policy (K, selected parent, blue-work and ordering), signed UTXO/rewards, independent state roots, checkpoint authority.
- Five *real* v3 daemons / >=32 genuinely economically accepted mined blocks during isolation, deterministic rejoin/restart/prune, identical canonical tips, UTXO roots and supply, negative peer tests, exact-head CI and separate public-testnet GO.

No previously deployed network exists and no migration/version compatibility is required. The first deployment, after an explicit GO, must be **public native-v3 testnet**; do not deploy a preliminary private network. A clean task-only test or a clean 2-peer Noise candidate test is not equivalent to launch readiness.

**Hard gates:** `DIRECT_GETBLOCK_LIVE_REQUESTS_ENABLED=false`; deployed protocol `ProtocolSupport::Outbound`; monetary mining OFF; contracts `PULSEDAG_CONTRACTS_ENABLED=false`/INACTIVE; public testnet and mainnet NO-GO.
