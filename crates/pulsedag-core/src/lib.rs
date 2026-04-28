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
pub mod selection;
pub mod state;
pub mod sync_pipeline;
pub mod tx;
pub mod types;
pub mod validation;

pub use accept::{accept_block, accept_transaction, AcceptSource};
pub use errors::PulseError;
pub use state::{
    ChainState, ContractRuntimeConfig, ContractRuntimeState, DagState, Mempool, UtxoState,
};
pub use tx::{
    address_from_public_key, compute_txid, signing_message, verify_transaction_signatures,
};
pub use types::*;

pub use replay::{
    rebuild_state_from_blocks, rebuild_state_from_blocks_defensive,
    rebuild_state_from_snapshot_and_blocks, ReplayDefensiveReport,
};

pub use pow::{
    canonical_pow_engine, dev_adjust_difficulty_for_interval, dev_base_difficulty,
    dev_current_difficulty_for_chain, dev_difficulty_policy, dev_difficulty_snapshot,
    dev_difficulty_use_median, dev_difficulty_window, dev_hash_score_u64,
    dev_max_future_drift_secs, dev_mine_header, dev_pow_accepts,
    dev_recent_avg_block_interval_secs, dev_recent_block_interval_secs_with_mode,
    dev_recommended_difficulty, dev_recommended_difficulty_for_chain, dev_retarget_multiplier_bps,
    dev_surrogate_pow_hash, dev_target_block_interval_secs, dev_target_u64, mine_header,
    pow_accepts, pow_evaluate, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes,
    pow_preimage_string, pow_target_u64, selected_pow_algorithm, selected_pow_name,
    CanonicalPowEngine, DevDifficultyPolicy, DevDifficultySnapshot, PowAlgorithm, PowEngine,
    PowEvaluation, PowHeaderPreimage, POW_HEADER_PREIMAGE_VERSION,
};

pub use mempool::{
    combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps, reconcile_mempool,
    MempoolPressureTier, MempoolReconcileResult,
};

pub use selection::{preferred_tip_hash, sorted_tip_hashes};

pub use consistency::dag_consistency_issues;

pub use orphans::{
    adopt_ready_orphans, missing_block_parents, prune_orphans, queue_orphan_block,
    DEFAULT_ORPHAN_MAX_AGE_MS, DEFAULT_ORPHAN_MAX_COUNT,
};

pub use mining::{build_candidate_block, build_coinbase_transaction, current_ts, is_coinbase};
pub use sync_pipeline::{
    rank_sync_candidates, RankedSyncPeer, SyncPeerCandidate, SyncPhase, SyncPipelineStatus,
    SyncProgressCounters,
};
