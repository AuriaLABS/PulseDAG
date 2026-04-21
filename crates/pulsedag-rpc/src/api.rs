use std::sync::Arc;

use pulsedag_core::state::ChainState;
use pulsedag_p2p::P2pHandle;
use pulsedag_storage::Storage;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ApiMeta {}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            meta: ApiMeta::default(),
        }
    }
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
            meta: ApiMeta::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineRequest {
    pub miner_address: String,
    pub pow_max_tries: Option<u64>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBlockTemplateRequest {
    pub miner_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMinedBlockRequest {
    pub template_id: Option<String>,
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
    pub accepted_mined_blocks: u64,
    pub rejected_mined_blocks: u64,
    pub startup_snapshot_exists: bool,
    pub startup_persisted_block_count: usize,
    pub startup_persisted_max_height: u64,
    pub startup_consistency_issue_count: usize,
    pub startup_recovery_mode: String,
    pub startup_rebuild_reason: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn api_response_ok_shape_is_stable() {
        let resp = ApiResponse::ok(123u64);
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["ok"], Value::Bool(true));
        assert_eq!(value["data"], Value::from(123u64));
        assert!(value["error"].is_null());
        assert!(value["meta"].is_object());
    }

    #[test]
    fn api_response_err_shape_is_stable() {
        let resp: ApiResponse<u64> = ApiResponse::err("BAD_REQUEST", "invalid payload");
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["ok"], Value::Bool(false));
        assert!(value["data"].is_null());
        assert_eq!(value["error"]["code"], Value::from("BAD_REQUEST"));
        assert_eq!(value["error"]["message"], Value::from("invalid payload"));
        assert!(value["meta"].is_object());
    }
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
