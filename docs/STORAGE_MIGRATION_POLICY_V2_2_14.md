# PulseDAG v2.2.14 storage migration policy

v2.2.14 closes as a storage, replay, snapshot, restore, pruning, and migration-policy hardening release. It does not add smart contracts, does not enable a contract runtime, does not add pool logic, and keeps the miner as an external standalone application.

## Current schema version

- Current storage schema: `STORAGE_SCHEMA_VERSION = 1`.
- The schema version is persisted in RocksDB metadata under `storage_schema_version`.
- `VERSION` is `v2.2.14`; Cargo workspace metadata is `2.2.14`; license metadata remains ISC.

## Supported upgrade path

v2.2.14 supports opening storage that already declares schema version `1`, and it initializes missing schema metadata on older compatible databases that do not yet have the metadata key. No automatic cross-schema migration is performed in v2.2.14.

Operators should upgrade by:

1. Stopping the node cleanly.
2. Exporting or copying a snapshot and the database directory.
3. Starting the v2.2.14 binary against the copied or intended data directory.
4. Running startup replay/restore/audit checks before declaring the node healthy.

## Missing metadata behavior

If `storage_schema_version` is missing, v2.2.14 treats the database as pre-metadata schema-1 storage and writes the current schema version. This is intentionally conservative and only applies to missing metadata, not corrupt or incompatible metadata.

## Future schema behavior

If the stored schema version is greater than `STORAGE_SCHEMA_VERSION`, startup fails with an operator-facing error. This prevents an older v2.2.14 binary from opening data that may have been written by a newer node.

Expected action: start with a newer PulseDAG binary that supports that schema, or restore from a v2.2.14-compatible snapshot/export.

## Corrupt metadata behavior

If schema metadata exists but is not valid UTF-8 or is not a numeric version, startup rejects the database. Operators should not manually edit the database in place unless they have an export/snapshot and an explicit recovery plan.

## Snapshot/export recommendation before migration

Before any migration or binary downgrade/upgrade rehearsal, capture at least one of:

- a snapshot export bundle,
- a filesystem-level copy of the RocksDB directory while the node is safely stopped,
- or both for operator-critical nodes.

The release evidence should record snapshot export/import and restore-drill results when practical.

## Rollback expectation

Rollback means restoring a known-compatible snapshot/export or stopped-node database copy. v2.2.14 does not promise in-place downgrade compatibility for future schemas and intentionally rejects future schema metadata to avoid silent data corruption.
