# PulseDAG v2.2.3-ops-readiness

Consolidated v2.2 operator-readiness package for operational recovery, rebuild, restore, and maintenance workflows.

## Frozen decisions
- Miner remains an external standalone application.
- No pool logic inside the miner.
- Smart contracts remain disabled until the network completes **30 days of stable testnet burn-in**.
- Current block target is **60 seconds per block**.
- Node RPC keeps mining-template + submit APIs for the **external miner app**, but pool/accounting/payout surfaces are out of scope in this phase.

## PoW clarity references
- PoW current-path audit guide: `docs/POW_CURRENT_PATH.md` (what node validates today, provisional/dev labels, and upgrade boundaries).
- Canonical PoW spec for current public testnet: `docs/POW_SPEC_FINAL.md`.

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

## RPC read-side enrichments (additive)
The RPC data plane now includes additional read-only surfaces for explorer/indexer-style access, without changing consensus, miner placement, or pool scope:

- `GET /blocks/:hash/overview`  
  Returns block lineage and indexing metadata (`parent_hashes`, `child_hashes`, `txids`, `confirmations`, tip flags).
- `GET /txs/:txid/lookup`  
  Returns richer transaction lookup details for both mempool and confirmed transactions (`nonce`, raw input outpoints, outputs, confirmation depth when confirmed).
- `GET /address/:address/summary`  
  Returns confirmed UTXO balance plus mempool-aware pending movement (`pending_incoming`, `pending_outgoing`, `pending_net`, related mempool txids).

All existing endpoints remain available and unchanged in behavior.

## Node configuration profiles (v2.2)
`pulsedagd` supports profile-oriented defaults through `PULSEDAG_CONFIG_PROFILE`:

- `dev` (default): local development with in-memory P2P simulation (`PULSEDAG_P2P_ENABLED=false`, `PULSEDAG_P2P_MODE=memory`).
- `testnet`: testnet-friendly defaults with libp2p dev loopback mode enabled (`PULSEDAG_P2P_ENABLED=true`, `PULSEDAG_P2P_MODE=libp2p`).
- `operator` (alias: `staging`): operator-oriented defaults with real libp2p runtime mode (`PULSEDAG_P2P_ENABLED=true`, `PULSEDAG_P2P_MODE=libp2p-real`) and conservative pruning retention.

Example:
```bash
PULSEDAG_CONFIG_PROFILE=testnet cargo run -p pulsedagd
```

All existing explicit env overrides still take precedence over profile defaults.  
Example override on top of `operator`:
```bash
PULSEDAG_CONFIG_PROFILE=operator \
PULSEDAG_P2P_MODE=memory \
PULSEDAG_CHAIN_ID=my-custom-chain \
cargo run -p pulsedagd
```

Invalid `PULSEDAG_CONFIG_PROFILE` values now fail fast with a clear startup error listing supported profile names.

## Operator package index (v2.2)
- Runbook index (recovery orchestration, maintenance/self-check, snapshot restore, snapshot+delta rebuild, fast-boot/fallback interpretation, staging upgrade/rollback): `docs/runbooks/INDEX.md`
- Dashboard package: `docs/dashboard/README.md`
- Official dashboard definitions: `ops/dashboard/v2.2/official-dashboards.json`
- Official alert rules: `ops/dashboard/v2.2/official-alert-rules.json`

## P2P mode labels (honest status/log semantics)
- `memory-simulated`: fully in-process simulation mode.
- `libp2p-dev-loopback-skeleton`: development skeleton that uses libp2p types/runtime wiring but **does not** represent a real external peer network yet.
- `libp2p-real`: real Swarm-backed foundation path where `connected_peers` is only treated as true network connectivity when this mode is active.

The node startup logs and `/status`, `/p2p/status`, `/p2p/topology` endpoints now expose whether `connected_peers` should be interpreted as real network connectivity, including `connected_peers_semantics` (`real-network-connected-peers` vs `simulated-or-internal-peer-observations`).
See `docs/OPERATIONS_P2P.md` for operational guidance.

## Staging upgrade and rollback validation (v2.2)
- Upgrade runbook: `docs/runbooks/STAGING_UPGRADE.md`
- Rollback runbook: `docs/runbooks/STAGING_ROLLBACK.md`
- Validation helper: `scripts/staging/validate_upgrade_rollback.sh`

## Release artifact packaging and verification (v2.2)
- Release asset naming + checksum policy: `docs/release/ARTIFACTS.md`
- Staging upgrade runbook: `docs/runbooks/STAGING_UPGRADE.md`
- Staging rollback runbook: `docs/runbooks/STAGING_ROLLBACK.md`

## Burn-in and evidence for v2.2
- The CI workflow `Soak Smoke (short CI signal)` is intentionally a short regression signal, not a release burn-in claim.
- Real release burn-in for v2.2 requires an operated run with evidence collection per release policy.
- See `docs/BURN_IN_14D.md` and `docs/RELEASE_EVIDENCE.md` for process and required artifacts.

## v2.2 closeout package (release hygiene only)
- Final release closeout checklist: `docs/V2_2_CLOSEOUT_CHECKLIST.md`
- Burn-in execution guide: `docs/BURN_IN_14D.md`
- Evidence bundle/index: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`

This closeout package is explicitly limited to release hygiene and evidence organization (no consensus/miner/pool feature scope changes).
