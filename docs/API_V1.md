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
