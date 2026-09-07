use std::collections::BTreeMap;

use pulsedag_core::{
    errors::PulseError, ActivatedV2P2pRuntime, ProtocolActivationIdentity, ProtocolConsensusMode,
};
use rocksdb::WriteBatch;

use super::{
    ActivatedV2P2pRuntimeRecordV1, FastSyncNetworkTransferPlanV1, SnapshotVerificationReport,
    Storage, ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION,
    ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY, PROTOCOL_ACTIVATION_STORAGE_KEY, ACCEPTED_BLOCKS_CF,
};

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

impl Storage {
    /// Import one fully verified live FastSync transfer into an activated-v2
    /// clean node without creating a restart gap between authoritative chain
    /// state and the activated P2P runtime sidecar.
    ///
    /// The network compact plan/chunks are fully decoded and verified before
    /// this method creates the write batch. Accepted blocks, chain snapshot,
    /// protocol identity, local accepted-storage generation and an empty runtime
    /// sidecar are then committed together. A successful return can therefore
    /// be reloaded immediately through the normal activated-v2 startup path.
    pub fn import_complete_fast_sync_bootstrap_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        chunks: &BTreeMap<u32, Vec<u8>>,
        expected: &ProtocolActivationIdentity,
    ) -> Result<
        (
            SnapshotVerificationReport,
            pulsedag_core::ChainState,
            ActivatedV2P2pRuntime,
        ),
        PulseError,
    > {
        if expected.consensus_mode != ProtocolConsensusMode::GhostdagV1 {
            return Err(storage_error(
                "clean-node FastSync bootstrap import requires ghostdag_v1 activation identity",
            ));
        }

        let (bundle, report) =
            self.decode_complete_fast_sync_network_transfer_v1(plan, chunks, expected)?;
        let protocol_bundle = &bundle.snapshot_bundle;
        let legacy_bundle = &protocol_bundle.legacy_bundle;
        let imported_state = legacy_bundle.snapshot.clone();
        protocol_bundle
            .activation_record
            .verify_expected(expected)
            .map_err(storage_error)?;

        let runtime = ActivatedV2P2pRuntime::default();
        let runtime_record = ActivatedV2P2pRuntimeRecordV1 {
            format_version: ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION,
            activation_record: protocol_bundle.activation_record.clone(),
            chain_state_generation: imported_state.chain_state_generation,
            runtime: runtime.clone(),
        };
        runtime_record.verify_expected(expected, &imported_state)?;

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
        for block in &legacy_bundle.persisted_blocks {
            batch.put_cf(
                &blocks_cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|error| storage_error(error.to_string()))?,
            );
        }

        self.stage_accepted_storage_generation_advance(&mut batch, &meta_cf)?;
        self.stage_chain_state_snapshot_with_captured_at(
            &mut batch,
            &meta_cf,
            &imported_state,
            legacy_bundle
                .snapshot_captured_at_unix
                .unwrap_or(legacy_bundle.exported_at_unix),
        )?;
        batch.put_cf(
            &meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&protocol_bundle.activation_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        batch.put_cf(
            &meta_cf,
            ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY,
            serde_json::to_vec(&runtime_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );

        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))?;

        let (reloaded_state, reloaded_runtime) =
            self.load_activated_v2_p2p_runtime_snapshot(expected)?;
        Ok((report, reloaded_state, reloaded_runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis_v2::init_chain_state_v2,
        snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
        GHOSTDAG_V1_ORDERING_VERSION,
    };
    use crate::{
        FastSyncNetworkTransferPlanV1, PreparedFastSyncSnapshotTransferV1,
        FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-fast-sync-bootstrap-import-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn source_fixture(
        storage: &Storage,
    ) -> (
        ProtocolActivationIdentity,
        PreparedFastSyncSnapshotTransferV1,
        FastSyncNetworkTransferPlanV1,
    ) {
        let state = init_chain_state_v2("fast-sync-bootstrap-import-v2".to_string()).unwrap();
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let genesis = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        storage
            .persist_activated_v2_p2p_blocks_and_runtime(
                &[genesis],
                &expected,
                &state,
                &ActivatedV2P2pRuntime::default(),
            )
            .unwrap();
        let (bundle, report) = storage
            .export_fast_sync_snapshot_bundle_v1(&expected)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        let prepared = storage
            .prepare_fast_sync_snapshot_transfer_v1(&bundle, &expected, 256)
            .unwrap()
            .0;
        let transfer = &prepared.plan;
        let manifest = &transfer.snapshot_manifest;
        let plan = FastSyncNetworkTransferPlanV1 {
            plan_version: FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
            payload_encoding: transfer.payload_encoding.clone(),
            chain_id: manifest.chain_id.clone(),
            genesis_hash: manifest.genesis_hash.clone(),
            protocol_fingerprint: manifest.protocol_fingerprint.clone(),
            manifest_version: manifest.manifest_version,
            protocol_snapshot_bundle_format_version: manifest
                .protocol_snapshot_bundle_format_version,
            storage_schema_version: manifest.storage_schema_version,
            transfer_id: transfer.transfer_id.clone(),
            commitment_set_id: snapshot_transfer_commitment_set_digest_v1(
                &transfer.transfer_id,
                &transfer.chunk_commitments,
            ),
            payload_len: transfer.payload_len,
            chunk_size: transfer.chunk_size,
            chunk_count: transfer.chunk_count,
            chunk_commitments: transfer.chunk_commitments.clone(),
            best_height: manifest.best_height,
            selected_tip: manifest.selected_tip.clone(),
            state_commitment: manifest.state_commitment.clone(),
            prune_boundary_height: manifest.prune_boundary_height,
            snapshot_generation: manifest.snapshot_generation,
            accepted_storage_generation: manifest.accepted_storage_generation,
            delta_start_generation: manifest.delta_start_generation,
            delta_end_generation: manifest.delta_end_generation,
        };
        (expected, prepared, plan)
    }

    #[test]
    fn activated_fast_sync_import_is_immediately_restartable() {
        let source_path = temp_db_path("source");
        let target_path = temp_db_path("target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let (expected, prepared, plan) = source_fixture(&source);
        let mut chunks = BTreeMap::new();
        for chunk_index in 0..prepared.plan.chunk_count {
            chunks.insert(chunk_index, prepared.chunk(chunk_index).unwrap().to_vec());
        }

        let before_generation = target.accepted_storage_generation().unwrap();
        let (report, imported_state, imported_runtime) = target
            .import_complete_fast_sync_bootstrap_v1(&plan, &chunks, &expected)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        assert!(imported_runtime.pending_is_empty());
        assert!(imported_runtime.staging().is_empty());
        assert!(target.accepted_storage_generation().unwrap() > before_generation);

        let (restarted_state, restarted_runtime) = target
            .load_activated_v2_p2p_runtime_snapshot(&expected)
            .unwrap();
        assert_eq!(restarted_state.dag.best_height, imported_state.dag.best_height);
        assert_eq!(
            restarted_state.chain_state_generation,
            imported_state.chain_state_generation
        );
        assert!(restarted_runtime.pending_is_empty());
        assert!(restarted_runtime.staging().is_empty());
        assert_eq!(
            target
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            expected
        );

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn bootstrap_import_rejects_non_activated_identity_before_mutation() {
        let source_path = temp_db_path("legacy-source");
        let target_path = temp_db_path("legacy-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let (expected_v2, prepared, plan) = source_fixture(&source);
        let legacy = ProtocolActivationIdentity::legacy_default_for_chain(
            expected_v2.chain_id.clone(),
            expected_v2.genesis_hash.clone(),
        );
        let chunks = (0..prepared.plan.chunk_count)
            .map(|chunk_index| {
                (
                    chunk_index,
                    prepared.chunk(chunk_index).unwrap().to_vec(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert!(target
            .import_complete_fast_sync_bootstrap_v1(&plan, &chunks, &legacy)
            .is_err());
        assert!(target.load_chain_state().unwrap().is_none());

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }
}
