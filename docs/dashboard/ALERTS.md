# PulseDAG v2.3.0 private-testnet alerts

The canonical Prometheus alert rules are:

- `ops/observability/v2.3.0/alert-rules.yml`

Thresholds are private-testnet operating baselines, not consensus parameters.

## Observability and RPC

- `PulseDAGExporterDown` — exporter unreachable for 2 minutes; critical.
- `PulseDAGRPCCollectionFailed` — one or more required RPC endpoints unavailable for 1 minute; critical.
- `PulseDAGRPCStatusStale` — status served from stale data for 2 minutes; warning.

Primary response: `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.

## P2P

- `PulseDAGNoConnectedPeers` — zero peers for 3 minutes; critical.
- `PulseDAGP2PStatusDegraded` — degraded P2P status for 3 minutes; warning.

Primary response: `docs/runbooks/P2P_RECOVERY.md`.

## Sync and recovery

- `PulseDAGSyncConsistencyFailure` — consistency failure or issue count above zero for 1 minute; critical.
- `PulseDAGSyncLagHigh` — lag above 100 blocks for 5 minutes; warning.
- `PulseDAGSyncLagCritical` — lag above 500 blocks for 5 minutes; critical.
- `PulseDAGMissingParentBacklog` — more than 128 pending missing parents for 5 minutes; warning.

Primary response: `docs/runbooks/RECOVERY_ORCHESTRATION.md`. Use `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` for critical lag or storage divergence.

## Mempool

- `PulseDAGMempoolOrphanPressure` — orphan pool above 80 percent of its configured limit for 5 minutes; warning.

Primary response: `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.

## Snapshot, pruning, and replay

- `PulseDAGSnapshotMissing` — no snapshot for 15 minutes; warning.
- `PulseDAGStorageReplayGap` — storage replay gap above zero for 5 minutes; critical.

Primary response: `docs/runbooks/SNAPSHOT_RESTORE.md` and `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.

## PoW cadence

- `PulseDAGPoWProductionSlow` — average block interval above 90 seconds for 10 minutes; warning.
- `PulseDAGPoWProductionTooFast` — non-zero average block interval below 30 seconds for 10 minutes; warning.

Primary response: verify external miner attachment, template freshness, accepted submissions, and difficulty evidence using `docs/runbooks/MAINTENANCE_SELF_CHECK.md`.

## Escalation

Treat simultaneous critical alerts across sync and storage as a private-testnet no-go condition until consistency is restored and evidence is captured. Alert resolution does not authorize a public testnet, a release tag, or the 30-day public-testnet clock.
