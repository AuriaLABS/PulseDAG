use crate::{
    api::ApiResponse,
    api::RpcStateLike,
    handlers::release::{operator_stage, repo_version},
    handlers::runtime::{
        build_runtime_trend_windows, runtime_incident_snapshot, runtime_surface_rollup,
        RuntimeIncidentSnapshot, RuntimeSurfaceRollup, RuntimeTrendWindow,
    },
};
use axum::{extract::State, Json};
use pulsedag_storage::StorageAuditReport;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, serde::Serialize)]
pub struct DiagnosticsData {
    pub version: String,
    pub stage: String,
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub snapshot_exists: bool,
    pub p2p_enabled: bool,
    pub peer_count: usize,
    pub storage_audit_ok: bool,
    pub storage_audit_issue_count: usize,
    pub storage_audit_summary: StorageAuditReport,
    pub startup_path: String,
    pub startup_fastboot_used: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
    pub runtime_surface_rollup: RuntimeSurfaceRollup,
    pub incident_primary_surface: String,
    pub incident_summary: String,
    pub incident_indicators: Vec<String>,
    pub incident_snapshot: RuntimeIncidentSnapshot,
    pub trend_windows: Vec<RuntimeTrendWindow>,
}

#[derive(Debug, serde::Serialize)]
pub struct OperatorQueryPackData {
    pub schema_version: String,
    pub generated_at_unix: u64,
    pub incident_view: RuntimeIncidentSnapshot,
    pub runtime_rollup: RuntimeSurfaceRollup,
    pub sync_recovery_view: OperatorSyncRecoveryView,
    pub relay_health_view: OperatorRelayHealthView,
    pub mining_audit_view: OperatorMiningAuditView,
    pub startup_recovery_view: OperatorStartupRecoveryView,
    pub deterministic_notes: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct OperatorSyncRecoveryView {
    pub sync_surface_health: String,
    pub sync_counters_coherent: bool,
    pub startup_replay_required: bool,
    pub recovery_confidence: String,
    pub recovery_confidence_reason: String,
}

#[derive(Debug, serde::Serialize)]
pub struct OperatorRelayHealthView {
    pub tx_propagation_health: String,
    pub tx_inbound_counters_coherent: bool,
    pub tx_drop_reason_counters_coherent: bool,
    pub tx_rebroadcast_outcomes_coherent: bool,
    pub p2p_enabled: bool,
    pub peer_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct OperatorMiningAuditView {
    pub external_mining_surface_health: String,
    pub external_mining_template_health: String,
    pub external_mining_submit_outcome_counters_coherent: bool,
    pub external_mining_rejection_counters_coherent: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct OperatorStartupRecoveryView {
    pub startup_status_summary: String,
    pub startup_path: String,
    pub startup_fastboot_used: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
}

pub async fn get_diagnostics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<DiagnosticsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let storage_audit = state
        .storage()
        .audit_state_integrity(Some(&chain.chain_id), false)
        .unwrap_or(StorageAuditReport {
            ok: false,
            read_only: true,
            deep_check_performed: false,
            snapshot_exists: false,
            snapshot_anchor_present: false,
            snapshot_best_height: None,
            persisted_block_count: 0,
            persisted_best_height: None,
            lineage_coherent: false,
            deep_replay_viable: None,
            restore_drill_confirms_recovery: false,
            recovery_confidence_non_misleading: false,
            confidence_evidence_path: "fallback::audit_unavailable".into(),
            recovery_confidence: "low".into(),
            confidence_reason: "storage audit fallback path used".into(),
            issue_count: 1,
            issues: vec![pulsedag_storage::StorageAuditIssue {
                code: "AUDIT_UNAVAILABLE".into(),
                message: "storage audit could not be completed".into(),
            }],
        });
    let snapshot_exists = state.storage().snapshot_exists().unwrap_or(false);
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let rollup = runtime_surface_rollup(&runtime);
    let (p2p_enabled, peer_count) = match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => (true, status.connected_peers.len()),
            Err(_) => (true, 0),
        },
        None => (false, 0),
    };

    let incident_primary_surface = rollup.incident_primary_surface.clone();
    let incident_summary = rollup.incident_summary.clone();
    let incident_indicators = rollup.incident_indicators.clone();
    let trend_events = state
        .storage()
        .list_runtime_events(2_000)
        .unwrap_or_default();
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let trend_windows = build_runtime_trend_windows(&trend_events, &rollup, now_unix);
    let warn_or_error_count = trend_events
        .iter()
        .filter(|event| matches!(event.level.as_str(), "warn" | "error"))
        .count();
    let incident_snapshot =
        runtime_incident_snapshot(&rollup, warn_or_error_count, trend_events.len());
    Json(ApiResponse::ok(DiagnosticsData {
        version: repo_version(),
        stage: operator_stage(),
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        snapshot_exists,
        p2p_enabled,
        peer_count,
        storage_audit_ok: storage_audit.ok,
        storage_audit_issue_count: storage_audit.issue_count,
        storage_audit_summary: storage_audit,
        startup_path: runtime.startup_path.clone(),
        startup_fastboot_used: runtime.startup_fastboot_used,
        startup_replay_required: runtime.startup_replay_required,
        startup_fallback_reason: runtime.startup_fallback_reason.clone(),
        runtime_surface_rollup: rollup,
        incident_primary_surface,
        incident_summary,
        incident_indicators,
        incident_snapshot,
        trend_windows,
    }))
}

