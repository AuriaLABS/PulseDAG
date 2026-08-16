use pulsedag_core::{
    genesis::init_chain_state, materialize_authoritative_state_v2,
    verify_authoritative_state_snapshot_v2, ChainState, ProtocolActivationIdentity,
    ProtocolActivationRecordV1, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_storage::{
    ProtocolSnapshotExportBundleV2, Storage, PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
};

fn temp_db_path(test_name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!(
            "pulsedag-task26-protocol-snapshot-{test_name}-{unique}"
        ))
        .to_string_lossy()
        .into_owned()
}

fn activated_metadata_state(chain_id: &str) -> (ChainState, ProtocolActivationIdentity) {
    let mut state = init_chain_state(chain_id.to_string());
    let genesis = state.dag.genesis_hash.clone();
    state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
    state.dag.merge_set_reds.insert(genesis, vec![]);
    let expected = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    (state, expected)
}

fn protocol_bundle_from_state(
    storage: &Storage,
    state: &ChainState,
    expected: &ProtocolActivationIdentity,
) -> ProtocolSnapshotExportBundleV2 {
    storage.persist_chain_state(state).unwrap();
    let (legacy_bundle, report) = storage
        .export_snapshot_bundle(Some(&state.chain_id))
        .unwrap();
    assert!(report.restore_guarantees_explicit);
    ProtocolSnapshotExportBundleV2 {
        format_version: PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
        activation_record: ProtocolActivationRecordV1::from_identity(expected.clone()).unwrap(),
        legacy_bundle,
    }
}

#[test]
fn activated_v2_materialized_snapshot_round_trips_through_protocol_bundle() {
    let source_path = temp_db_path("roundtrip-source");
    let target_path = temp_db_path("roundtrip-target");
    let source = Storage::open(&source_path).unwrap();
    let target = Storage::open(&target_path).unwrap();

    let (metadata_state, expected) = activated_metadata_state("task26-snapshot-v2");
    let materialized = materialize_authoritative_state_v2(&metadata_state).unwrap();
    let expected_diagnostics = verify_authoritative_state_snapshot_v2(&materialized).unwrap();
    let bundle = protocol_bundle_from_state(&source, &materialized, &expected);

    let report = target
        .verify_protocol_snapshot_bundle_v2(&bundle, &expected)
        .unwrap();
    assert!(report.restore_guarantees_explicit);
    target
        .import_protocol_snapshot_bundle_v2(bundle, &expected)
        .unwrap();

    let restored = target.load_chain_state().unwrap().unwrap();
    let restored_diagnostics = verify_authoritative_state_snapshot_v2(&restored).unwrap();
    assert_eq!(restored_diagnostics, expected_diagnostics);
    assert_eq!(
        restored.utxo.compute_state_root().unwrap(),
        expected_diagnostics.state_root
    );
    assert_eq!(
        target
            .protocol_activation_record()
            .unwrap()
            .unwrap()
            .identity,
        expected
    );

    let (reexported, reexport_report) = target
        .export_protocol_snapshot_bundle_v2(&expected)
        .unwrap();
    assert!(reexport_report.restore_guarantees_explicit);
    assert!(target
        .verify_protocol_snapshot_bundle_v2(&reexported, &expected)
        .is_ok());

    drop(source);
    drop(target);
    let _ = std::fs::remove_dir_all(source_path);
    let _ = std::fs::remove_dir_all(target_path);
}

#[test]
fn activated_v2_stale_unmaterialized_snapshot_is_rejected_before_import() {
    let source_path = temp_db_path("stale-source");
    let target_path = temp_db_path("stale-target");
    let source = Storage::open(&source_path).unwrap();
    let target = Storage::open(&target_path).unwrap();

    let (stale_state, expected) = activated_metadata_state("task26-stale-v2");
    assert!(verify_authoritative_state_snapshot_v2(&stale_state).is_err());
    let bundle = protocol_bundle_from_state(&source, &stale_state, &expected);

    let target_state = init_chain_state("task26-existing-target".to_string());
    let target_identity = ProtocolActivationIdentity::legacy_from_state(&target_state);
    target
        .persist_chain_state_with_protocol_record(&target_state)
        .unwrap();

    assert!(target
        .verify_protocol_snapshot_bundle_v2(&bundle, &expected)
        .is_err());
    assert!(target
        .import_protocol_snapshot_bundle_v2(bundle, &expected)
        .is_err());

    let unchanged = target.load_chain_state().unwrap().unwrap();
    assert_eq!(unchanged.chain_id, target_state.chain_id);
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
