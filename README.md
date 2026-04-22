# PulseDAG v2.1.0-readiness

Consolidated v2.1 operator-readiness package prior to the public testnet cutover window.

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

## Operator package index (v2.1)
- Runbooks: `docs/runbooks/INDEX.md`
- Dashboard package: `docs/dashboard/README.md`

## P2P mode labels (honest status/log semantics)
- `memory-simulated`: fully in-process simulation mode.
- `libp2p-dev-loopback-skeleton`: development skeleton that uses libp2p types/runtime wiring but **does not** represent a real external peer network yet.
- `libp2p-real`: reserved label for a future/real libp2p networking mode where `connected_peers` reflects true network connectivity.

The node startup logs and `/status`, `/p2p/status`, `/p2p/topology` endpoints now expose whether `connected_peers` should be interpreted as real network connectivity.
See `docs/OPERATIONS_P2P.md` for operational guidance.

## Staging upgrade and rollback validation (v2.1)
- Upgrade runbook: `docs/runbooks/STAGING_UPGRADE.md`
- Rollback runbook: `docs/runbooks/STAGING_ROLLBACK.md`
- Validation helper: `scripts/staging/validate_upgrade_rollback.sh`

## Burn-in and evidence for v2.1
- The CI workflow `Soak Smoke (short CI signal)` is intentionally a short regression signal, not a release burn-in claim.
- Real release burn-in for v2.1 requires a 14-day operated run with evidence collection.
- See `docs/BURN_IN_14D.md` and `docs/RELEASE_EVIDENCE.md` for process and required artifacts.
