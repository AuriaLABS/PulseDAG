# PulseDAG first-network alert policy

The current candidate alert definitions and thresholds are maintained only in [v3.0.0 alert rules](../../ops/observability/v3.0.0/alert-rules.yml) and [host operations alerts](../../ops/observability/v3.0.0/alert-rules-operations.yml). They are operating baselines, not consensus parameters or authorization to launch.

## Response surfaces

- Exporter/RPC unreachability, staleness, degraded liveness and mining-submit pressure: [maintenance self-check](../runbooks/MAINTENANCE_SELF_CHECK.md).
- Peer isolation, low effective peers and dropped recovery work: [P2P recovery](../runbooks/P2P_RECOVERY.md).
- Commit publication mismatch, lost accepted hashes, invalid roots, selected-tip divergence and stalled recovery: [recovery orchestration](../runbooks/RECOVERY_ORCHESTRATION.md).
- Missing or unverifiable snapshots and replay gaps: [snapshot restore](../runbooks/SNAPSHOT_RESTORE.md) and [snapshot and delta recovery](../runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md).

Before public testnet authorization, independently validate operational signals for native monetary admission, target cadence, missing-parent pressure, checkpoint integrity and recovery. Prototype thresholds are not automatically valid for the first deployed network. Track missing coverage under #1418 and #1465.

An unresolved integrity or security incident blocks the affected acceptance decision. Alert resolution is not public testnet GO, mainnet GO or permission to start the 30-day public-testnet clock. Smart contracts remain inactive.
