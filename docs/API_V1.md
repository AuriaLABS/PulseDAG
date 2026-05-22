# PulseDAG API v1

PulseDAG v2.2.14 introduces a stable public API namespace at `/api/v1` as the compatibility foundation for v3.0.0 clients. The intent is to give explorers, wallets, miners, and operators predictable URLs while preserving practical aliases for existing integrations.

## Version response

`GET /api/v1` and `GET /api/v1/version` return the API namespace metadata:

```json
{
  "ok": true,
  "data": {
    "api_version": "v1",
    "stable_prefix": "/api/v1",
    "release_version": "v2.2.14",
    "stage": "v2.2-readiness"
  },
  "error": null,
  "meta": null
}
```

## Public stable endpoints

The v1 namespace exposes read-mostly public chain, explorer, wallet-broadcast, miner, sync-status, and observability endpoints. Existing top-level routes remain compatibility aliases where practical.

| Stable route | Compatibility alias | Purpose |
| --- | --- | --- |
| `GET /api/v1/status` | `GET /status` | Node and chain status. |
| `GET /api/v1/health` | `GET /health` | Basic health probe. |
| `GET /api/v1/blocks` | `GET /blocks` | Block list for explorers. |
| `GET /api/v1/blocks/latest` | `GET /blocks/latest` | Latest known block. |
| `GET /api/v1/blocks/recent` | `GET /blocks/recent` | Recent block page. |
| `GET /api/v1/blocks/page` | `GET /blocks/page` | Offset/limit block page. |
| `GET /api/v1/blocks/:hash` | `GET /blocks/:hash` | Block by hash. |
| `GET /api/v1/blocks/:hash/overview` | `GET /blocks/:hash/overview` | Explorer block summary. |
| `GET /api/v1/blocks/:hash/transactions` | `GET /blocks/:hash/transactions` | Transactions in a block. |
| `GET /api/v1/txs` | `GET /txs` | Transaction list. |
| `GET /api/v1/txs/:txid` | `GET /txs/:txid` | Transaction by id. |
| `POST /api/v1/tx/submit` | `POST /tx/submit` | Submit a transaction. |
| `GET /api/v1/address/:address` | `GET /address/:address` | Address explorer view. |
| `GET /api/v1/address/:address/summary` | `GET /address/:address/summary` | Address balance/activity summary. |
| `GET /api/v1/address/:address/activity` | `GET /address/:address/activity` | Address activity. |
| `GET /api/v1/address/:address/utxos` | `GET /address/:address/utxos` | Address UTXOs. |
| `GET /api/v1/mempool` | `GET /mempool` | Mempool view. |
| `POST /api/v1/mining/template` | `POST /mining/template` | Miner work template. |
| `POST /api/v1/mining/submit` | `POST /mining/submit` | Submit mined work. |
| `GET /api/v1/p2p/status` | `GET /p2p/status` | Public P2P status. |
| `GET /api/v1/sync/status` | `GET /sync/status` | Public sync status. |
| `GET /api/v1/sync/verify` | `GET /sync/verify` | Storage/sync verification summary. |
| `GET /api/v1/snapshot` | `GET /snapshot` | Snapshot metadata only. |
| `GET /api/v1/release` | `GET /release` | Release readiness metadata. |
| `GET /api/v1/policy` | `GET /policy` | Consensus/runtime policy summary. |

No smart-contract endpoints are part of API v1 yet.

### P2P diagnostics payload additions (v2.2.15)

`GET /api/v1/p2p/status` / `GET /p2p/status` remains a public, read-only diagnostic endpoint. In v2.2.15 it includes additional operator fields for chain-id isolation and peer troubleshooting without moving admin-only data onto the public surface:

- `chain_id`: local chain id used for P2P topics and message validation.
- `p2p_mode` / `mode`: configured P2P mode, for example `libp2p-real`.
- `peer_id` and `local_node_id`: the local libp2p node id.
- `peer_count` and `connected_peers`: currently compatible connected peer count and peer ids.
- `peer_recovery[]`: per-peer connection state with `peer_id`, `connected`, score/tier fields, `last_seen_unix`, `last_activity_unix`, optional peer `chain_id`, and `chain_id_compatible`.
- `peer_state_summary.chain_compatible` and `peer_state_summary.chain_incompatible_or_unknown`: summary counters to separate compatible peers from peers that have not proven the local chain id.
- `inbound_chain_mismatch_dropped` and `last_drop_reason`: counters/reason strings for messages rejected because their embedded `chain_id` did not match the local node.
- Sync/tip context such as `selected_sync_peer`, `sync_candidates`, `sync_state`, and propagation counters already exposed by the P2P status surface.

