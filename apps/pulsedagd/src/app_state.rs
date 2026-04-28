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
    pub startup_bootstrap_mode: String,
    pub startup_status_summary: String,
    pub startup_fastboot_used: bool,
    pub startup_snapshot_detected: bool,
    pub startup_snapshot_validated: bool,
    pub startup_delta_applied: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupLifecycleEvent {
    pub level: &'static str,
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct OperatorConsoleInputs {
    pub best_height: u64,
    pub best_tip_hash: String,
    pub startup_path: String,
    pub startup_summary: String,
    pub sync_phase: String,
    pub sync_health: String,
    pub connected_peers: usize,
    pub connected_peers_semantics: String,
    pub mempool_size: usize,
    pub orphan_count: usize,
    pub active_alerts: Vec<String>,
    pub last_height_change_unix: Option<u64>,
    pub now_unix: u64,
    pub accepted_p2p_blocks: u64,
    pub accepted_mined_blocks: u64,
    pub snapshot_status: String,
    pub prune_status: String,
}

#[derive(Debug, Clone)]
pub struct OperatorConsoleRollup {
    pub tip_hash_short: String,
    pub stagnation_secs: u64,
    pub height_changed: bool,
    pub accepted_p2p_blocks_delta: u64,
    pub accepted_mined_blocks_delta: u64,
    pub line: String,
}

pub fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

pub fn build_operator_console_rollup(
    inputs: &OperatorConsoleInputs,
    previous_best_height: u64,
    previous_accepted_p2p_blocks: u64,
    previous_accepted_mined_blocks: u64,
) -> OperatorConsoleRollup {
    let tip_hash_short = short_hash(&inputs.best_tip_hash);
    let stagnation_secs = inputs
        .last_height_change_unix
        .map(|ts| inputs.now_unix.saturating_sub(ts))
        .unwrap_or(0);
    let accepted_p2p_blocks_delta = inputs
        .accepted_p2p_blocks
        .saturating_sub(previous_accepted_p2p_blocks);
    let accepted_mined_blocks_delta = inputs
        .accepted_mined_blocks
        .saturating_sub(previous_accepted_mined_blocks);
    let alerts = if inputs.active_alerts.is_empty() {
        "none".to_string()
    } else {
        inputs.active_alerts.join(" | ")
    };
    let line = format!(
        "operator_rollup height={} tip={} startup_path={} startup_summary=\"{}\" sync_phase={} sync_health={} peers={} ({}) mempool={} orphans={} alerts=\"{}\" stagnation_secs={} accepted_inbound_blocks_delta={} accepted_mined_blocks_delta={} snapshot_status=\"{}\" prune_status=\"{}\"",
        inputs.best_height,
        tip_hash_short,
        inputs.startup_path,
        inputs.startup_summary,
        inputs.sync_phase,
        inputs.sync_health,
        inputs.connected_peers,
        inputs.connected_peers_semantics,
        inputs.mempool_size,
        inputs.orphan_count,
        alerts,
        stagnation_secs,
        accepted_p2p_blocks_delta,
        accepted_mined_blocks_delta,
        inputs.snapshot_status,
        inputs.prune_status
    );
    OperatorConsoleRollup {
        tip_hash_short,
        stagnation_secs,
        height_changed: inputs.best_height > previous_best_height,
        accepted_p2p_blocks_delta,
        accepted_mined_blocks_delta,
        line,
    }
}

pub fn build_startup_lifecycle_events(
    _startup_recovery_mode: &str,
    startup_report: &StartupPathReport,
    startup_duration_ms: u128,
) -> Vec<StartupLifecycleEvent> {
    let mut events = Vec::new();
    if startup_report.startup_snapshot_detected {
        events.push(StartupLifecycleEvent {
            level: "info",
            kind: "snapshot_validation_started",
            message: "validating startup snapshot state".to_string(),
        });
        if startup_report.startup_snapshot_validated {
            events.push(StartupLifecycleEvent {
                level: "info",
                kind: "snapshot_validation_succeeded",
                message: "startup snapshot validation succeeded".to_string(),
            });
            events.push(StartupLifecycleEvent {
                level: "info",
                kind: "delta_apply_started",
                message: "applying persisted delta on top of validated snapshot".to_string(),
            });
            events.push(StartupLifecycleEvent {
                level: "info",
                kind: "delta_apply_succeeded",
                message: "persisted delta apply completed".to_string(),
            });
        } else {
            let reason = startup_report
                .startup_fallback_reason
                .clone()
                .unwrap_or_else(|| "startup snapshot validation failed".to_string());
            events.push(StartupLifecycleEvent {
                level: "warn",
                kind: "snapshot_validation_failed",
                message: reason.clone(),
            });
            events.push(StartupLifecycleEvent {
                level: "warn",
                kind: "delta_apply_failed",
                message: format!(
                    "delta apply skipped because snapshot validation failed: {reason}"
                ),
            });
        }
    }

    let replay_path = matches!(
        startup_report.startup_path.as_str(),
        "full_replay" | "fallback_full_replay"
    );
    if replay_path {
        events.push(StartupLifecycleEvent {
            level: "warn",
            kind: "full_replay_started",
            message: "starting full replay from persisted blocks".to_string(),
        });
        events.push(StartupLifecycleEvent {
            level: "info",
            kind: "full_replay_completed",
            message: "full replay from persisted blocks completed".to_string(),
        });
    }

    events.push(StartupLifecycleEvent {
        level: "info",
        kind: "startup_completed",
        message: format!(
            "startup completed (path={}, duration_ms={})",
            startup_report.startup_path, startup_duration_ms
        ),
    });
    events
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupRecoveryMode {
    Snapshot,
    SnapshotMissing,
    ReplayedBlocks,
    GenesisInit,
    Unknown,
}

impl StartupRecoveryMode {
    fn parse(value: &str) -> Self {
        match value {
            "snapshot" => Self::Snapshot,
            "snapshot_missing" => Self::SnapshotMissing,
            "replayed_blocks" => Self::ReplayedBlocks,
            "genesis_init" => Self::GenesisInit,
            _ => Self::Unknown,
        }
    }
}

pub fn derive_startup_path_report(
    startup_recovery_mode: &str,
    snapshot_exists: bool,
    persisted_block_count: usize,
    startup_rebuild_reason: Option<String>,
) -> StartupPathReport {
    let mode = StartupRecoveryMode::parse(startup_recovery_mode);
    let mut startup_fallback_reason = None;
    let startup_path = match mode {
        StartupRecoveryMode::ReplayedBlocks => {
            startup_fallback_reason = Some(startup_rebuild_reason.unwrap_or_else(|| {
                "startup recovery requested full replay without explicit fallback reason"
                    .to_string()
            }));
            "fallback_full_replay"
        }
        StartupRecoveryMode::Snapshot if snapshot_exists => "fast_boot",
        StartupRecoveryMode::Snapshot | StartupRecoveryMode::SnapshotMissing => {
            if persisted_block_count > 0 {
                "full_replay"
            } else {
                "genesis_init"
            }
        }
        StartupRecoveryMode::GenesisInit => {
            if persisted_block_count > 0 {
                "full_replay"
            } else {
                "genesis_init"
            }
        }
        StartupRecoveryMode::Unknown => {
            if snapshot_exists {
                "fast_boot"
            } else if persisted_block_count > 0 {
                "full_replay"
            } else {
                "genesis_init"
            }
        }
    }
    .to_string();
    let startup_fastboot_used = startup_path == "fast_boot";
    let startup_delta_applied = startup_fastboot_used;
    let startup_bootstrap_mode = if startup_path == "fallback_full_replay" {
        "recovery_fallback"
    } else if startup_fastboot_used {
        "snapshot_assisted"
    } else if startup_path == "full_replay" {
        "replay"
    } else {
        "normal"
    }
    .to_string();
    let startup_status_summary = if let Some(reason) = startup_fallback_reason.as_ref() {
        format!(
            "{} startup via {}; fallback_reason={}",
            startup_bootstrap_mode, startup_path, reason
        )
    } else if mode == StartupRecoveryMode::Snapshot && !snapshot_exists {
        format!(
            "{} startup via {}; recovered from contradictory input mode=snapshot while snapshot_exists=false",
            startup_bootstrap_mode, startup_path
        )
    } else {
        format!("{startup_bootstrap_mode} startup via {startup_path}")
    };

    StartupPathReport {
        startup_path,
        startup_bootstrap_mode,
        startup_status_summary,
        startup_fastboot_used,
        startup_snapshot_detected: snapshot_exists,
        startup_snapshot_validated: startup_fastboot_used,
        startup_delta_applied,
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
        external_mining_last_rejection_kind: None,
        external_mining_last_rejection_reason: None,
        external_mining_last_invalid_pow_reason: None,
        startup_snapshot_exists: false,
        startup_persisted_block_count: 0,
        startup_persisted_max_height: 0,
        startup_consistency_issue_count: 0,
        startup_recovery_mode: "unknown".to_string(),
        startup_rebuild_reason: None,
        startup_path: "unknown".to_string(),
        startup_bootstrap_mode: "unknown".to_string(),
        startup_status_summary: "startup status unknown".to_string(),
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
    use super::{
        build_operator_console_rollup, build_startup_lifecycle_events, derive_startup_path_report,
        short_hash, OperatorConsoleInputs,
    };

    #[test]
    fn valid_snapshot_path_reports_fastboot_usage_correctly() {
        let report = derive_startup_path_report("snapshot", true, 10, None);
        assert_eq!(report.startup_path, "fast_boot");
        assert_eq!(report.startup_bootstrap_mode, "snapshot_assisted");
        assert!(report.startup_fastboot_used);
        assert!(report.startup_snapshot_detected);
        assert!(report.startup_snapshot_validated);
        assert!(report.startup_delta_applied);
        assert!(!report.startup_replay_required);
    }

    #[test]
    fn invalid_snapshot_path_reports_fallback_reason_correctly() {
        let reason = "persisted max height exceeds snapshot height".to_string();
        let report = derive_startup_path_report("replayed_blocks", true, 10, Some(reason.clone()));
        assert_eq!(report.startup_path, "fallback_full_replay");
        assert_eq!(report.startup_bootstrap_mode, "recovery_fallback");
        assert_eq!(report.startup_fallback_reason, Some(reason));
        assert!(!report.startup_fastboot_used);
    }

    #[test]
    fn full_replay_path_reports_replay_required_status_correctly() {
        let report = derive_startup_path_report("snapshot_missing", false, 42, None);
        assert_eq!(report.startup_path, "full_replay");
        assert_eq!(report.startup_bootstrap_mode, "replay");
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
        assert!(!report.startup_delta_applied);
    }

    #[test]
    fn startup_classification_regression_guard_for_genesis_init() {
        let report = derive_startup_path_report("genesis_init", false, 0, None);
        assert_eq!(report.startup_path, "genesis_init");
        assert_eq!(report.startup_bootstrap_mode, "normal");
        assert!(!report.startup_fastboot_used);
        assert!(report.startup_fallback_reason.is_none());
        assert!(report.startup_replay_required);
    }

    #[test]
    fn startup_path_emits_lifecycle_events_in_operator_friendly_order() {
        let report = derive_startup_path_report("snapshot", true, 10, None);
        let events = build_startup_lifecycle_events("snapshot", &report, 12);
        let kinds: Vec<&str> = events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "snapshot_validation_started",
                "snapshot_validation_succeeded",
                "delta_apply_started",
                "delta_apply_succeeded",
                "startup_completed"
            ]
        );
    }

    #[test]
    fn fallback_sequence_reports_snapshot_failure_then_replay_honestly() {
        let report = derive_startup_path_report(
            "replayed_blocks",
            true,
            10,
            Some("persisted max height exceeds snapshot height".to_string()),
        );
        let events = build_startup_lifecycle_events("replayed_blocks", &report, 33);
        let kinds: Vec<&str> = events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "snapshot_validation_started",
                "snapshot_validation_failed",
                "delta_apply_failed",
                "full_replay_started",
                "full_replay_completed",
                "startup_completed"
            ]
        );
    }

    #[test]
    fn startup_success_always_emits_single_completion_event() {
        let report = derive_startup_path_report("snapshot", true, 3, None);
        let events = build_startup_lifecycle_events("snapshot", &report, 7);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == "startup_completed")
                .count(),
            1
        );
    }

    #[test]
    fn lifecycle_events_avoid_obvious_duplicate_kinds() {
        let report = derive_startup_path_report(
            "replayed_blocks",
            true,
            9,
            Some("startup consistency issues detected".to_string()),
        );
        let events = build_startup_lifecycle_events("replayed_blocks", &report, 14);
        let mut unique = std::collections::HashSet::new();
        for event in events {
            assert!(
                unique.insert(event.kind),
                "duplicate event kind: {}",
                event.kind
            );
        }
    }

    #[test]
    fn replay_path_is_reported_without_fallback_noise() {
        let report = derive_startup_path_report("snapshot_missing", false, 25, None);
        assert_eq!(report.startup_bootstrap_mode, "replay");
        assert_eq!(report.startup_path, "full_replay");
        assert!(report.startup_replay_required);
        assert!(report.startup_fallback_reason.is_none());
    }

    #[test]
    fn replay_path_reported_coherently_for_operator_status() {
        let report = derive_startup_path_report("snapshot_missing", false, 25, None);
        let events = build_startup_lifecycle_events("snapshot_missing", &report, 11);
        let kinds: Vec<&str> = events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            vec![
                "full_replay_started",
                "full_replay_completed",
                "startup_completed"
            ]
        );
        assert_eq!(report.startup_bootstrap_mode, "replay");
        assert_eq!(report.startup_path, "full_replay");
        assert!(report.startup_fallback_reason.is_none());
    }

    #[test]
    fn fallback_path_is_explicitly_marked_as_recovery_fallback() {
        let reason = "snapshot decode failed; rebuilding from persisted blocks".to_string();
        let report = derive_startup_path_report("replayed_blocks", true, 25, Some(reason.clone()));
        assert_eq!(report.startup_bootstrap_mode, "recovery_fallback");
        assert_eq!(report.startup_path, "fallback_full_replay");
        assert_eq!(report.startup_fallback_reason, Some(reason));
    }

    #[test]
    fn recovery_path_reported_coherently_as_snapshot_assisted() {
        let report = derive_startup_path_report("snapshot", true, 42, None);
        assert_eq!(report.startup_bootstrap_mode, "snapshot_assisted");
        assert_eq!(report.startup_path, "fast_boot");
        assert!(report.startup_fastboot_used);
        assert!(!report.startup_replay_required);
    }

    #[test]
    fn fallback_path_reported_coherently_with_reason() {
        let report = derive_startup_path_report("replayed_blocks", true, 42, None);
        assert_eq!(report.startup_bootstrap_mode, "recovery_fallback");
        assert_eq!(report.startup_path, "fallback_full_replay");
        assert!(report.startup_fallback_reason.is_some());
        assert!(report.startup_status_summary.contains("fallback_reason="));
    }

    #[test]
    fn startup_summary_distinguishes_snapshot_assisted_from_normal_boot() {
        let snapshot_assisted = derive_startup_path_report("snapshot", true, 25, None);
        let normal = derive_startup_path_report("genesis_init", false, 0, None);
        assert!(snapshot_assisted
            .startup_status_summary
            .contains("snapshot_assisted"));
        assert!(normal.startup_status_summary.contains("normal"));
    }

    #[test]
    fn startup_flags_do_not_contradict_each_other() {
        let scenarios = vec![
            derive_startup_path_report("snapshot", true, 8, None),
            derive_startup_path_report("snapshot_missing", false, 8, None),
            derive_startup_path_report(
                "replayed_blocks",
                true,
                8,
                Some("snapshot validation failed".to_string()),
            ),
            derive_startup_path_report("genesis_init", false, 0, None),
        ];
        for report in scenarios {
            assert_eq!(
                report.startup_fastboot_used,
                report.startup_path == "fast_boot"
            );
            assert_eq!(
                report.startup_snapshot_validated,
                report.startup_fastboot_used
            );
            assert_eq!(report.startup_delta_applied, report.startup_fastboot_used);
            assert_eq!(
                report.startup_replay_required,
                !report.startup_fastboot_used
            );
            if report.startup_bootstrap_mode == "recovery_fallback" {
                assert!(report.startup_fallback_reason.is_some());
            }
        }
    }

    #[test]
    fn contradictory_startup_state_is_prevented() {
        let report = derive_startup_path_report("snapshot", false, 18, None);
        assert_eq!(report.startup_path, "full_replay");
        assert_eq!(report.startup_bootstrap_mode, "replay");
        assert!(!report.startup_fastboot_used);
        assert!(!report.startup_snapshot_validated);
        assert!(report.startup_replay_required);
        assert!(report
            .startup_status_summary
            .contains("contradictory input mode=snapshot"));
    }

    #[test]
    fn operator_rollup_surfaces_height_change_and_block_acceptance_deltas() {
        let input = OperatorConsoleInputs {
            best_height: 44,
            best_tip_hash: "abcdef1234567890".to_string(),
            startup_path: "fast_boot".to_string(),
            startup_summary: "snapshot_assisted startup via fast_boot".to_string(),
            sync_phase: "Idle".to_string(),
            sync_health: "idle".to_string(),
            connected_peers: 3,
            connected_peers_semantics: "real network peers".to_string(),
            mempool_size: 7,
            orphan_count: 1,
            active_alerts: vec![],
            last_height_change_unix: Some(120),
            now_unix: 150,
            accepted_p2p_blocks: 12,
            accepted_mined_blocks: 4,
            snapshot_status: "last_snapshot_height=44".to_string(),
            prune_status: "auto_prune=on".to_string(),
        };
        let rollup = build_operator_console_rollup(&input, 43, 10, 3);
        assert!(rollup.height_changed);
        assert_eq!(rollup.accepted_p2p_blocks_delta, 2);
        assert_eq!(rollup.accepted_mined_blocks_delta, 1);
        assert_eq!(rollup.tip_hash_short, "abcdef123456");
        assert!(rollup.line.contains("height=44"));
    }

    #[test]
    fn operator_rollup_reports_stagnation_and_alerts_consistently() {
        let input = OperatorConsoleInputs {
            best_height: 88,
            best_tip_hash: "0011223344556677".to_string(),
            startup_path: "full_replay".to_string(),
            startup_summary: "replay startup via full_replay".to_string(),
            sync_phase: "BlockDownload".to_string(),
            sync_health: "active".to_string(),
            connected_peers: 0,
            connected_peers_semantics: "simulated peers (not external network connectivity)"
                .to_string(),
            mempool_size: 600,
            orphan_count: 40,
            active_alerts: vec![
                "high mempool size: 600".to_string(),
                "high orphan count: 40".to_string(),
            ],
            last_height_change_unix: Some(1_000),
            now_unix: 1_900,
            accepted_p2p_blocks: 3,
            accepted_mined_blocks: 0,
            snapshot_status: "last_snapshot_height=none".to_string(),
            prune_status: "auto_prune=off".to_string(),
        };
        let rollup = build_operator_console_rollup(&input, 88, 3, 0);
        assert!(!rollup.height_changed);
        assert_eq!(rollup.stagnation_secs, 900);
        assert!(rollup
            .line
            .contains("alerts=\"high mempool size: 600 | high orphan count: 40\""));
        assert!(rollup.line.contains("stagnation_secs=900"));
    }

    #[test]
    fn short_hash_truncates_without_noise() {
        assert_eq!(short_hash("1234567890abcdef"), "1234567890ab");
    }
}
