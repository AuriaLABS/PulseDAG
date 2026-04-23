use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::RwLock;

use pulsedag_core::state::ChainState;
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::api::{NodeRuntimeStats, RpcStateLike};
use pulsedag_storage::Storage;

#[derive(Debug, Clone)]
pub struct StartupPathReport {
    pub startup_path: String,
    pub startup_fastboot_used: bool,
    pub startup_snapshot_detected: bool,
    pub startup_snapshot_validated: bool,
    pub startup_delta_applied: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
}

pub fn derive_startup_path_report(
    startup_recovery_mode: &str,
    snapshot_exists: bool,
    persisted_block_count: usize,
    startup_rebuild_reason: Option<String>,
) -> StartupPathReport {
    let replayed_blocks = startup_recovery_mode == "replayed_blocks";
    let startup_fallback_reason = if replayed_blocks {
        startup_rebuild_reason
    } else {
        None
    };
    let startup_path = if replayed_blocks && startup_fallback_reason.is_some() {
        "fallback_full_replay"
    } else if replayed_blocks {
        "full_replay"
    } else if snapshot_exists {
        "fast_boot"
    } else if persisted_block_count > 0 {
        "full_replay"
    } else {
        "genesis_init"
    }
    .to_string();
    let startup_fastboot_used = startup_path == "fast_boot";

    StartupPathReport {
        startup_path,
        startup_fastboot_used,
        startup_snapshot_detected: snapshot_exists,
        startup_snapshot_validated: startup_fastboot_used,
        startup_delta_applied: false,
        startup_replay_required: !startup_fastboot_used,
        startup_fallback_reason,
    }
}

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
        external_mining_templates_emitted: 0,
        external_mining_templates_invalidated: 0,
        external_mining_stale_work_detected: 0,
        external_mining_submit_accepted: 0,
        external_mining_submit_rejected: 0,
        external_mining_rejected_invalid_pow: 0,
        external_mining_rejected_stale_template: 0,
        external_mining_rejected_unknown_template: 0,
        external_mining_rejected_submit_block_error: 0,
        external_mining_rejected_storage_error: 0,
        external_mining_last_template_id: None,
        startup_snapshot_exists: false,
        startup_persisted_block_count: 0,
        startup_persisted_max_height: 0,
        startup_consistency_issue_count: 0,
        startup_recovery_mode: "unknown".to_string(),
        startup_rebuild_reason: None,
        startup_path: "unknown".to_string(),
        startup_fastboot_used: false,
        startup_snapshot_detected: false,
        startup_snapshot_validated: false,
        startup_delta_applied: false,
        startup_replay_required: false,
        startup_fallback_reason: None,
        startup_duration_ms: 0,
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
    use super::derive_startup_path_report;

    #[test]
    fn valid_snapshot_path_reports_fastboot_usage_correctly() {
        let report = derive_startup_path_report("snapshot", true, 10, None);
        assert_eq!(report.startup_path, "fast_boot");
        assert!(report.startup_fastboot_used);
        assert!(report.startup_snapshot_detected);
        assert!(report.startup_snapshot_validated);
        assert!(!report.startup_replay_required);
    }

    #[test]
    fn invalid_snapshot_path_reports_fallback_reason_correctly() {
        let reason = "persisted max height exceeds snapshot height".to_string();
        let report = derive_startup_path_report("replayed_blocks", true, 10, Some(reason.clone()));
        assert_eq!(report.startup_path, "fallback_full_replay");
        assert_eq!(report.startup_fallback_reason, Some(reason));
        assert!(!report.startup_fastboot_used);
    }

    #[test]
    fn full_replay_path_reports_replay_required_status_correctly() {
        let report = derive_startup_path_report("snapshot_missing", false, 42, None);
        assert_eq!(report.startup_path, "full_replay");
        assert!(report.startup_replay_required);
        assert!(!report.startup_fastboot_used);
    }

    #[test]
    fn restart_does_not_leave_stale_or_contradictory_flags() {
        let first = derive_startup_path_report("snapshot", true, 12, None);
        let second = derive_startup_path_report("snapshot_missing", false, 12, None);
        assert!(first.startup_fastboot_used);
        assert!(!second.startup_fastboot_used);
        assert!(second.startup_replay_required);
        assert!(second.startup_fallback_reason.is_none());
    }

    #[test]
    fn runtime_output_stays_coherent_after_fallback() {
        let report = derive_startup_path_report(
            "replayed_blocks",
            true,
            20,
            Some("startup consistency issues detected".to_string()),
        );
        assert_eq!(report.startup_path, "fallback_full_replay");
        assert!(report.startup_replay_required);
        assert!(!report.startup_fastboot_used);
        assert!(!report.startup_snapshot_validated);
    }

    #[test]
    fn startup_classification_regression_guard_for_genesis_init() {
        let report = derive_startup_path_report("genesis_init", false, 0, None);
        assert_eq!(report.startup_path, "genesis_init");
        assert!(!report.startup_fastboot_used);
        assert!(report.startup_fallback_reason.is_none());
        assert!(report.startup_replay_required);
    }
}
