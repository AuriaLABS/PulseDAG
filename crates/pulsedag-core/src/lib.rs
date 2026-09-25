pub mod accept;
pub mod acceptance_v2;
pub mod access_set_v1;
pub mod apply;
pub mod based_app_v0;
pub mod channel_v1;
pub mod colored_utxo_v1;
pub mod consensus_metadata;
pub mod consistency;
#[allow(clippy::too_many_arguments)]
pub mod contract_state_v1;
pub mod contract_v3;
pub mod contracts_gate;
pub mod covenant_utxo_v1;
pub mod covenant_v1;
pub mod errors;
pub mod finality_envelope_v1;
pub mod finality_v2;
pub mod genesis;
pub mod genesis_v2;
pub mod ghostdag;
pub mod ghostdag_v1;
pub mod header_v2;
pub mod htlc_v1;
pub mod mempool;
pub mod mempool_admission_v3;
pub mod mempool_protocol;
pub mod mempool_replacement_v3;
pub mod mempool_resource_v1;
pub mod mempool_v3;
pub mod mined_block_v2;
pub mod mining;
pub mod mining_protocol;
pub mod mining_state_v2;
pub mod mining_template_v2;
pub mod mining_v2;
pub mod multisig_v1;
pub mod network_block_v2;
pub mod network_context_v2;
pub mod network_runtime_v2;
pub mod network_staging_v2;
pub mod ordering;
pub mod ordering_v2;
pub mod orphans;
pub mod pay_stream_v1;
pub mod pow;
pub mod pow_protocol;
pub mod pow_v2;
pub mod pqc;
pub mod protocol;
pub mod protocol_persistence;
pub mod pulseclock_v1;
pub mod pulsescript_vm_v1;
pub mod replay;
pub mod retarget;
pub mod selection;
pub mod selection_v2;
pub mod snapshot_transfer;
pub mod state;
pub mod state_replay_v2;
pub mod sync_pipeline;
pub mod tx;
pub mod tx_protocol;
pub mod tx_rejection;
pub mod tx_submission;
pub mod tx_v3;
pub mod types;
pub mod validation;
pub mod validation_v2;
pub mod vault_v1;
pub mod verify_receipt_v1;
pub mod work_cert_v1;

pub use accept::{
    accept_block, accept_block_atomically, accept_block_with_result, accept_transaction,
    accept_transaction_for_protocol, accept_transaction_with_result,
    accept_transaction_with_result_for_protocol, canonical_state_apply_latency_summary,
    mutate_chain_state_serialized, AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult,
    CanonicalStateApplyLatencySummary, ChainStateMutationOutcome, TxAcceptanceResult,
};
pub use acceptance_v2::{commit_ghostdag_v1_metadata_for_activated_v2, ActivatedV2MetadataCommit};
pub use contracts_gate::{
    contracts_compile_identity, contracts_compile_time_executable, contracts_may_execute,
    reject_inactive_contract_apply, EXECUTABLE_CONTRACTS_COMPILED,
};
pub use errors::{
    InvalidStateRootClassification, InvalidStateRootDiagnostics, InvalidStateRootError, PulseError,
};
pub use pqc::{
    address_from_hybrid_public_key_v1, decode_hybrid_public_key_v1, decode_hybrid_signature_v1,
    encode_hybrid_public_key_v1, encode_hybrid_signature_v1, HybridPublicKeyV1, HybridSignatureV1,
    ED25519_PUBLIC_KEY_BYTES, ED25519_SIGNATURE_BYTES, ML_DSA_65_PUBLIC_KEY_BYTES,
    ML_DSA_65_SIGNATURE_BYTES, PQC_ENVELOPE_VERSION_V1,
};
pub use state::{
    ChainState, ConsensusMode, ContractRuntimeConfig, ContractRuntimeState, DagState, Mempool,
    SelectedParentPolicy, UtxoState,
};
pub use tx::{
    address_from_public_key, canonical_transaction_bytes_v2,
    canonical_unsigned_transaction_bytes_v2, compute_txid, compute_txid_v2, signing_message,
    signing_message_v2, validate_transaction_version_v1, verify_transaction_signatures,
    verify_transaction_signatures_v2, TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};
