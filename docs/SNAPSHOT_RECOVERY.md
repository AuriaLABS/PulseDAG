# Snapshot recovery gates

v2.2.14 snapshots include restore metadata so operators can verify a snapshot before it is trusted.

## Snapshot metadata

Each persisted snapshot records:

- `chain_id`
- `schema_version`
- `best_height`
- `selected_tip`
- `state_root`
- `created_at`

Snapshot restore and import gates reject mismatched chain IDs, unsupported schema versions, and metadata that no longer matches the snapshot state root or selected tip. This prevents accidental cross-chain restores and catches corrupted or stale snapshot metadata before replay.

## Dry-run verification

Use snapshot export/import verification or the restore drill endpoint/workflow to validate snapshot + delta replay before relying on the data for recovery. A successful restore drill appends runtime evidence that audit endpoints can surface.

## Recovery procedure

1. Confirm the target node's expected `chain_id`.
2. Verify the snapshot bundle before import.
3. Reject and replace any bundle that reports chain ID, schema version, state root, or lineage issues.
4. Run a restore drill where practical before deleting old recovery anchors.
