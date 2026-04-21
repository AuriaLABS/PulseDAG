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
    rebuild_state_from_blocks, rebuild_state_from_blocks_defensive, ReplayDefensiveReport,
};

pub use pow::{
    dev_adjust_difficulty_for_interval, dev_base_difficulty, dev_difficulty_window,
    dev_hash_score_u64, dev_max_future_drift_secs, dev_mine_header, dev_pow_accepts,
    dev_recent_avg_block_interval_secs, dev_recommended_difficulty,
    dev_recommended_difficulty_for_chain, dev_retarget_multiplier_bps, dev_surrogate_pow_hash,
    dev_target_block_interval_secs, dev_target_u64, pow_preimage_string, selected_pow_algorithm,
    selected_pow_name, PowAlgorithm,
};

pub use mempool::{
    evict_lowest_fee_density, mempool_policy, mempool_top, rebuild_spent_outpoints,
    reconcile_mempool, sanitize_mempool, MempoolPolicy, MempoolReconcileResult, MempoolTopItem,
};

pub use selection::{preferred_tip_hash, sorted_tip_hashes};

pub use consistency::dag_consistency_issues;

pub use orphans::{
    adopt_ready_orphans, missing_block_parents, prune_orphans, queue_orphan_block,
    DEFAULT_ORPHAN_MAX_AGE_MS, DEFAULT_ORPHAN_MAX_COUNT,
};

pub use mining::{build_candidate_block, build_coinbase_transaction, current_ts, is_coinbase};
