use std::sync::Arc;

use pulsedag_core::state::ChainState;
use pulsedag_core::SyncPipelineStatus;
use pulsedag_p2p::P2pHandle;
use pulsedag_storage::Storage;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub use pulsedag_api::{
    ApiError, ApiMeta, ApiResponse, GetBlockTemplateRequest, MineRequest, SubmitMinedBlockRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSignRequest {
    pub private_key: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransferRequest {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub private_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxRequest {
    pub transaction: pulsedag_core::types::Transaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitBlockRequest {
    pub block: pulsedag_core::types::Block,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeRuntimeStats {
    pub started_at_unix: u64,
    pub accepted_p2p_blocks: u64,
    pub rejected_p2p_blocks: u64,
    pub duplicate_p2p_blocks: u64,
    pub queued_orphan_blocks: u64,
    pub adopted_orphan_blocks: u64,
    pub accepted_p2p_txs: u64,
    pub rejected_p2p_txs: u64,
    pub duplicate_p2p_txs: u64,
    pub dropped_p2p_txs: u64,
    pub dropped_p2p_txs_duplicate_mempool: u64,
    pub dropped_p2p_txs_duplicate_confirmed: u64,
    pub dropped_p2p_txs_accept_failed: u64,
    pub dropped_p2p_txs_persist_failed: u64,
    pub tx_rebroadcast_attempts: u64,
    pub tx_rebroadcast_success: u64,
    pub tx_rebroadcast_failed: u64,
    pub tx_rebroadcast_skipped_no_p2p: u64,
    pub tx_rebroadcast_skipped_no_peers: u64,
    pub last_tx_rebroadcast_unix: Option<u64>,
    pub last_tx_rebroadcast_error: Option<String>,
    pub tx_inbound_total: u64,
    pub tx_inbound_accepted_total: u64,
    pub tx_inbound_rejected_total: u64,
    pub tx_inbound_dropped_total: u64,
    pub last_tx_accept_unix: Option<u64>,
    pub last_tx_reject_unix: Option<u64>,
    pub last_tx_drop_unix: Option<u64>,
    pub last_tx_drop_reason: Option<String>,
    pub last_tx_drop_txid: Option<String>,
    pub tx_drop_reasons: Vec<String>,
    pub accepted_mined_blocks: u64,
    pub rejected_mined_blocks: u64,
    pub external_mining_templates_emitted: u64,
    pub external_mining_templates_invalidated: u64,
    pub external_mining_stale_work_detected: u64,
    pub external_mining_submit_accepted: u64,
    pub external_mining_submit_rejected: u64,
    pub external_mining_rejected_invalid_pow: u64,
    pub external_mining_rejected_stale_template: u64,
    pub external_mining_rejected_unknown_template: u64,
    pub external_mining_rejected_submit_block_error: u64,
    pub external_mining_rejected_duplicate_block: u64,
    pub external_mining_rejected_invalid_block: u64,
    pub external_mining_rejected_chain_id_mismatch: u64,
    pub external_mining_rejected_internal_error: u64,
    pub external_mining_rejected_storage_error: u64,
    pub external_mining_last_template_id: Option<String>,
    pub external_mining_last_rejection_kind: Option<String>,
    pub external_mining_last_rejection_reason: Option<String>,
    pub external_mining_last_invalid_pow_reason: Option<String>,
    pub startup_snapshot_exists: bool,
    pub startup_persisted_block_count: usize,
    pub startup_persisted_max_height: u64,
    pub startup_consistency_issue_count: usize,
    pub startup_recovery_mode: String,
    pub startup_rebuild_reason: Option<String>,
    pub startup_path: String,
    pub startup_bootstrap_mode: String,
    pub startup_status_summary: String,
    pub startup_fastboot_used: bool,
    pub startup_snapshot_detected: bool,
    pub startup_snapshot_validated: bool,
    pub startup_delta_applied: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
    pub startup_duration_ms: u128,
    pub last_self_audit_unix: Option<u64>,
    pub last_self_audit_ok: bool,
    pub last_self_audit_issue_count: usize,
    pub last_self_audit_message: Option<String>,
    pub last_observed_best_height: u64,
    pub last_height_change_unix: Option<u64>,
    pub active_alerts: Vec<String>,
    pub snapshot_auto_every_blocks: u64,
    pub auto_prune_enabled: bool,
    pub auto_prune_every_blocks: u64,
    pub prune_keep_recent_blocks: u64,
    pub prune_require_snapshot: bool,
    pub last_snapshot_height: Option<u64>,
    pub last_snapshot_unix: Option<u64>,
    pub last_prune_height: Option<u64>,
    pub last_prune_unix: Option<u64>,
    pub sync_pipeline: SyncPipelineStatus,
}

pub trait RpcStateLike: Clone + Send + Sync + 'static {
    fn chain(&self) -> Arc<RwLock<ChainState>>;
    fn p2p(&self) -> Option<Arc<dyn P2pHandle>>;
    fn storage(&self) -> Arc<Storage>;
    fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRebuildRequest {
    pub force: bool,
    pub persist_after_rebuild: Option<bool>,
    pub reconcile_mempool: Option<bool>,
    pub allow_partial_replay: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningWorkerHeartbeatRequest {
    pub worker_id: String,
    pub miner_address: String,
    pub templates_requested: u64,
    pub blocks_submitted: u64,
    pub accepted_blocks: u64,
    pub stale_rejections: u64,
    pub invalid_pow_rejections: u64,
    pub accepted_shares: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimMiningJobRequest {
    pub worker_id: String,
    pub miner_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMiningJobRequest {
    pub worker_id: String,
    pub job_id: String,
    pub block: pulsedag_core::types::Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureMiningWorkerRequest {
    pub worker_id: String,
    pub share_difficulty: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMiningShareRequest {
    pub worker_id: String,
    pub job_id: String,
    pub header: pulsedag_core::types::BlockHeader,
}