pub use tx_protocol::{
    resolve_transaction_validation_path, validate_transaction_for_protocol,
    TransactionValidationPath,
};
pub use tx_rejection::{
    classify_transaction_version, classify_typed_transaction_error, TransactionRejectionClass,
};
pub use tx_submission::compute_submission_id_v2;
pub use tx_v3::{
    canonical_transaction_bytes_v3, canonical_unsigned_transaction_bytes_v3, compute_txid_v3,
    signing_message_v3, TRANSACTION_VERSION_V3,
};
pub use types::*;

pub use retarget::{
    consensus_difficulty_snapshot, expected_difficulty, expected_target_u64,
    ConsensusDifficultySnapshot, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
};

pub use replay::{
    compact_snapshot_to_retained_blocks, merge_set_digest, ordered_dag_digest,
    rebuild_state_from_blocks, rebuild_state_from_blocks_defensive,
    rebuild_state_from_snapshot_and_blocks, selection_digest, sort_blocks_for_deterministic_replay,
    state_digest, ReplayDefensiveReport,
};

pub use snapshot_transfer::{
    snapshot_transfer_chunk_digest_v1, snapshot_transfer_payload_digest_v1,
};

pub use pow::{
    canonical_pow_adapter, canonical_pow_engine, compact_from_target,
    dev_adjust_difficulty_for_interval, dev_base_difficulty, dev_current_difficulty_for_chain,
    dev_difficulty_policy, dev_difficulty_snapshot, dev_difficulty_use_median,
    dev_difficulty_window, dev_hash_score_u64, dev_max_future_drift_secs, dev_mine_header,
    dev_pow_accepts, dev_recent_avg_block_interval_secs, dev_recent_block_interval_secs_with_mode,
    dev_recommended_difficulty, dev_recommended_difficulty_for_chain, dev_retarget_multiplier_bps,
    dev_surrogate_pow_hash, dev_target_block_interval_secs, dev_target_u64, mine_header,
    pow_accepts, pow_evaluate, pow_hash, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes,
    pow_preimage_string, pow_target_u64, pow_validation_result, selected_pow_algorithm,
    selected_pow_name, target_from_compact, validate_pow_header, validate_pow_preimage_encoding,
    verify_work, CanonicalPowAdapter, CanonicalPowAttempt, CanonicalPowEngine, CanonicalPowHash,
    CanonicalPowMaterial, CanonicalPowTarget, DevDifficultyPolicy, DevDifficultySnapshot,
    PowAlgorithm, PowEngine, PowEvaluation, PowHeaderPreimage, PowRejectReason,
    PowTargetComparison, PowValidationResult, POW_HEADER_PREIMAGE_VERSION,
};
pub use pow_protocol::{
    evaluate_pow_for_protocol, resolve_pow_identity_path, resolve_pow_validation_path,
    validate_pow_for_protocol, PowValidationPath,
};
pub use pow_v2::{canonical_pow_v2_adapter, CanonicalPowV2Adapter};

pub use protocol::{
    ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V1,
    BLOCK_HEADER_VERSION_V2,
};
pub use protocol_persistence::{
    verify_protocol_restore_identity, ProtocolActivationRecordV1, ProtocolRestoreIdentityGate,
    PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION,
};

pub use header_v2::{
    canonical_block_header_bytes_v2, canonical_mining_preimage_bytes_v2,
    canonicalize_block_parents_v2, compute_block_hash_v2, validate_block_header_v2_shape,
};

pub use consensus_metadata::{
    BlockConsensusMetadataV1, BlueScoreSemantics, ConsensusMetadataSnapshotV1,
    CONSENSUS_METADATA_SCHEMA_VERSION,
};

pub use finality_v2::{
    derive_finality_boundary_v1, FinalityBoundaryV1, FinalityV2Error,
    GHOSTDAG_V1_FINALITY_POLICY_VERSION,
};

pub use pulseclock_v1::{
    observe_pulse_v1, observe_pulse_v1_at, PulseClockV1Error, PulseObservationV1, PULSE_DOMAIN_V1,
    PULSE_FINALITY_DEPTH_UNPUBLISHED_V1, PULSE_UNCERTAINTY_POLICY_MAX_SECS_V1, PULSE_VERSION_V1,
    PULSE_WINDOW_K_V1,
};

