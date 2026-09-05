use pulsedag_core::{
    errors::PulseError, merge_set_digest, ordered_dag_digest, selection_digest, state_digest,
    ProtocolActivationIdentity,
};
use serde::{Deserialize, Serialize};

use super::{
    ProtocolSnapshotExportBundleV2, SnapshotVerificationReport, Storage,
    PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION, STORAGE_SCHEMA_VERSION,
};

pub const FAST_SYNC_SNAPSHOT_MANIFEST_VERSION: u32 = 1;

/// Versioned, deterministic bootstrap contract carried beside the existing
/// protocol-bound snapshot bundle.
///
/// The manifest intentionally commits only to deterministic protocol/storage
/// identity and canonical state/DAG digests. Wall-clock export time is not part
/// of the commitment surface, so equivalent state produces equivalent manifest
/// commitments across nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncSnapshotManifestV1 {
    pub manifest_version: u32,
    pub protocol_snapshot_bundle_format_version: u32,
    pub storage_schema_version: u32,
    pub chain_id: String,
    pub genesis_hash: String,
    pub protocol_fingerprint: String,
    pub best_height: u64,
    pub selected_tip: String,
    pub state_commitment: String,
    pub selection_commitment: String,
    pub merge_set_commitment: String,
    pub ordered_dag_commitment: String,
    pub prune_boundary_height: Option<u64>,
    pub original_genesis_hash: Option<String>,
    pub omitted_parent_hashes: Vec<String>,
    pub snapshot_captured_at_unix: Option<u64>,
    pub snapshot_generation: u64,
    pub accepted_storage_generation: u64,
    pub delta_start_generation: u64,
    pub delta_end_generation: u64,
}

/// Bootstrap envelope used by the #1035 fast-sync/state-bootstrap path.
///
/// The inner v2 protocol snapshot remains the serialization/restore authority;
/// this outer contract freezes the v3 launch-facing manifest and deterministic
/// commitments without changing legacy bundle decoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FastSyncSnapshotBundleV1 {
    pub manifest: FastSyncSnapshotManifestV1,
    pub snapshot_bundle: ProtocolSnapshotExportBundleV2,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn deterministic_manifest(
    bundle: &ProtocolSnapshotExportBundleV2,
) -> Result<FastSyncSnapshotManifestV1, PulseError> {
    let snapshot = &bundle.legacy_bundle.snapshot;
    let metadata = &bundle.legacy_bundle.snapshot_metadata;
    let mut omitted_parent_hashes = metadata.omitted_parent_hashes.clone();
    omitted_parent_hashes.sort();

    Ok(FastSyncSnapshotManifestV1 {
        manifest_version: FAST_SYNC_SNAPSHOT_MANIFEST_VERSION,
        protocol_snapshot_bundle_format_version: bundle.format_version,
        storage_schema_version: metadata.schema_version,
        chain_id: snapshot.chain_id.clone(),
        genesis_hash: snapshot.dag.genesis_hash.clone(),
        protocol_fingerprint: bundle.activation_record.fingerprint.clone(),
        best_height: snapshot.dag.best_height,
        selected_tip: metadata.selected_tip.clone(),
        state_commitment: state_digest(snapshot)?,
        selection_commitment: selection_digest(snapshot),
        merge_set_commitment: merge_set_digest(snapshot),
        ordered_dag_commitment: ordered_dag_digest(snapshot),
        prune_boundary_height: metadata.prune_boundary_height,
        original_genesis_hash: metadata.original_genesis_hash.clone(),
        omitted_parent_hashes,
        snapshot_captured_at_unix: bundle.legacy_bundle.snapshot_captured_at_unix,
        snapshot_generation: bundle.legacy_bundle.snapshot_generation,
        accepted_storage_generation: bundle.legacy_bundle.accepted_storage_generation,
        delta_start_generation: bundle.legacy_bundle.delta_start_generation,
        delta_end_generation: bundle.legacy_bundle.delta_end_generation,
    })
}

fn require_manifest_match<T: std::fmt::Debug + PartialEq>(
    field: &str,
    actual: &T,
    expected: &T,
) -> Result<(), PulseError> {
    if actual != expected {
        return Err(storage_error(format!(
            "fast-sync snapshot manifest mismatch for {field}: actual={actual:?} expected={expected:?}"
        )));
    }
    Ok(())
}

