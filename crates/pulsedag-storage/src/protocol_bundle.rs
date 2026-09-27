use pulsedag_core::{
    derive_finality_boundary_v1, errors::PulseError, verify_authoritative_state_snapshot_v2,
    MonetaryCadenceSegment, ProtocolActivationIdentity, ProtocolActivationRecordV1,
    ProtocolConsensusMode, ProtocolMonetaryActivationRecordV2, ProtocolRestoreIdentityGate,
};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

use super::{
    protocol_identity::{
        PROTOCOL_ACTIVATION_STORAGE_KEY, PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY,
    },
    SnapshotExportBundle, SnapshotVerificationReport, Storage, ACCEPTED_BLOCKS_CF,
};

pub const PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION: u32 = 2;
pub const MONETARY_PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION: u32 = 3;

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

/// Additive v3 snapshot envelope that carries the exact monetary policy and
/// cadence binding alongside the existing protocol-bound v2 snapshot.
///
/// The inner v2 bundle remains byte/schema compatible for historical consumers.
/// v3 restore must validate both records before any durable mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMonetarySnapshotExportBundleV3 {
    pub format_version: u32,
    pub protocol_bundle: ProtocolSnapshotExportBundleV2,
    pub monetary_record: ProtocolMonetaryActivationRecordV2,
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

        if expected.consensus_mode == ProtocolConsensusMode::GhostdagV1 {
            let diagnostics = verify_authoritative_state_snapshot_v2(
                &bundle.legacy_bundle.snapshot,
            )
            .map_err(|error| {
                storage_error(format!(
                    "activated-v2 snapshot is not authoritatively materialized: {error:?}"
                ))
            })?;
            let finality =
                derive_finality_boundary_v1(&bundle.legacy_bundle.snapshot).map_err(|error| {
                    storage_error(format!(
                        "activated-v2 snapshot finality boundary is not derivable: {error:?}"
                    ))
                })?;
            if finality.protocol_identity != *expected {
                return Err(storage_error(
                    "activated-v2 snapshot finality identity does not match envelope identity",
                ));
            }
            if finality.ordered_dag_digest != diagnostics.ordered_dag_digest {
                return Err(storage_error(
                    "activated-v2 snapshot finality digest does not match authoritative ordering",
                ));
            }
        }

        Ok(report)
    }

    /// Verify the complete protocol-bound bundle before mutation, then replace
    /// accepted blocks, snapshot metadata/state, and the activation sidecar in
    /// one RocksDB batch. Any identity or bundle verification failure returns
    /// before durable state is changed.
    pub fn import_protocol_snapshot_bundle_v2(
        &self,
        bundle: ProtocolSnapshotExportBundleV2,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        let report = self.verify_protocol_snapshot_bundle_v2(&bundle, expected)?;
        let blocks_cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| storage_error("missing cf accepted blocks"))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let existing_blocks = self.list_blocks()?;
        let mut batch = WriteBatch::default();

        for block in existing_blocks {
            batch.delete_cf(&blocks_cf, block.hash.as_bytes());
        }
        for block in &bundle.legacy_bundle.persisted_blocks {
            batch.put_cf(
                &blocks_cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|error| storage_error(error.to_string()))?,
            );
        }
        self.stage_chain_state_snapshot_with_captured_at(
            &mut batch,
            &meta_cf,
            &bundle.legacy_bundle.snapshot,
            bundle
                .legacy_bundle
                .snapshot_captured_at_unix
                .unwrap_or(bundle.legacy_bundle.exported_at_unix),
        )?;
        batch.put_cf(
            &meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&bundle.activation_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))?;
        Ok(report)
    }
    /// Export the additive v3 monetary snapshot envelope only when both durable
    /// sidecars are present and match the caller's exact identity and cadence.
    pub fn export_monetary_protocol_snapshot_bundle_v3(
        &self,
        expected: &ProtocolActivationIdentity,
        expected_cadence: &[MonetaryCadenceSegment],
        expected_reward_finality_policy_version: &str,
    ) -> Result<
        (
            ProtocolMonetarySnapshotExportBundleV3,
            SnapshotVerificationReport,
        ),
        PulseError,
    > {
        self.verify_persisted_monetary_identity(
            expected,
            expected_cadence,
            expected_reward_finality_policy_version,
        )?;
        let monetary_record = self.protocol_monetary_activation_record()?.ok_or_else(|| {
            storage_error("verified monetary activation sidecar disappeared before export")
        })?;
        monetary_record
            .verify_expected(
                expected,
                expected_cadence,
                expected_reward_finality_policy_version,
            )
            .map_err(storage_error)?;

        let (protocol_bundle, report) = self.export_protocol_snapshot_bundle_v2(expected)?;
        if protocol_bundle.activation_record.fingerprint != monetary_record.protocol_fingerprint {
            return Err(storage_error(
                "protocol and monetary snapshot sidecars disagree on protocol fingerprint",
            ));
        }

        Ok((
            ProtocolMonetarySnapshotExportBundleV3 {
                format_version: MONETARY_PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
                protocol_bundle,
                monetary_record,
            },
            report,
        ))
    }

    /// Verify a v3 monetary snapshot without mutating storage.
    pub fn verify_monetary_protocol_snapshot_bundle_v3(
        &self,
        bundle: &ProtocolMonetarySnapshotExportBundleV3,
        expected: &ProtocolActivationIdentity,
        expected_cadence: &[MonetaryCadenceSegment],
        expected_reward_finality_policy_version: &str,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        if bundle.format_version != MONETARY_PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION {
            return Err(storage_error(format!(
                "unsupported monetary protocol snapshot bundle format version {}; expected {}",
                bundle.format_version, MONETARY_PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
            )));
        }
        bundle
            .monetary_record
            .verify_expected(
                expected,
                expected_cadence,
                expected_reward_finality_policy_version,
            )
            .map_err(storage_error)?;
        if bundle.protocol_bundle.activation_record.fingerprint
            != bundle.monetary_record.protocol_fingerprint
        {
            return Err(storage_error(
                "protocol and monetary snapshot records disagree on protocol fingerprint",
            ));
        }
        self.verify_protocol_snapshot_bundle_v2(&bundle.protocol_bundle, expected)
    }

    /// Verify the complete v3 envelope before mutation, then replace accepted
    /// blocks, snapshot state, protocol identity and monetary identity in one
    /// RocksDB batch.
    pub fn import_monetary_protocol_snapshot_bundle_v3(
        &self,
        bundle: ProtocolMonetarySnapshotExportBundleV3,
        expected: &ProtocolActivationIdentity,
        expected_cadence: &[MonetaryCadenceSegment],
        expected_reward_finality_policy_version: &str,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        let report = self.verify_monetary_protocol_snapshot_bundle_v3(
            &bundle,
            expected,
            expected_cadence,
            expected_reward_finality_policy_version,
        )?;
        let blocks_cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| storage_error("missing cf accepted blocks"))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let existing_blocks = self.list_blocks()?;
        let mut batch = WriteBatch::default();

        for block in existing_blocks {
            batch.delete_cf(&blocks_cf, block.hash.as_bytes());
        }
        for block in &bundle.protocol_bundle.legacy_bundle.persisted_blocks {
            batch.put_cf(
                &blocks_cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|error| storage_error(error.to_string()))?,
            );
        }
        self.stage_chain_state_snapshot_with_captured_at(
            &mut batch,
            &meta_cf,
            &bundle.protocol_bundle.legacy_bundle.snapshot,
            bundle
                .protocol_bundle
                .legacy_bundle
                .snapshot_captured_at_unix
                .unwrap_or(bundle.protocol_bundle.legacy_bundle.exported_at_unix),
        )?;
        batch.put_cf(
            &meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&bundle.protocol_bundle.activation_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        batch.put_cf(
            &meta_cf,
            PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&bundle.monetary_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, MonetaryCadenceSegment, ProtocolActivationIdentity,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-protocol-bundle-{test_name}-{unique}"
            ))
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

        assert_eq!(
            bundle.format_version,
            PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
        );
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

    #[test]
    fn protocol_bundle_v2_import_persists_snapshot_and_identity_atomically() {
        let source_path = temp_db_path("import-source");
        let target_path = temp_db_path("import-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        source
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let (bundle, _) = source
            .export_protocol_snapshot_bundle_v2(&expected)
            .unwrap();

        let report = target
            .import_protocol_snapshot_bundle_v2(bundle, &expected)
            .unwrap();

        assert!(report.restore_guarantees_explicit);
        assert_eq!(
            target.load_chain_state().unwrap().unwrap().chain_id,
            state.chain_id
        );
        assert_eq!(
            target
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            expected
        );
        assert!(target.protocol_snapshot_sidecar_complete().unwrap());

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn protocol_bundle_v2_failed_verification_leaves_target_unchanged() {
        let source_path = temp_db_path("preverify-source");
        let target_path = temp_db_path("preverify-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let source_state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&source_state);
        source
            .persist_chain_state_with_protocol_record(&source_state)
            .unwrap();
        let (mut bundle, _) = source
            .export_protocol_snapshot_bundle_v2(&expected)
            .unwrap();
        bundle.format_version += 1;

        let target_state = init_chain_state("pulsedag-private".to_string());
        let target_identity = ProtocolActivationIdentity::legacy_from_state(&target_state);
        target
            .persist_chain_state_with_protocol_record(&target_state)
            .unwrap();

        assert!(target
            .import_protocol_snapshot_bundle_v2(bundle, &expected)
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

    const MONETARY_TEST_CADENCE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];
    const MONETARY_TEST_FINALITY: &str = "reward-finality-test-v1";

    #[test]
    fn monetary_protocol_bundle_v3_round_trips_both_sidecars_atomically() {
        let source_path = temp_db_path("monetary-v3-source");
        let target_path = temp_db_path("monetary-v3-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let state = init_chain_state("pulsedag-monetary-bundle-test".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        source
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();
        let (bundle, report) = source
            .export_monetary_protocol_snapshot_bundle_v3(
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        assert_eq!(
            bundle.format_version,
            MONETARY_PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
        );
        assert_eq!(
            bundle.protocol_bundle.activation_record.fingerprint,
            bundle.monetary_record.protocol_fingerprint
        );

        target
            .import_monetary_protocol_snapshot_bundle_v3(
                bundle,
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();

        assert!(target
            .monetary_protocol_snapshot_sidecar_complete()
            .unwrap());
        target
            .verify_persisted_monetary_identity(
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn monetary_protocol_bundle_v3_rejects_cadence_substitution_before_import() {
        let source_path = temp_db_path("monetary-v3-cadence-source");
        let target_path = temp_db_path("monetary-v3-cadence-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let state = init_chain_state("pulsedag-monetary-bundle-cadence".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        source
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();
        let (bundle, _) = source
            .export_monetary_protocol_snapshot_bundle_v3(
                &expected,
                &MONETARY_TEST_CADENCE,
                MONETARY_TEST_FINALITY,
            )
            .unwrap();

        let alternate = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 500_000_000,
        }];
        assert!(target
            .import_monetary_protocol_snapshot_bundle_v3(
                bundle,
                &expected,
                &alternate,
                MONETARY_TEST_FINALITY,
            )
            .is_err());
        assert!(target
            .protocol_monetary_activation_record()
            .unwrap()
            .is_none());
        assert!(target.load_chain_state().unwrap().is_none());

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }
}
