# PulseDAG v2.2.8 current status

Consolidated v2.2.8 foundation-closeout status for the PoW/mining/P2P groundwork that precedes the full private-testnet milestone in v2.3.0.

## Current v2.2.8 status
- Repository workspace version is aligned to **v2.2.8**.
- Workspace crates and apps inherit the workspace package version through `version.workspace = true`.
- v2.2.8 closes the current PoW/mining/P2P foundation and keeps the full private-testnet milestone assigned to **v2.3.0**.
- External miner architecture remains unchanged; pool/accounting/payout logic stays out of scope.
- Smart contracts remain disabled until **30 stable testnet burn-in days** are completed.
- Current chain target remains **60 seconds per block**.

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
1. Close v2.2.8 release hygiene and evidence.
2. Validate the external miner template/submit smoke path.
3. Harden the real P2P path for private-testnet execution.
4. Validate multi-node block and transaction propagation.
5. Validate sync/catch-up and delayed node recovery.
6. Enforce bounded mempool behavior under burst conditions.
7. Run snapshot, prune, restore, and recovery drills.
8. Prepare private-testnet scripts, dashboards, and operator runbooks.
9. Complete private-testnet burn-in evidence.
10. Only then move toward contracts.

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
- `GET /blocks/:hash/transactions`  
  Returns deterministic paged transactions scoped to a confirmed block with explicit context flags per entry (`context`, `is_confirmed`, `is_mempool`).
- `GET /blocks`, `/blocks/recent`, `/blocks/page`  
  Now share a stable sort order and common pagination metadata (`total`, `limit`, `offset`, `has_more`) to make indexer paging deterministic.
- `GET /txs/:txid/lookup`  
  Returns richer transaction lookup details for both mempool and confirmed transactions (`nonce`, raw input outpoints, outputs, confirmation depth when confirmed), including explicit `is_mempool`/`is_confirmed` flags.
- `GET /txs/activity`  
  Returns a deterministic mixed activity stream across mempool + confirmed transactions with explicit context metadata and block linkage for confirmed entries.
- `GET /txs`, `/txs/recent`, `/txs/page`  
  Now share stable fee/txid ordering and common pagination metadata (`total`, `limit`, `offset`, `has_more`) for predictable bounded lookups.
- `GET /address/:address/summary`  
  Returns confirmed UTXO balance plus mempool-aware pending movement (`pending_incoming`, `pending_outgoing`, `pending_net`, related mempool txids`) with explicit mempool accounting marker.
- `GET /address/:address/activity`  
  Returns deterministic address-level movement entries across mempool and confirmed contexts with explicit `direction`, net value, and context flags for explorer timelines.

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
- Release asset naming + checksum policy for both standalone binaries (`pulsedagd` and external `pulsedag-miner`): `docs/release/ARTIFACTS.md`
- Release publish output now includes a generated `INSTALL-VERIFY.md` with per-target unpack + binary smoke snippets derived from shipped manifests.
- Repeatable standalone operator smoke helper (external miner flow): `scripts/release/standalone_operator_smoke.sh`
- Staging upgrade runbook: `docs/runbooks/STAGING_UPGRADE.md`
- Staging rollback runbook: `docs/runbooks/STAGING_ROLLBACK.md`

## Burn-in and evidence (historical v2.2.5/v2.2.6 + current v2.2.8)
- The CI workflow `Soak Smoke (short CI signal)` is intentionally a short regression signal, not a release burn-in claim.
- Real release burn-in requires an operated run with evidence collection per release policy.
- See `docs/BURN_IN_14D.md` and `docs/RELEASE_EVIDENCE.md` for process and required artifacts.

## Historical and current closeout package
- v2.2.8 closeout checklist: `docs/checklists/V2_2_7_CLOSEOUT.md`
- v2.2.6 closeout checklist: `docs/checklists/V2_2_6_BURNIN_CLOSEOUT.md`
- v2.2.5 closeout checklist (historical evidence): `docs/checklists/V2_2_5_BURNIN_CLOSEOUT.md`
- Burn-in execution guide: `docs/BURN_IN_14D.md`
- Evidence bundle/index: `docs/RELEASE_EVIDENCE.md`
- Runbook index: `docs/runbooks/INDEX.md`

This closeout package is explicitly limited to release hygiene and evidence organization. Full private-testnet execution remains scoped to v2.3.0.
