# PulseDAG v2.3.0 operator runbook

## Start a node

```bash
cargo run --locked -p pulsedagd
```

## Start the external miner

```bash
cargo run --locked -p pulsedag-miner -- \
  --node http://127.0.0.1:8080 \
  --miner-address YOUR_ADDRESS \
  --threads 4 \
  --loop \
  --sleep-ms 1500 \
  --max-tries 50000
```

The miner is a standalone application. Pool logic, shares, payouts, and accounting are not part of the node or miner.

## Health and status endpoints

- `/health`
- `/status`
- `/runtime`
- `/p2p/runtime`
- `/p2p/status`
- `/sync/status`
- `/sync/lag`
- `/readiness`
- `/release`

## If a node falls behind

1. Inspect `/sync/status` and `/sync/verify`.
2. Inspect `/p2p/status`, direct connected peers, and the selected sync peer.
3. Inspect `/orphans` and missing-parent pressure.
4. Check storage and runtime events before using rebuild or reconciliation operations.
5. Confirm convergence after the corrective action.

## If runtime alerts grow

- inspect `/runtime/events?limit=50`;
- inspect `/runtime/events/summary?limit=500`;
- verify peers, lag, orphan count, missing-parent backlog, mempool size, and RPC responsiveness.

## RPC hardening limits

- `PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES`: maximum request body size for guarded endpoints.
- `PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE`: request budget; `0` disables rate limiting.
- `PULSEDAG_RPC_RATE_LIMIT_PER_IP`: per-source-IP budgets when `true`, one global budget when `false`.

Default posture:

- `private_operator`: `512 KiB`, `120 rpm` per IP;
- `public_safe`: `128 KiB`, `30 rpm` per IP;
- `local_dev` / `disabled_admin`: `1 MiB`, rate limiting disabled.

Guarded surfaces include transaction submit, mining submit, snapshot/rebuild/reconcile/prune operations, and heavy diagnostics.

Machine-readable errors include:

- `request_too_large`;
- `rate_limited`.

## Expected `/release` identity

```json
{
  "ok": true,
  "data": {
    "version": "v2.3.0",
    "pow_algorithm": "kHeavyHash",
    "pow_engine": "canonical_core",
    "miner_mode": "external",
    "smart_contracts": "disabled",
    "pool_logic": "disabled_not_in_node"
  }
}
```

## Readiness boundary

`v2.3.0` is the current private-testnet release candidate. This runbook does not authorize a public-testnet launch. `public_testnet_ready` remains `false`, and the 30-day public-testnet clock has not started.

For multi-node operations, use [`runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`](runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md).