pub async fn get_operator_query_pack<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<OperatorQueryPackData>> {
    let Json(diag_response) = get_diagnostics(State(state)).await;
    let data = diag_response
        .data
        .expect("diagnostics handler always returns data");
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Json(ApiResponse::ok(OperatorQueryPackData {
        schema_version: "operator-query-pack-v1".to_string(),
        generated_at_unix: now_unix,
        incident_view: data.incident_snapshot.clone(),
        runtime_rollup: data.runtime_surface_rollup.clone(),
        sync_recovery_view: OperatorSyncRecoveryView {
            sync_surface_health: data.runtime_surface_rollup.sync_surface_health.clone(),
            sync_counters_coherent: data.runtime_surface_rollup.sync_counters_coherent,
            startup_replay_required: data.startup_replay_required,
            recovery_confidence: data.runtime_surface_rollup.recovery_confidence.clone(),
            recovery_confidence_reason: data.runtime_surface_rollup.recovery_confidence_reason,
        },
        relay_health_view: OperatorRelayHealthView {
            tx_propagation_health: data.runtime_surface_rollup.tx_propagation_health.clone(),
            tx_inbound_counters_coherent: data.runtime_surface_rollup.tx_inbound_counters_coherent,
            tx_drop_reason_counters_coherent: data
                .runtime_surface_rollup
                .tx_drop_reason_counters_coherent,
            tx_rebroadcast_outcomes_coherent: data
                .runtime_surface_rollup
                .tx_rebroadcast_outcomes_coherent,
            p2p_enabled: data.p2p_enabled,
            peer_count: data.peer_count,
        },
        mining_audit_view: OperatorMiningAuditView {
            external_mining_surface_health: data
                .runtime_surface_rollup
                .external_mining_surface_health
                .clone(),
            external_mining_template_health: data
                .runtime_surface_rollup
                .external_mining_template_health
                .clone(),
            external_mining_submit_outcome_counters_coherent: data
                .runtime_surface_rollup
                .external_mining_submit_outcome_counters_coherent,
            external_mining_rejection_counters_coherent: data
                .runtime_surface_rollup
                .external_mining_rejection_counters_coherent,
        },
        startup_recovery_view: OperatorStartupRecoveryView {
            startup_status_summary: data.runtime_surface_rollup.startup_status_summary,
            startup_path: data.startup_path,
            startup_fastboot_used: data.startup_fastboot_used,
            startup_replay_required: data.startup_replay_required,
            startup_fallback_reason: data.startup_fallback_reason,
        },
        deterministic_notes: vec![
            "derived_from_runtime_surface_rollup".to_string(),
            "no_consensus_mutation".to_string(),
            "operator_read_only_surface".to_string(),
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::{get_diagnostics, get_operator_query_pack};
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use crate::handlers::runtime::{
        get_runtime_events_summary, get_runtime_status, RuntimeEventsQuery,
    };
    use axum::{
        extract::{Query, State},
        Json,
    };
    use pulsedag_core::genesis::init_chain_state;
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<pulsedag_core::ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<pulsedag_core::ChainState>> {
            self.chain.clone()
        }
        fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
            None
        }
        fn storage(&self) -> Arc<Storage> {
            self.storage.clone()
        }
        fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
            self.runtime.clone()
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-diagnostics-{name}-{unique}"))
    }

    #[tokio::test]
    async fn diagnostics_reports_storage_audit_pass_fail_status() {
        let path = temp_db_path("audit-status");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        let chain = init_chain_state("testnet".to_string());
        storage
            .persist_chain_state(&chain)
            .expect("persist healthy snapshot");
        let genesis = chain
            .dag
            .blocks
            .values()
            .find(|block| block.header.height == chain.dag.best_height)
            .expect("best-height block")
            .clone();
        storage.persist_block(&genesis).expect("persist genesis");
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };

        let response = get_diagnostics(State(state)).await;
        let data = response.0.data.expect("data");
        assert!(data.storage_audit_ok);
        assert_eq!(data.storage_audit_issue_count, 0);
    }

    #[tokio::test]
    async fn diagnostics_and_event_summary_rollups_match_runtime_status() {
        let path = temp_db_path("cross-surface-rollup");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        storage
            .append_runtime_event("warn", "sync_phase_change", "sync stalled")
            .expect("append event");
        let chain = init_chain_state("testnet".to_string());
        storage
            .persist_chain_state(&chain)
            .expect("persist healthy snapshot");
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.counters.blocks_requested = 2;
        runtime.sync_pipeline.counters.blocks_acquired = 1;
        runtime.sync_pipeline.counters.blocks_validated = 2;
        runtime.sync_pipeline.counters.blocks_applied = 2;
        runtime.sync_pipeline.last_error = Some("validation mismatch".to_string());
        runtime.external_mining_submit_accepted = 1;
        runtime.external_mining_submit_rejected = 1;
        runtime.external_mining_rejected_invalid_pow = 1;
        runtime.tx_rebroadcast_attempts = 1;
        runtime.tx_rebroadcast_success = 0;
        runtime.active_alerts = vec!["sync stalled".to_string()];
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(runtime_resp) = get_runtime_status(State(state.clone())).await;
        let runtime_data = runtime_resp.data.expect("runtime data");
        let Json(diagnostics_resp) = get_diagnostics(State(state.clone())).await;
        let diagnostics_data = diagnostics_resp.data.expect("diagnostics data");
        let Json(summary_resp) =
            get_runtime_events_summary(State(state), Query(RuntimeEventsQuery { limit: Some(20) }))
                .await;
        let summary_data = summary_resp.data.expect("summary data");

        assert_eq!(
            diagnostics_data
                .runtime_surface_rollup
                .node_runtime_surface_health,
            runtime_data.node_runtime_surface_health
        );
        assert_eq!(
            summary_data.runtime_surface_rollup.sync_surface_health,
            runtime_data.sync_surface_health
        );
        assert_eq!(
            diagnostics_data
                .runtime_surface_rollup
                .tx_propagation_health,
            runtime_data.tx_propagation_health
        );
        assert_eq!(
            summary_data
                .runtime_surface_rollup
                .external_mining_surface_health,
            runtime_data.external_mining_surface_health
        );
        assert_eq!(
            diagnostics_data
                .runtime_surface_rollup
                .startup_status_summary,
            runtime_data.startup_status_summary
        );
        assert_eq!(
            diagnostics_data.incident_primary_surface,
            runtime_data.incident_primary_surface
        );
        assert_eq!(
            diagnostics_data.incident_summary,
            runtime_data.incident_summary
        );
        assert_eq!(
            summary_data.runtime_surface_rollup.incident_summary,
            diagnostics_data.incident_summary
        );
        assert_eq!(
            summary_data.runtime_surface_rollup.runtime_alert_classes,
            runtime_data.runtime_alert_classes
        );
        assert_eq!(
            diagnostics_data.incident_snapshot.primary_surface,
            runtime_data.incident_primary_surface
        );
        assert_eq!(
            diagnostics_data.incident_snapshot.runtime_health_slo_bps,
            runtime_data.runtime_health_slo_bps
        );
        assert_eq!(diagnostics_data.trend_windows.len(), 3);
        assert!(diagnostics_data.trend_windows.iter().all(|window| window
            .incident_snapshot
            .indicators
            .len()
            <= 5));
        assert!(diagnostics_data.trend_windows.iter().all(|window| window
            .incident_snapshot
            .summary
            .len()
            < 220));
    }

    #[tokio::test]
    async fn operator_query_pack_remains_coherent_with_runtime_and_diagnostics() {
        let path = temp_db_path("operator-query-pack-coherence");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        storage
            .append_runtime_event("warn", "sync_phase_change", "sync stalled")
            .expect("append event");
        let chain = init_chain_state("testnet".to_string());
        storage
            .persist_chain_state(&chain)
            .expect("persist healthy snapshot");
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.last_error = Some("peer timeout".to_string());
        runtime.tx_rebroadcast_attempts = 1;
        runtime.tx_rebroadcast_success = 0;
        runtime.external_mining_submit_rejected = 1;
        runtime.external_mining_rejected_invalid_pow = 1;
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(diag_resp) = get_diagnostics(State(state.clone())).await;
        let diag = diag_resp.data.expect("diag");
        let Json(runtime_resp) = get_runtime_status(State(state.clone())).await;
        let runtime_data = runtime_resp.data.expect("runtime");
        let Json(pack_resp) = get_operator_query_pack(State(state)).await;
        let pack = pack_resp.data.expect("pack");

        assert_eq!(pack.runtime_rollup.incident_summary, diag.incident_summary);
        assert_eq!(
            pack.runtime_rollup.node_runtime_surface_health,
            runtime_data.node_runtime_surface_health
        );
        assert_eq!(
            pack.sync_recovery_view.sync_surface_health,
            runtime_data.sync_surface_health
        );
        assert_eq!(
            pack.relay_health_view.tx_propagation_health,
            runtime_data.tx_propagation_health
        );
    }

    #[tokio::test]
    async fn operator_query_pack_audit_views_are_explicit_and_deterministic() {
        let path = temp_db_path("operator-query-pack-explicit");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        let chain = init_chain_state("testnet".to_string());
        storage
            .persist_chain_state(&chain)
            .expect("persist healthy snapshot");
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };
        let Json(pack_resp) = get_operator_query_pack(State(state)).await;
        let pack = pack_resp.data.expect("pack");
        assert_eq!(pack.schema_version, "operator-query-pack-v1");
        assert!(pack
            .deterministic_notes
            .contains(&"operator_read_only_surface".to_string()));
        assert_eq!(pack.deterministic_notes.len(), 3);
    }
}
