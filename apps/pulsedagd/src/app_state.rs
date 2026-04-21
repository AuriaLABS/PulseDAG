use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::RwLock;

use pulsedag_core::state::ChainState;
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::api::{NodeRuntimeStats, RpcStateLike};
use pulsedag_storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub chain: Arc<RwLock<ChainState>>,
    pub storage: Arc<Storage>,
    pub p2p: Option<Arc<dyn P2pHandle>>,
    pub runtime: Arc<RwLock<NodeRuntimeStats>>,
}

pub fn new_runtime_stats() -> NodeRuntimeStats {
    NodeRuntimeStats {
        started_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        accepted_p2p_blocks: 0,
        rejected_p2p_blocks: 0,
        duplicate_p2p_blocks: 0,
        queued_orphan_blocks: 0,
        adopted_orphan_blocks: 0,
        accepted_p2p_txs: 0,
        rejected_p2p_txs: 0,
        duplicate_p2p_txs: 0,
        accepted_mined_blocks: 0,
        rejected_mined_blocks: 0,
        startup_snapshot_exists: false,
        startup_persisted_block_count: 0,
        startup_persisted_max_height: 0,
        startup_consistency_issue_count: 0,
        startup_recovery_mode: "unknown".to_string(),
        startup_rebuild_reason: None,
        last_self_audit_unix: None,
        last_self_audit_ok: true,
        last_self_audit_issue_count: 0,
        last_self_audit_message: None,
        last_observed_best_height: 0,
        last_height_change_unix: None,
        active_alerts: Vec::new(),
        mempool_sanitize_runs: 0,
    }
}

impl RpcStateLike for AppState {
    fn chain(&self) -> Arc<RwLock<ChainState>> {
        self.chain.clone()
    }
    fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
        self.p2p.clone()
    }
    fn storage(&self) -> Arc<Storage> {
        self.storage.clone()
    }
    fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
        self.runtime.clone()
    }
}
