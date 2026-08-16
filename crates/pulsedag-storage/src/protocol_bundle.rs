use pulsedag_core::{
    errors::PulseError, ProtocolActivationIdentity, ProtocolActivationRecordV1,
    ProtocolRestoreIdentityGate,
};
use serde::{Deserialize, Serialize};

use super::{SnapshotExportBundle, SnapshotVerificationReport, Storage};

pub const PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION: u32 = 2;

/// Protocol-bound snapshot envelope for v2.4.0 activation work.
///
/// The historical `SnapshotExportBundle` remains unchanged as the inner payload
/// so existing bundle-v1 decoding stays stable. The outer envelope carries the
/// exact activation identity and fingerprint required before an activated
/// restore can be trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSnapshotExportBundleV2 {
    pub format_version: u32,
    pub activation_record: ProtocolActivationRecordV1,
    pub legacy_bundle: SnapshotExportBundle,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn verification_error(report: &SnapshotVerificationReport) -> PulseError {
    storage_error(format!(
        "protocol snapshot bundle verification failed: {}",
        report
            .issues
            .iter()
            .map(|issue| format!("{}={}", issue.code, issue.message))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

impl Storage {
    /// Export a protocol-bound snapshot only when an explicit durable activation
    /// record is already present and exactly matches the caller expectation.
    /// Legacy schema-1 compatibility without a sidecar is intentionally not
    /// promoted into a v2 protocol bundle.
    pub fn export_protocol_snapshot_bundle_v2(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(ProtocolSnapshotExportBundleV2, SnapshotVerificationReport), PulseError> {
        let gate = self.verify_persisted_protocol_identity(expected)?;
        if gate != ProtocolRestoreIdentityGate::VerifiedRecordV1 {
            return Err(storage_error(
                "protocol snapshot v2 export requires an explicit verified activation record",
            ));
        }
        let activation_record = self.protocol_activation_record()?.ok_or_else(|| {
            storage_error("verified protocol activation record disappeared before export")
        })?;
        activation_record
            .verify_expected(expected)
            .map_err(storage_error)?;

        let (legacy_bundle, report) = self.export_snapshot_bundle(Some(&expected.chain_id))?;
        if legacy_bundle.snapshot.dag.genesis_hash != expected.genesis_hash {
            return Err(storage_error(format!(
                "snapshot genesis hash {} does not match expected protocol genesis {}",
                legacy_bundle.snapshot.dag.genesis_hash, expected.genesis_hash
            )));
        }

        Ok((
            ProtocolSnapshotExportBundleV2 {
                format_version: PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
                activation_record,
                legacy_bundle,
            },
            report,
        ))
    }

    /// Verify a protocol-bound bundle without mutating storage. Exact protocol
    /// identity is checked before the historical bundle verification result is
    /// accepted, including chain and genesis identity.
    pub fn verify_protocol_snapshot_bundle_v2(
        &self,
        bundle: &ProtocolSnapshotExportBundleV2,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        if bundle.format_version != PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION {
            return Err(storage_error(format!(
                "unsupported protocol snapshot bundle format version {}; expected {}",
                bundle.format_version, PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
            )));
        }
        if bundle.legacy_bundle.format_version != 1 {
            return Err(storage_error(format!(
                "unsupported inner snapshot bundle format version {}; expected 1",
                bundle.legacy_bundle.format_version
            )));
        }
        bundle
            .activation_record
            .verify_expected(expected)
            .map_err(storage_error)?;
        if bundle.legacy_bundle.snapshot.chain_id != expected.chain_id {
            return Err(storage_error(format!(
                "snapshot chain_id={} does not match expected protocol chain_id={}",
                bundle.legacy_bundle.snapshot.chain_id, expected.chain_id
            )));
        }
        if bundle.legacy_bundle.snapshot.dag.genesis_hash != expected.genesis_hash {
            return Err(storage_error(format!(
                "snapshot genesis hash {} does not match expected protocol genesis {}",
                bundle.legacy_bundle.snapshot.dag.genesis_hash, expected.genesis_hash
            )));
        }

        let report = self.verify_snapshot_bundle(&bundle.legacy_bundle, Some(&expected.chain_id));
        if !report.restore_guarantees_explicit {
            return Err(verification_error(&report));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{genesis::init_chain_state, ProtocolActivationIdentity};

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedag-storage-protocol-bundle-{test_name}-{unique}"))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn protocol_bundle_v2_export_requires_and_carries_verified_record() {
        let path = temp_db_path("export");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();

        let (bundle, report) = storage
            .export_protocol_snapshot_bundle_v2(&expected)
            .unwrap();

        assert_eq!(bundle.format_version, PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION);
        assert_eq!(bundle.legacy_bundle.format_version, 1);
        assert_eq!(bundle.activation_record.identity, expected);
        assert!(report.restore_guarantees_explicit);
        assert!(storage
            .verify_protocol_snapshot_bundle_v2(&bundle, &expected)
            .is_ok());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn legacy_snapshot_without_explicit_record_cannot_be_promoted_to_v2_bundle() {
        let path = temp_db_path("missing-record");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        storage.persist_chain_state(&state).unwrap();

        assert!(storage
            .export_protocol_snapshot_bundle_v2(&expected)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn protocol_bundle_v2_rejects_identity_and_format_mismatches() {
        let path = temp_db_path("mismatch");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let (bundle, _) = storage
            .export_protocol_snapshot_bundle_v2(&expected)
            .unwrap();

        let mut wrong_identity = expected.clone();
        wrong_identity.chain_id = "pulsedag-private".to_string();
        assert!(storage
            .verify_protocol_snapshot_bundle_v2(&bundle, &wrong_identity)
            .is_err());

        let mut wrong_outer_format = bundle.clone();
        wrong_outer_format.format_version += 1;
        assert!(storage
            .verify_protocol_snapshot_bundle_v2(&wrong_outer_format, &expected)
            .is_err());

        let mut wrong_inner_format = bundle;
        wrong_inner_format.legacy_bundle.format_version += 1;
        assert!(storage
            .verify_protocol_snapshot_bundle_v2(&wrong_inner_format, &expected)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
