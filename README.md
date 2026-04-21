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