impl Storage {
    /// Export a v1 fast-sync envelope only after the existing protocol snapshot
    /// gate has produced a fully verified bundle.
    pub fn export_fast_sync_snapshot_bundle_v1(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(FastSyncSnapshotBundleV1, SnapshotVerificationReport), PulseError> {
        let (snapshot_bundle, report) = self.export_protocol_snapshot_bundle_v2(expected)?;
        let manifest = deterministic_manifest(&snapshot_bundle)?;
        Ok((
            FastSyncSnapshotBundleV1 {
                manifest,
                snapshot_bundle,
            },
            report,
        ))
    }

    /// Verify the full fast-sync envelope without mutating durable storage.
    ///
    /// Identity, schema and protocol-bundle compatibility fail closed before a
    /// caller can enter the import boundary. Canonical state/DAG commitments are
    /// then recomputed from the enclosed snapshot and compared field-by-field.
    pub fn verify_fast_sync_snapshot_bundle_v1(
        &self,
        bundle: &FastSyncSnapshotBundleV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        if bundle.manifest.manifest_version != FAST_SYNC_SNAPSHOT_MANIFEST_VERSION {
            return Err(storage_error(format!(
                "unsupported fast-sync snapshot manifest version {}; expected {}",
                bundle.manifest.manifest_version, FAST_SYNC_SNAPSHOT_MANIFEST_VERSION
            )));
        }
        if bundle.manifest.storage_schema_version != STORAGE_SCHEMA_VERSION {
            return Err(storage_error(format!(
                "fast-sync snapshot storage schema {} is incompatible with local schema {}",
                bundle.manifest.storage_schema_version, STORAGE_SCHEMA_VERSION
            )));
        }
        if bundle.manifest.protocol_snapshot_bundle_format_version
            != PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
        {
            return Err(storage_error(format!(
                "fast-sync manifest protocol snapshot format {} is incompatible with local format {}",
                bundle.manifest.protocol_snapshot_bundle_format_version,
                PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
            )));
        }

        let report = self.verify_protocol_snapshot_bundle_v2(&bundle.snapshot_bundle, expected)?;
        let derived = deterministic_manifest(&bundle.snapshot_bundle)?;
        let manifest = &bundle.manifest;

        require_manifest_match("chain_id", &manifest.chain_id, &derived.chain_id)?;
        require_manifest_match(
            "genesis_hash",
            &manifest.genesis_hash,
            &derived.genesis_hash,
        )?;
        require_manifest_match(
            "protocol_fingerprint",
            &manifest.protocol_fingerprint,
            &derived.protocol_fingerprint,
        )?;
        require_manifest_match("best_height", &manifest.best_height, &derived.best_height)?;
        require_manifest_match(
            "selected_tip",
            &manifest.selected_tip,
            &derived.selected_tip,
        )?;
        require_manifest_match(
            "state_commitment",
            &manifest.state_commitment,
            &derived.state_commitment,
        )?;
        require_manifest_match(
            "selection_commitment",
            &manifest.selection_commitment,
            &derived.selection_commitment,
        )?;
        require_manifest_match(
            "merge_set_commitment",
            &manifest.merge_set_commitment,
            &derived.merge_set_commitment,
        )?;
        require_manifest_match(
            "ordered_dag_commitment",
            &manifest.ordered_dag_commitment,
            &derived.ordered_dag_commitment,
        )?;
        require_manifest_match(
            "prune_boundary_height",
            &manifest.prune_boundary_height,
            &derived.prune_boundary_height,
        )?;
        require_manifest_match(
            "original_genesis_hash",
            &manifest.original_genesis_hash,
            &derived.original_genesis_hash,
        )?;
        require_manifest_match(
            "omitted_parent_hashes",
            &manifest.omitted_parent_hashes,
            &derived.omitted_parent_hashes,
        )?;
        require_manifest_match(
            "snapshot_captured_at_unix",
            &manifest.snapshot_captured_at_unix,
            &derived.snapshot_captured_at_unix,
        )?;
        require_manifest_match(
            "snapshot_generation",
            &manifest.snapshot_generation,
            &derived.snapshot_generation,
        )?;
        require_manifest_match(
            "accepted_storage_generation",
            &manifest.accepted_storage_generation,
            &derived.accepted_storage_generation,
        )?;
        require_manifest_match(
            "delta_start_generation",
            &manifest.delta_start_generation,
            &derived.delta_start_generation,
        )?;
        require_manifest_match(
            "delta_end_generation",
            &manifest.delta_end_generation,
            &derived.delta_end_generation,
        )?;

        Ok(report)
    }

    /// Verify the complete fast-sync envelope before crossing the durable
    /// mutation boundary. The inner protocol import re-verifies its own contract
    /// and applies accepted blocks, state and protocol identity atomically.
    pub fn import_fast_sync_snapshot_bundle_v1(
        &self,
        bundle: FastSyncSnapshotBundleV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        self.verify_fast_sync_snapshot_bundle_v1(&bundle, expected)?;
        self.import_protocol_snapshot_bundle_v2(bundle.snapshot_bundle, expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::genesis::init_chain_state;

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-fast-sync-manifest-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn source_bundle(storage: &Storage) -> (FastSyncSnapshotBundleV1, ProtocolActivationIdentity) {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let (bundle, report) = storage
            .export_fast_sync_snapshot_bundle_v1(&expected)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        (bundle, expected)
    }

    #[test]
    fn fast_sync_manifest_is_deterministic_and_protocol_bound() {
        let path = temp_db_path("deterministic");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);

        let derived = deterministic_manifest(&bundle.snapshot_bundle).unwrap();
        assert_eq!(bundle.manifest, derived);
        assert_eq!(
            bundle.manifest.manifest_version,
            FAST_SYNC_SNAPSHOT_MANIFEST_VERSION
        );
        assert_eq!(
            bundle.manifest.storage_schema_version,
            STORAGE_SCHEMA_VERSION
        );
        assert_eq!(bundle.manifest.chain_id, expected.chain_id);
        assert_eq!(bundle.manifest.genesis_hash, expected.genesis_hash);
        assert!(storage
            .verify_fast_sync_snapshot_bundle_v1(&bundle, &expected)
            .is_ok());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn fast_sync_manifest_rejects_state_commitment_tampering() {
        let path = temp_db_path("state-tamper");
        let storage = Storage::open(&path).unwrap();
        let (mut bundle, expected) = source_bundle(&storage);
        bundle.manifest.state_commitment = "00".repeat(32);

        let error = storage
            .verify_fast_sync_snapshot_bundle_v1(&bundle, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("state_commitment"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn fast_sync_manifest_rejects_storage_schema_tampering() {
        let path = temp_db_path("schema-tamper");
        let storage = Storage::open(&path).unwrap();
        let (mut bundle, expected) = source_bundle(&storage);
        bundle.manifest.storage_schema_version += 1;

        let error = storage
            .verify_fast_sync_snapshot_bundle_v1(&bundle, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("storage schema"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn fast_sync_manifest_rejects_protocol_fingerprint_tampering() {
        let path = temp_db_path("protocol-tamper");
        let storage = Storage::open(&path).unwrap();
        let (mut bundle, expected) = source_bundle(&storage);
        bundle.manifest.protocol_fingerprint = "ff".repeat(32);

        let error = storage
            .verify_fast_sync_snapshot_bundle_v1(&bundle, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("protocol_fingerprint"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn failed_fast_sync_manifest_verification_leaves_target_unchanged() {
        let source_path = temp_db_path("import-source");
        let target_path = temp_db_path("import-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let (mut bundle, expected) = source_bundle(&source);
        bundle.manifest.state_commitment = "11".repeat(32);

        let target_state = init_chain_state("pulsedag-private".to_string());
        let target_identity = ProtocolActivationIdentity::legacy_from_state(&target_state);
        target
            .persist_chain_state_with_protocol_record(&target_state)
            .unwrap();

        assert!(target
            .import_fast_sync_snapshot_bundle_v1(bundle, &expected)
            .is_err());
        assert_eq!(
            target.load_chain_state().unwrap().unwrap().chain_id,
            target_state.chain_id
        );
        assert_eq!(
            target
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            target_identity
        );

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }
}