Peers that send messages for a different `chain_id` are penalized and are not counted as healthy compatible peers. Admin boundaries are unchanged; sensitive diagnostics remain under `/admin`.

## Admin/operator endpoints

Operator routes are mounted under `/admin` when admin routing is enabled. Their historical top-level aliases are also only registered when admin routing is enabled.

Admin routing is controlled by `PULSEDAG_ADMIN_ENABLED` and the bind/profile defaults in `pulsedagd`:

- Enabled by default for local/dev-style operation (`dev`, `local`, rehearsal profiles, or localhost RPC binds).
- Disabled by default for public operator/testnet/private binds such as `0.0.0.0:8080`.
- Can be explicitly set with `PULSEDAG_ADMIN_ENABLED=true` or `PULSEDAG_ADMIN_ENABLED=false`.

### Dangerous or sensitive endpoints

The following endpoints are intentionally treated as admin/operator surface:

| Admin route | Compatibility alias when enabled | Risk |
| --- | --- | --- |
| `POST /admin/snapshot/create` | `POST /snapshot/create` | Creates/persists node snapshots. |
| `POST /admin/prune` | `POST /prune` | Prunes historical block data. |
| `POST /admin/sync/rebuild` | `POST /sync/rebuild` | Rebuilds in-memory chain state from persisted data. |
| `GET /admin/sync/rebuild-preview` | `GET /sync/rebuild-preview` | Rebuild planning details. |
| `GET /admin/sync/replay-plan` | `GET /sync/replay-plan` | Snapshot/delta replay plan. |
| `GET /admin/sync/incremental-plan` | `GET /sync/incremental-plan` | Incremental sync planning details. |
| `GET /admin/diagnostics` | `GET /diagnostics` | Rich diagnostics that may include sensitive runtime/storage details. |
| `GET /admin/operator/query-pack` | `GET /operator/query-pack` | Bundled operator diagnostics. |
| `GET /admin/runtime/events` | `GET /runtime/events` | Runtime event stream/history. |
| `GET /admin/maintenance/report` | `GET /maintenance/report` | Operator maintenance guidance. |
| `POST /admin/pow/metrics/capture` | `POST /pow/metrics/capture` | Writes PoW metrics snapshots. |
| `POST /admin/pow/metrics/prune` | `POST /pow/metrics/prune` | Deletes old PoW metric snapshots. |
| `POST /admin/pow/auto/run` | `POST /pow/auto/run` | Runs automated PoW test/capture workflow. |

Snapshot restore is currently documented as an operator runbook workflow rather than exposed as a public RPC route. It should remain operator-only if a future RPC endpoint is added.

## Compatibility guidance

New integrations should use `/api/v1/...`. Existing clients can continue using top-level aliases for public endpoints during the v2.x compatibility window. Operators should migrate scripts from dangerous top-level aliases to `/admin/...` and keep admin routing disabled on public-facing RPC binds unless the endpoint is protected by network controls.

## RPC security profiles (v2.2.19 hardening)

PulseDAG now supports four explicit RPC exposure profiles for public-testnet readiness, without enabling public testnet by default:

- `local_dev`: localhost-oriented development profile.
- `private_operator`: private/local operator use; admin routes remain disabled unless explicitly enabled.
- `public_safe`: public-read surface only; admin/operator/dangerous routes are not mounted.
- `disabled_admin`: full public/private route set except admin routes are always disabled.

### Public exposure warning

Do not expose RPC directly to the public internet without network controls. Even in `public_safe`, place RPC behind firewall and rate controls.

### Firewall examples

- Allow only local subnet operators: `ufw allow from 10.0.0.0/8 to any port 8080 proto tcp`
- Deny global inbound to RPC: `ufw deny 8080/tcp`
- Allow loopback-only process binding: set `PULSEDAG_RPC_BIND=127.0.0.1:8080`

### Recommended production profile

Use `PULSEDAG_API_PROFILE=public_safe` for public readers and keep operator/admin flows on separate private infrastructure.

### Public-safe endpoints

Public-safe profile includes read-only explorer/health/status surfaces, for example:
`/api/v1/health`, `/api/v1/status`, `/api/v1/blocks`, `/api/v1/txs`, `/api/v1/address/:address`, `/api/v1/readiness`, `/api/v1/release`, `/api/v1/policy`.

Admin/operator paths such as `/admin/*`, `/snapshot/create`, `/prune`, `/sync/rebuild`, and `/operator/query-pack` are not available in `public_safe`.