pub use pay_stream_v1::{
    evaluate_pay_stream_withdraw_v1, matured_buckets_v1, validate_pay_stream_output_v1,
    withdrawable_buckets_v1, PayStreamAdmissionV1, PayStreamV1Error, PayStreamV1Output,
    PAY_STREAM_DOMAIN_V1, PAY_STREAM_TEMPLATE_ID_V1,
};

pub use htlc_v1::{
    evaluate_htlc_spend_v1, htlc_spend_signing_message_v1, payment_hash_v1, refund_height_v1,
    reserved_payment_hash_v1, validate_htlc_output_v1, HtlcAdmissionV1, HtlcSpendPathV1,
    HtlcSpendWitnessV1, HtlcV1Error, HtlcV1Output, HTLC_DOMAIN_V1, HTLC_TEMPLATE_ID_V1,
};

pub use vault_v1::{
    created_pulse_height_from_tip, emergency_unlock_height, evaluate_vault_spend_v1,
    owner_unlock_height, pulses_remaining_owner, validate_vault_output_v1,
    vault_spend_signing_message_v1, verify_vault_spend_signature_v1, VaultAdmissionV1,
    VaultSpendPathV1, VaultSpendWitnessV1, VaultV1Error, VaultV1Output, VAULT_DOMAIN_V1,
    VAULT_TEMPLATE_ID_V1,
};

pub use multisig_v1::{
    evaluate_multisig_spend_v1, multisig_spend_signing_message_v1, validate_multisig_output_v1,
    MultisigAdmissionV1, MultisigSpendWitnessV1, MultisigV1Error, MultisigV1Output,
    MULTISIG_DOMAIN_V1, MULTISIG_TEMPLATE_ID_V1,
};

pub use channel_v1::{
    challenge_deadline_v1, channel_spend_signing_message_v1, evaluate_channel_spend_v1,
    validate_channel_output_v1, ChannelAdmissionV1, ChannelSpendPathV1, ChannelSpendWitnessV1,
    ChannelStageV1, ChannelV1Error, ChannelV1Output, CHANNEL_DOMAIN_V1, CHANNEL_TEMPLATE_ID_V1,
};

pub use colored_utxo_v1::{
    derive_color_id_v1, evaluate_colored_mint_v1, evaluate_colored_units_v1,
    validate_colored_output_v1, ColoredAdmissionV1, ColoredSpendPathV1, ColoredV1Error,
    ColoredV1Output, COLORED_DOMAIN_V1, COLORED_TEMPLATE_ID_V1,
};

pub use access_set_v1::{
    access_sets_conflict_v1, inspects_underdeclared_v1, schedule_access_sets_v1,
    spends_underdeclared_v1, validate_access_set_v1, AccessConflictClassV1, AccessKeyIdV1,
    AccessOutpointV1, AccessSetAdmissionV1, AccessSetV1, AccessSetV1Error, ACCESS_SET_DOMAIN_V1,
    ACCESS_SET_VERSION_V1,
};

pub use based_app_v0::{
    derive_app_id_v0, evaluate_based_app_v0, settle_height_v0, BasedAppAdmissionV0, BasedAppPathV0,
    BasedAppProfileV0, BasedAppV0Error, BasedCommitV0, BasedDaModeV0, BASED_APP_DOMAIN_V0,
    BASED_COMMIT_TEMPLATE_V0,
};

pub use verify_receipt_v1::{
    compare_reconstructed_v1, reject_host_time_v1, validate_verify_receipt_v1,
    verify_receipt_canonical_bytes_v1, verify_receipt_digest_v1, VerifyReceiptV1,
    VerifyReceiptV1Error, VerifyResultV1, VERIFY_RECEIPT_DOMAIN_V1, VERIFY_RECEIPT_VERSION_V1,
};

pub use work_cert_v1::{
    issue_work_certificate_v1, validate_work_certificate_v1, work_meets_target_v1,
    WorkCertAdmissionV1, WorkCertV1Error, WorkCertificateV1, WORK_CERT_DOMAIN_V1,
    WORK_CERT_VERSION_V1,
};

pub use finality_envelope_v1::{
    operational_finality_lag_v1, publish_finality_envelope_v1, unpublished_envelope_v1,
    validate_measurements_v1, CadenceClassV1, FinalityEnvelopeAdmissionV1, FinalityEnvelopeV1,
    FinalityEnvelopeV1Error, FinalityMeasurementsV1, FINALITY_ENVELOPE_DOMAIN_V1,
    FINALITY_ENVELOPE_VERSION_V1,
};

