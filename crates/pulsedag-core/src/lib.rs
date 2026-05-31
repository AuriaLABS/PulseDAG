pub mod accept;
pub mod apply;
pub mod consistency;
pub mod errors;
pub mod genesis;
pub mod mempool;
pub mod mining;
pub mod orphans;
pub mod pow;
pub mod replay;
pub mod retarget;
pub mod selection;
pub mod state;
pub mod sync_pipeline;
pub mod tx;
pub mod types;
pub mod validation;

pub use accept::{
    accept_block, accept_block_atomically, accept_block_with_result, accept_transaction,
    accept_transaction_with_result, AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult,
    TxAcceptanceResult,
};
pub use errors::PulseError;
pub use state::{
    ChainState, ContractRuntimeConfig, ContractRuntimeState, DagState, Mempool, UtxoState,
};
pub use tx::{
    address_from_public_key, compute_txid, signing_message, verify_transaction_signatures,
};
pub use types::*;

pub use retarget::{
    consensus_difficulty_snapshot, expected_difficulty, expected_target_u64,
    ConsensusDifficultySnapshot, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
};

pub use replay::{
    rebuild_state_from_blocks, rebuild_state_from_blocks_defensive,
    rebuild_state_from_snapshot_and_blocks, sort_blocks_for_deterministic_replay,
    ReplayDefensiveReport,
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

pub use mempool::{
    combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps, reconcile_mempool,
    MempoolPressureTier, MempoolReconcileResult,
};

pub use selection::{preferred_tip_hash, sorted_tip_hashes};

pub use consistency::{assert_dag_consistent_for_tests, dag_consistency_issues};

pub use orphans::{
    adopt_ready_orphans, adopt_ready_orphans_with_result, missing_block_parents,
    orphan_children_waiting_for_parent, pending_missing_parent_count, prune_orphans,
    queue_orphan_block, queue_orphan_block_bounded, OrphanAdoptionResult, OrphanQueueResult,
    DEFAULT_ORPHAN_MAX_AGE_MS, DEFAULT_ORPHAN_MAX_COUNT,
};

pub use mining::{
    build_candidate_block, build_coinbase_transaction, current_ts, is_coinbase,
    refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
};
pub use sync_pipeline::{
    rank_sync_candidates, RankedSyncPeer, SyncPeerCandidate, SyncPhase, SyncPipelineStatus,
    SyncProgressCounters,
};
pub use validation::{
    block_subsidy, total_block_fees, validate_coinbase_reward, validate_created_utxo_outpoints,
    INITIAL_BLOCK_SUBSIDY, SUBSIDY_HALVING_INTERVAL,
};
