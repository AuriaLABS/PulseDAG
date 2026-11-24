# PulseDAG first-network runbook index

Public native-v3 testnet is the first deployed network. This index does not authorize launch, monetary mining, smart contracts or the 30-day public-testnet clock. The only launch decision record is [#781](https://github.com/AuriaLABS/PulseDAG/issues/781).

## First-network preparation

- [Launch sequence and acceptance gates](V3_0_0_TESTNET_FIRST_LAUNCH.md)
- [Native-only network policy](V3_FIRST_NETWORK_NATIVE_ONLY.md)
- [Public testnet preparation and role separation](V3_0_0_PUBLIC_TESTNET_PREP.md)
- [Offline identity preparation](V3_PUBLIC_TESTNET_IDENTITY_DRAFT_OFFLINE.md)
- [Native pull synchronization](NATIVE_V3_PULL_SYNC_FIRST_PUBLIC_TESTNET.md)
- [Binary installation](../INSTALL_BINARIES.md)
- [Release evidence](../RELEASE_EVIDENCE.md)
- [Public monitoring](../dashboard/README.md) — repository path: `docs/dashboard/README.md`.

## Recovery and operational safeguards

The following procedures preserve transferable operational requirements. Validate their commands and assumptions against the exact native candidate before use; a prototype recovery result does not establish native monetary acceptance.

- [Maintenance self-check](MAINTENANCE_SELF_CHECK.md)
- [Peer recovery](P2P_RECOVERY.md)
- [Recovery orchestration](RECOVERY_ORCHESTRATION.md)
- [Snapshot and delta recovery](REBUILD_FROM_SNAPSHOT_AND_DELTA.md)
- [Snapshot restore](SNAPSHOT_RESTORE.md)
- [Prune and restore drill](SNAPSHOT_PRUNE_RESTORE_DRILL.md)
- [Restart and recovery suite](CHAOS_RESTART_RECOVERY_SUITE.md)
- [Fast boot and fallback](FAST_BOOT_AND_FALLBACK.md)

## Transitional development references

Retained prototype procedures are not active operator instructions or compatibility requirements. Their useful incident, security, storage and upgrade safeguards must survive replacement. Do not deploy their old network profiles or start their historical miner/lifecycle commands.

- `V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`
- `V2_3_0_INCIDENT_RESPONSE.md`
- `V2_3_0_SECURITY_AND_CAPACITY.md`
- `STAGING_UPGRADE.md`
- `STAGING_ROLLBACK.md`
- Redacted incident evidence collector: `scripts/private_testnet/collect_incident_evidence.py`.

Unresolved integrity, security, storage, convergence or recovery failures block the relevant network acceptance. Current launch boundaries: `public_testnet_ready=false`, `contracts_enabled=false`; no start or backdating of the 30-day public-testnet clock.