pub use ordering::{
    derive_ordered_dag, ordered_dag_tip, refresh_ordered_dag, DAG_ORDERING_VERSION,
};

pub use ordering_v2::{
    derive_ordered_dag_v2, OrderedDagV2, OrderingV2Error, GHOSTDAG_V1_ORDERING_VERSION,
};

pub use state_replay_v2::{
    materialize_authoritative_state_v2, rebuild_authoritative_state_v2,
    verify_authoritative_state_snapshot_v2, StateReplayV2, StateReplayV2Diagnostics,
};

pub use apply::{
    accept_block_to_dag_metadata, apply_transaction, commit_rebuilt_state,
    rebuild_state_from_ordered_dag, refresh_ordered_dag_phase, refresh_selected_chain_phase,
    OrderedDagRebuild, OrderedDagRebuildDiagnostics,
};

pub use mempool::{
    canonical_mempool_txids, combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps,
    reconcile_mempool, MempoolPressureTier, MempoolReconcileResult,
};
pub use mempool_admission_v3::{
    accept_transaction_with_mempool_policy_v3,
    accept_transaction_with_mempool_policy_v3_for_protocol,
    mempool_policy_rejection_code_from_reason_v3, mempool_policy_rejection_detail_v3,
    mempool_policy_rejection_reason_v3, reconcile_mempool_with_production_policy_v3,
    reconcile_mempool_with_production_policy_v3_for_protocol,
};
pub use mempool_protocol::reconcile_mempool_for_protocol;
pub use mempool_replacement_v3::{
    assess_mempool_replacement_v3, MempoolReplacementAssessmentV3,
    MEMPOOL_REPLACEMENT_ASSESSMENT_V3_VERSION,
};
pub use mempool_resource_v1::{
    canonical_transaction_size_for_resource_v1, mempool_resource_rejection_code_from_reason_v1,
    mempool_resource_rejection_detail_v1, mempool_resource_rejection_reason_v1,
    normalize_production_mempool_resources_v1, production_mempool_resource_invariants_v1,
    MempoolResourceNormalizationV1, MempoolResourcePolicyV1, MempoolResourceRejectionV1,
    MEMPOOL_RESOURCE_EXPIRY_BOUNDARY_V1, MEMPOOL_RESOURCE_LIVE_MAX_AGE_BLOCKS_V1,
    MEMPOOL_RESOURCE_MAX_ORPHANS_V1, MEMPOOL_RESOURCE_MAX_SPENT_OUTPOINTS_V1,
    MEMPOOL_RESOURCE_MAX_TRANSACTIONS_V1, MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1,
    MEMPOOL_RESOURCE_ORPHAN_MAX_AGE_BLOCKS_V1, MEMPOOL_RESOURCE_POLICY_V1_VERSION,
};
pub use mempool_v3::{
    admission_order_key_v3, canonical_transaction_size_for_mempool_v3, fee_rate_v3, FeeRateV3,
    MempoolPolicyAssessmentErrorV3, MempoolPolicyRejectionV3, MempoolPolicyV3,
    FEE_RATE_SCALE_BYTES_V3, MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTIONS,
    MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTION_FEE, MEMPOOL_POLICY_V3_COMPAT_MIN_RELAY_FEE_RATE,
    MEMPOOL_POLICY_V3_VERSION,
};

pub use ghostdag::{
    calculate_merge_set, classify_merge_set, classify_merge_set_with_k, MergeSetClassification,
    MergeSetColor, MergeSetDiagnostics, DEFAULT_MERGE_SET_K,
};

pub use ghostdag_v1::{
    calculate_bounded_merge_set_v1, classify_merge_set_v1, BoundedMergeSetV1,
    GhostdagV1Classification, GhostdagV1Error, GhostdagV1Limits, GHOSTDAG_V1_K,
    GHOSTDAG_V1_MAX_ANCESTOR_VISITS, GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS,
    GHOSTDAG_V1_MAX_RELATION_VISITS,
};

pub use selection::{
    calculate_selected_parent, legacy_preferred_tip_hash, preferred_tip_hash,
    refresh_selected_chain, sorted_legacy_tip_hashes, sorted_tip_hashes,
};

