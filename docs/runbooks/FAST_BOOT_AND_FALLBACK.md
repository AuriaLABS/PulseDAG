# Fast-Boot and Fallback Interpretation (v2.2)

## Purpose
Help operators correctly interpret startup/runtime fast-boot signals and fallback paths so expected behavior is not mistaken for corruption, and true failures are escalated quickly.

## What to check
```bash
curl -s http://127.0.0.1:8080/status | jq
curl -s http://127.0.0.1:8080/runtime | jq
curl -s http://127.0.0.1:8080/sync/status | jq
curl -s http://127.0.0.1:8080/sync/verify | jq
curl -s 'http://127.0.0.1:8080/runtime/events?limit=200' | jq
curl -s 'http://127.0.0.1:8080/runtime/events/summary?limit=500' | jq
```

## Fallback-related signals you may observe
Storage/runtime fallback event kinds include:
- `snapshot_decode_failed_fallback_full`
- `snapshot_delta_replay_failed_fallback_full`
- `restore_drill_snapshot_decode_failed_fallback_full`
- `restore_drill_snapshot_delta_failed_fallback_full`

These indicate fallback to full rebuild path was engaged when possible.

## Interpretation guide
- **Single fallback event + successful `/sync/verify`**: monitor and document; system recovered through safe fallback path.
- **Repeated fallback events across restarts**: investigate snapshot quality and replay window; plan controlled rebuild.
- **Fallback event + inconsistent `/sync/verify`**: treat as active incident; run rebuild/restore runbooks.
- **No fallback possible (explicit restore failure)**: preserve evidence, avoid destructive retries, and run controlled restore from known-good data path.

## P2P mode context for startup interpretation
Always evaluate peer indicators with mode semantics:
- `memory-simulated`: not real network connectivity.
- `libp2p-dev-loopback-skeleton`: not real external network connectivity.
- `libp2p-real`: connected peers represent real network connectivity.

Reference: `docs/OPERATIONS_P2P.md`.

## Recommended operator actions
1. Archive relevant runtime events.
2. Validate coherence with `/sync/verify`.
3. Validate replay/snapshot readiness (`/snapshot`, `/sync/replay-plan`, `/sync/rebuild-preview`).
4. If risk persists, execute `REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
