# PulseDAG v2.3.0 Private-Testnet Runbook Index

This is the active operator entrypoint for v2.3.0. It does not authorize a public testnet, tag publication, or the start of the 30-day public-testnet clock.

## Primary operations

- `V2_3_0_PRIVATE_TESTNET_OPERATIONS.md` — bootstrap, external miner, lifecycle, upgrade, rollback, state protection, and evidence.
- `V2_3_0_PRIVATE_TESTNET_REHEARSAL.md` — five-node rehearsal, restart, partition, rejoin, and GO/NO-GO evidence.
- `V2_3_0_NETNS_REHEARSAL.md` — isolated Linux namespace rehearsal.
- `V2_3_0_INCIDENT_RESPONSE.md` — severity model, roles, containment, recovery, and closure.
- `V2_3_0_SECURITY_AND_CAPACITY.md` — RPC pressure, disk capacity, identity rotation, and monitoring access.

## Recovery

- `MAINTENANCE_SELF_CHECK.md`
- `P2P_RECOVERY.md`
- `RECOVERY_ORCHESTRATION.md`
- `REBUILD_FROM_SNAPSHOT_AND_DELTA.md`
- `SNAPSHOT_RESTORE.md`
- `SNAPSHOT_PRUNE_RESTORE_DRILL.md`
- `CHAOS_RESTART_RECOVERY_SUITE.md`
- `FAST_BOOT_AND_FALLBACK.md`

## Upgrade compatibility

- `STAGING_UPGRADE.md`
- `STAGING_ROLLBACK.md`

The v2.3.0 lifecycle controller remains the active upgrade and rollback mechanism.

## Evidence and observability

- `docs/RELEASE_EVIDENCE.md`
- `docs/checklists/V2_3_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md`
- `docs/dashboard/README.md`
- `scripts/private_testnet/collect_incident_evidence.py`
- `ops/observability/v2.3.0/README.md`

An unresolved integrity, security, storage, convergence, recovery, or evidence failure is a private-testnet NO-GO. Private-testnet completion never changes `public_testnet_ready=false` or starts/backdates the 30-day public-testnet clock.
