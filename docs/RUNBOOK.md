# PulseDAG v2.4.0 operator runbook

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
- `/runtime/status`
- `/p2p/status`
- `/sync/status`
- `/sync/lag`
- `/readiness`
- `/release`

The exact endpoint set depends on the configured RPC profile. Public-safe listeners intentionally expose a narrower surface than private/operator listeners.

## If a node falls behind

1. Inspect `/sync/status` and the selected sync peer.
2. Inspect `/p2p/status`, direct connected peers, retained-history capability, and the network-selected height gap.
3. Inspect orphan and missing-parent pressure where operator diagnostics are enabled.
4. Check storage/runtime events before using snapshot, rebuild, reconciliation, prune, or restore operations.
5. Confirm selected height, tip and canonical state convergence after the corrective action.

## If runtime alerts grow

- inspect the configured runtime/event diagnostics on the private operator listener;
- verify peers, lag, orphan count, missing-parent backlog, mempool size, disk pressure, snapshot state, mining-submit finality and RPC responsiveness;
- preserve an incident/evidence bundle before any destructive recovery action.

## RPC hardening limits

- `PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES`: maximum request body size for guarded endpoints.
- `PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE`: request budget; `0` disables rate limiting where the profile permits it.
- `PULSEDAG_RPC_RATE_LIMIT_PER_IP`: per-source-IP budgets when `true`, one global budget when `false`.

Default public-testnet preparation posture keeps operator/admin RPC on loopback or a private management interface. The `public_safe` profile uses a bounded per-IP limiter, deny-by-default/allowlisted CORS and an explicit route allowlist. Signed transaction submission uses canonical `POST /api/v1/tx/submit`; wallet secrets are never accepted by the node.

Machine-readable errors include request-size, rate-limit, route/admission and signed-transaction rejection classes. Treat an unknown mining-submit finality result as non-final and reconcile it rather than recording an immediate definitive rejection.

## Expected `/release` identity

```json
{
  "ok": true,
  "data": {
    "version": "v2.4.0",
    "pow_algorithm": "kHeavyHash",
    "pow_engine": "canonical_core",
    "miner_mode": "external",
    "smart_contracts": "disabled",
    "pool_logic": "disabled_not_in_node"
  }
}
```

## v2.4.0 private burn-in

Use [`runbooks/V2_4_0_SINGLE_NODE_OPERATIONS.md`](runbooks/V2_4_0_SINGLE_NODE_OPERATIONS.md) for the isolated private-burn-in profile and its fixed v2.4 identity. The exact launch candidate SHA, clean database/network initialization, configuration digest, process inventory and binary/image digests must be recorded before an official burn-in clock starts.

Published release evidence and later launch evidence must remain bound to their exact SHAs; evidence from different candidates must not be combined.

## Readiness boundary

`v2.4.0` is the active software release. Tag/GitHub Release publication is separate from public-testnet launch authorization.

Until a separate explicit public launch decision and actual public launch are recorded:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

For release and launch sequencing, use [`ROADMAP_V2_4_0.md`](ROADMAP_V2_4_0.md), [`RELEASE_EVIDENCE.md`](RELEASE_EVIDENCE.md), and [`checklists/V2_4_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md`](checklists/V2_4_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md).
