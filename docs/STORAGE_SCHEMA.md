# Storage schema versioning

PulseDAG stores explicit schema metadata in the RocksDB `meta` column family.

## Current schema

- `storage_schema_version`: current node schema version (`1` for v2.2.14).
- `chain_id`: chain identifier associated with the last persisted snapshot.
- `snapshot_metadata`: structured restore-gate metadata for the last snapshot.

Node startup opens storage through `Storage::open`, which creates missing metadata for new databases and rejects databases whose stored schema version does not match the node's supported schema. Operators should treat a schema mismatch as a hard startup gate: upgrade with the matching binary, or restore from a compatible snapshot bundle.

## Upgrade and rollback expectations

- Back up the database or export a snapshot bundle before upgrading binaries.
- Upgrade one schema generation at a time; v2.2.14 supports schema version `1`.
- Do not roll back a node onto a database written by a newer schema unless the newer release explicitly documents compatibility.
- If rollback is required, restore a snapshot bundle whose `schema_version` matches the rollback binary.
