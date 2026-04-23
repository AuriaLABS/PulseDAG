# v2.2 Official Alerts: Operator Guide

This guide describes the official alert intents and response paths.

## P2P health/recovery
- `p2p-peer-recovery-stall`
- `p2p-relay-duplicate-ratio-high`

Primary action: follow `docs/runbooks/P2P_RECOVERY.md`.

## Sync state and lag
- `sync-fallbacks-growing`
- `sync-rebuild-recommended`
- `consistency-issues-present`

Primary action: follow `docs/runbooks/RECOVERY_ORCHESTRATION.md`, then `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.

## Mempool pressure and relay behavior
- `mempool-pressure-high`
- `mempool-reject-rate-rising`

Primary action: follow `docs/runbooks/MAINTENANCE_SELF_CHECK.md`; if unresolved, escalate through `docs/runbooks/RECOVERY_ORCHESTRATION.md`.

## Snapshot/prune/rebuild health
- `snapshot-missing-on-startup`
- `consistency-issues-present`

Primary action: `docs/runbooks/SNAPSHOT_RESTORE.md` and `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.

## Mining flow health
- `mining-rejections-increasing`

Primary action: validate template freshness, parent-tip alignment, and runtime consistency via `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.

## Release/runtime health
- `runtime-self-audit-failing`
- `active-runtime-alerts-present`

Primary action: `docs/runbooks/MAINTENANCE_SELF_CHECK.md` and `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
