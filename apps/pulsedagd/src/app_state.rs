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
        dropped_p2p_txs: 0,
        dropped_p2p_txs_duplicate_mempool: 0,
        dropped_p2p_txs_duplicate_confirmed: 0,
        dropped_p2p_txs_accept_failed: 0,
        dropped_p2p_txs_persist_failed: 0,
        tx_rebroadcast_attempts: 0,
        tx_rebroadcast_success: 0,
        tx_rebroadcast_failed: 0,
        tx_rebroadcast_skipped_no_p2p: 0,
        tx_rebroadcast_skipped_no_peers: 0,
        last_tx_rebroadcast_unix: None,
        last_tx_rebroadcast_error: None,
        tx_inbound_total: 0,
        tx_inbound_accepted_total: 0,
        tx_inbound_rejected_total: 0,
        tx_inbound_dropped_total: 0,
        last_tx_accept_unix: None,
        last_tx_reject_unix: None,
        last_tx_drop_unix: None,
        last_tx_drop_reason: None,
        last_tx_drop_txid: None,
        tx_drop_reasons: Vec::new(),
        accepted_mined_blocks: 0,
        rejected_mined_blocks: 0,
        mining_templates_issued: 0,
        mining_template_refresh_events: 0,
        mining_template_invalidations: 0,
        mining_submit_total: 0,
        mining_submit_accepted: 0,
        mining_submit_rejected: 0,
        mining_submit_rejected_stale: 0,
        mining_submit_rejected_invalid_pow: 0,
        mining_submit_rejected_unknown_template: 0,
        mining_submit_rejected_storage: 0,
        mining_submit_rejected_other: 0,
        mining_submit_broadcast_success: 0,
        mining_submit_broadcast_failed: 0,
        mining_stale_work_indicated: 0,
        mining_submit_traces_completed: 0,
        mining_last_template_id: None,
        mining_last_template_created_unix: None,
        mining_last_submit_block_hash: None,
        mining_last_submit_unix: None,
        mining_last_accept_unix: None,
        mining_last_broadcast_unix: None,
        mining_last_rejection_code: None,
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
        snapshot_auto_every_blocks: 0,
        auto_prune_enabled: false,
        auto_prune_every_blocks: 0,
        prune_keep_recent_blocks: 0,
        prune_require_snapshot: true,
        last_snapshot_height: None,
        last_snapshot_unix: None,
        last_prune_height: None,
        last_prune_unix: None,
        sync_pipeline: pulsedag_core::SyncPipelineStatus::default(),
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

#[cfg(test)]
mod tests {
    use super::new_runtime_stats;

    #[test]
    fn runtime_stats_restart_snapshot_is_coherent() {
        let mut before_restart = new_runtime_stats();
        before_restart.mining_submit_total = 9;
        before_restart.mining_submit_accepted = 4;
        before_restart.mining_submit_rejected = 5;
        before_restart.mining_template_invalidations = 3;

        let after_restart = new_runtime_stats();
        assert!(after_restart.started_at_unix > 0);
        assert_eq!(after_restart.mining_submit_total, 0);
        assert_eq!(after_restart.mining_submit_accepted, 0);
        assert_eq!(after_restart.mining_submit_rejected, 0);
        assert_eq!(after_restart.mining_template_invalidations, 0);
    }
}