pub use selection_v2::{
    calculate_selected_parent_v1, calculate_selected_tip_v1, compare_selection_scores_v1,
    rebuild_selected_chain_v1, SelectionScoreV1, SelectionV2Error, GHOSTDAG_V1_MAX_PARENTS,
};

pub use consistency::{assert_dag_consistent_for_tests, dag_consistency_issues};

pub use mined_block_v2::{
    accept_activated_v2_mined_block_atomically, prepare_activated_v2_mined_block_state,
};
pub use mining::{
    build_candidate_block, build_coinbase_transaction, current_ts, is_coinbase,
    refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
};
pub use mining_protocol::{
    derive_activated_v2_mining_parent_context, ActivatedV2MiningParentContext,
    MiningParentExclusionReasonV1, MiningParentExclusionV1,
};
pub use mining_state_v2::{
    finalize_activated_v2_mining_candidate_state, ActivatedV2MiningStateContext,
};
pub use mining_template_v2::{
    build_activated_v2_mining_template, ActivatedV2MiningTemplate, ActivatedV2MiningTemplateSpec,
    ACTIVATED_V2_MINING_TEMPLATE_SCHEMA_VERSION,
};
pub use mining_v2::{
    build_candidate_block_v2, build_coinbase_transaction_v2, refresh_block_consensus_ids_v2,
    CandidateBlockV2Spec,
};
pub use network_block_v2::{
    accept_activated_v2_p2p_block_atomically, preflight_activated_v2_p2p_block,
    prepare_activated_v2_p2p_block_state, ActivatedV2P2pDisposition,
};
pub use network_context_v2::{
    validate_activated_v2_p2p_block_context, ActivatedV2P2pContextDisposition,
    ActivatedV2P2pContextValidation,
};
pub use network_runtime_v2::{
    drive_activated_v2_p2p_block_atomically, drive_activated_v2_p2p_block_with_runtime_persistence,
    ActivatedV2P2pDriveResult, ActivatedV2P2pRuntime, ActivatedV2P2pRuntimeOutcome,
    ActivatedV2P2pRuntimePersistence, ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS,
};
pub use network_staging_v2::{
    promote_activated_v2_p2p_anchor_atomically, stage_activated_v2_p2p_block,
    ActivatedV2P2pPromotion, ActivatedV2P2pStageOutcome, ActivatedV2P2pStaging,
    ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS,
};
pub use orphans::{
    adopt_ready_orphans, adopt_ready_orphans_with_result, classify_orphan_backlog,
    evict_stale_orphans_bounded, mark_selected_segment_required_parent, missing_block_parents,
    orphan_children_waiting_for_parent, orphan_missing_roots, pending_missing_parent_count,
    prune_historical_terminal_missing_parents, prune_orphans, quarantined_missing_parent_count,
    queue_orphan_block, queue_orphan_block_bounded, rebuild_orphan_parent_index,
    revalidate_orphan_backlog, terminal_missing_parent_active_blocking_count,
    terminal_missing_parent_active_blocking_details, terminal_missing_parent_count,
    terminal_missing_parent_historical_count, terminal_missing_parent_reason,
    terminalize_residual_waiting_missing_parents,
    terminalize_residual_waiting_missing_parents_guarded, terminally_exhaust_missing_parent,
    MissingParentTerminalResult, OrphanAdoptionResult, OrphanBacklogClassification,
    OrphanQueueResult, ResidualMissingParentTerminalResult, DEFAULT_ORPHAN_MAX_AGE_MS,
    DEFAULT_ORPHAN_MAX_COUNT, DEFAULT_ORPHAN_RECOVERY_EVICT_LIMIT,
    DEFAULT_TERMINAL_MISSING_PARENT_HISTORY_LIMIT,
};
pub use sync_pipeline::{
    rank_sync_candidates, RankedSyncPeer, SyncPeerCandidate, SyncPhase, SyncPipelineStatus,
    SyncProgressCounters,
};
pub use validation::{
    block_subsidy, invalid_state_root_diagnostics, invalid_state_root_diagnostics_with_context,
    invalid_state_root_error, parent_state_context, selected_parent_for_state_validation,
    total_block_fees, validate_block, validate_coinbase_reward, validate_created_utxo_outpoints,
    INITIAL_BLOCK_SUBSIDY, SUBSIDY_HALVING_INTERVAL,
};
pub use validation_v2::validate_transaction_v2;
