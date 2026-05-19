# Runbook

## Start node
Load env file and run:
```powershell
cargo run -p pulsedagd
```

## Start miner
```powershell
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address YOUR_ADDRESS --threads 4 --loop --sleep-ms 1500 --max-tries 50000
```

## Check health
- `/health`
- `/status`
- `/runtime`
- `/p2p/runtime`
- `/sync/lag`
- `/readiness`

## If node falls behind
- inspect `/sync/status`
- inspect `/sync/verify`
- inspect `/orphans`
- if needed run rebuild
- then run mempool sanitize

## If runtime alerts grow
- inspect `/runtime/events?limit=50`
- inspect `/runtime/events/summary?limit=500`
- verify peers, lag, orphan count, mempool size

## RPC hardening limits
- `PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES`: max accepted request body size in bytes for guarded RPC endpoints.
- `PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE`: requests/minute budget for guarded RPC endpoints (`0` disables rate limiting).
- `PULSEDAG_RPC_RATE_LIMIT_PER_IP`: when `true`, budgets apply per source IP; when `false`, one global budget applies.
- Default profile posture:
  - `private_operator`: `512 KiB` body limit and `120 rpm` per IP.
  - `public_safe`: `128 KiB` body limit and `30 rpm` per IP.
  - `local_dev`/`disabled_admin`: `1 MiB` body limit and rate limiting disabled.
- Guarded endpoints include transaction submit, mining submit, snapshot/rebuild/reconcile/prune operations, and heavy diagnostics/operator query-pack routes.
- Machine-readable hardening errors:
  - `request_too_large`
  - `rate_limited`
