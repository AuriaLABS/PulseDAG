# PulseDAG API v1

PulseDAG v2.4.0 keeps the stable public API namespace at `/api/v1`. The sample below matches `get_api_version()` (`repo_version()` + `operator_stage()`). Historical v2.2.14 / v2.3.0 payloads are compatibility provenance only.

This document does **not** authorize a `v2.4.0` tag, GitHub Release, public-testnet GO, Day 0, default high cadence, or `contracts_enabled=true`. Live trackers: #1132 (audit epic), #1128 (docs identity), #1127 / #1139 (dependency graph), #781 / #794 (launch control).

## Version response

`GET /api/v1` and `GET /api/v1/version` return the API namespace metadata:

```json
{
  "ok": true,
  "data": {
    "api_version": "v1",
    "stable_prefix": "/api/v1",
    "release_version": "v2.4.0",
    "stage": "v2.4-readiness"
  },
  "error": null,
  "meta": null
}
```

`stage` remains a readiness label for the moving Task31 candidate. It is not a public-testnet or contracts-activation signal.

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
| `GET /api/v1/pulse` | `GET /pulse` | Observational PulseClock v1 (`docs/PULSECLOCK_V1.md`). Telemetry only; not a consensus or covenant activation. |

No smart-contract endpoints are part of API v1. `contracts_enabled` on `/status` must remain `false` on the Task31 line.

### P2P diagnostics payload (historical v2.2.15 fields, still current)

`GET /api/v1/p2p/status` / `GET /p2p/status` remains a public, read-only diagnostic endpoint. It includes operator fields for chain-id isolation and peer troubleshooting without moving admin-only data onto the public surface:

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

Operator routes are mounted under `/admin` only when admin routing is explicitly enabled. Their historical top-level aliases are also only registered when admin routing is enabled.

Admin routing is controlled by `PULSEDAG_ADMIN_ENABLED` and the exposure checks in `pulsedagd`:

- Admin is disabled by default for **all** profiles and RPC binds.
- Enabling admin is an explicit operator action with `PULSEDAG_ADMIN_ENABLED=true`.
- `public_safe` and `disabled_admin` reject startup when admin is enabled.
- A non-local RPC bind cannot use the implicit `local_dev` exposure profile.
- Keep admin/operator RPC on loopback or private management infrastructure. The unsafe remote-admin override is not part of the public-safe deployment profile and is not a public-testnet GO.

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

Snapshot restore is an operator runbook workflow rather than a public RPC route. It should remain operator-only if a future RPC endpoint is added.

## Compatibility guidance

New integrations should use `/api/v1/...`. Existing clients can continue using top-level aliases for public endpoints during the v2.x compatibility window. Operators should migrate scripts from dangerous top-level aliases to `/admin/...` and keep admin routing disabled on public-facing RPC binds unless the endpoint is protected by network controls.

## RPC security profiles

PulseDAG supports four explicit RPC exposure profiles. They are hardening knobs for private/operator use. They do not enable public testnet by default (`public_testnet_ready=false`).

- `local_dev`: localhost-oriented development profile.
- `private_operator`: private/local operator use; admin routes remain disabled unless explicitly enabled.
- `public_safe`: public-read surface only; admin/operator/dangerous routes are not mounted.
- `disabled_admin`: full public/private route set except admin routes are always disabled.

### Public-safe hardening defaults

For `public_safe`, the built-in guarded-route defaults are:

- request body limit: **128 KiB**;
- rate limit: **30 requests per 60 seconds**;
- rate-limit key: **per client IP** when connection information is available;
- wildcard CORS origin (`*`): **rejected**; use an explicit allowlist;
- admin routes: **not mounted**.

`public_safe` also rejects a zero request-rate limit unless an explicit unsafe override is supplied. Unsafe overrides are not part of the supported public-testnet baseline and must not be used to claim #794/#781 readiness.

### Public exposure warning

Do not expose RPC directly to the public internet without network controls. Even in `public_safe`, place RPC behind firewall and rate controls. Preferred bind for Task31 rehearsal remains loopback unless `public_safe` or `PULSEDAG_RPC_UNSAFE_BIND_ANY=true` is set (see #1126 / PR #1133).

### Firewall examples

- Allow only local subnet operators: `ufw allow from 10.0.0.0/8 to any port 8080 proto tcp`
- Deny global inbound to RPC: `ufw deny 8080/tcp`
- Allow loopback-only process binding: set `PULSEDAG_RPC_BIND=127.0.0.1:8080`

### Recommended operator profile

Use `PULSEDAG_API_PROFILE=public_safe` only when a read surface must leave loopback. Keep operator/admin flows on separate private infrastructure.

Public-safe profile includes read-only explorer/health/status surfaces, for example:
`/api/v1/health`, `/api/v1/status`, `/api/v1/blocks`, `/api/v1/txs`, `/api/v1/address/:address`, `/api/v1/readiness`, `/api/v1/release`, `/api/v1/policy`, `/api/v1/pulse`.

Admin/operator paths such as `/admin/*`, `/snapshot/create`, `/prune`, `/sync/rebuild`, and `/operator/query-pack` are not available in `public_safe`.
