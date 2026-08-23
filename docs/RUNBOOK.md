# PulseDAG v2.4.0 operator runbook

> Task31 candidate status: `PENDING_EXACT_CANDIDATE_EVIDENCE`. The final activated-v2 startup/storage/P2P path and packaged recovery procedure are still being validated. Do not treat this runbook as release or public-testnet authorization.

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

## Expected release boundary

The final v2.4.0 candidate must report `version=v2.4.0`, external-miner mode, disabled smart contracts and an exact protocol/network identity matching the frozen candidate. Raw-private-key wallet RPC is removed from the supported node boundary; signed transactions must be produced outside the node.

The exact `chain_id`, genesis hash, activation identity and artifact digests are not frozen until Task31 completes the activated-v2 startup/recovery implementation and exact-SHA validation.

## Readiness boundary

The repository is constructing the v2.4.0 release/activation candidate. This runbook does not authorize tag creation, GitHub Release publication or public-testnet launch.

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- default high cadence remains experimental/disabled
- `contracts_enabled=false`

The v2.3.0 private-testnet operations documents remain historical/compatibility inputs only and must not be used as v2.4.0 exact-candidate evidence.
