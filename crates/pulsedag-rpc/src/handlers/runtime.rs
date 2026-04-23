use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResponse, RpcStateLike};
use pulsedag_core::{SyncPhase, SyncProgressCounters};
use pulsedag_p2p::{mode_connected_peers_are_real_network, PeerRecoveryStatus};

#[derive(Debug, serde::Serialize)]
pub struct RuntimeStatusData {
    pub started_at_unix: u64,
    pub uptime_secs: u64,
    pub burn_in_target_days: u64,
    pub burn_in_elapsed_days: u64,
    pub burn_in_remaining_days: u64,
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
    pub sync_phase: SyncPhase,
    pub sync_last_transition_unix: Option<u64>,
    pub sync_completed_cycles: u64,
    pub sync_restart_count: u64,
    pub sync_last_error: Option<String>,
    pub sync_selected_peer: Option<String>,
    pub sync_selection_version: u64,
    pub sync_fallback_count: u64,
    pub sync_timeout_fallback_count: u64,
    pub sync_last_fallback_reason: Option<String>,
    pub sync_last_fallback_peer: Option<String>,
    pub sync_counters: SyncProgressCounters,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
    pub p2p_peer_reconnect_attempts: u64,
    pub p2p_peer_recovery_success_count: u64,
    pub p2p_last_peer_recovery_unix: Option<u64>,
    pub p2p_peer_cooldown_suppressed_count: u64,
    pub p2p_peer_flap_suppressed_count: u64,
    pub p2p_peers_under_cooldown: usize,
    pub p2p_peers_under_flap_guard: usize,
    pub p2p_last_peer_seen_unix: Option<u64>,
    pub p2p_peers_with_recent_failures: usize,
    pub p2p_connected_peers_are_real_network: bool,
    pub p2p_peer_health_healthy: usize,
    pub p2p_peer_health_degraded: usize,
    pub p2p_peer_health_recovering: usize,
    pub p2p_tx_outbound_duplicates_suppressed: usize,
    pub p2p_tx_outbound_first_seen_relayed: usize,
    pub p2p_inbound_duplicates_suppressed: usize,
}

#[derive(Default)]
struct RuntimeP2pRecoverySummary {
    reconnect_attempts: u64,
    recovery_success_count: u64,
    last_recovery_unix: Option<u64>,
    cooldown_suppressed_count: u64,
    flap_suppressed_count: u64,
    peers_under_cooldown: usize,
    peers_under_flap_guard: usize,
    last_peer_seen_unix: Option<u64>,
    peers_with_recent_failures: usize,
    connected_peers_are_real_network: bool,
    peer_health_healthy: usize,
    peer_health_degraded: usize,
    peer_health_recovering: usize,
    tx_outbound_duplicates_suppressed: usize,
    tx_outbound_first_seen_relayed: usize,
    inbound_duplicates_suppressed: usize,
}

fn is_peer_recovering(peer: &PeerRecoveryStatus, now_unix: u64) -> bool {
    if !peer.connected || peer.fail_streak > 0 {
        return true;
    }
    if peer
        .suppression_until_unix
        .is_some_and(|until| until > now_unix)
    {
        return true;
    }
    peer.next_retry_unix > now_unix
}

fn is_peer_degraded(peer: &PeerRecoveryStatus) -> bool {
    peer.score < 80 || peer.flap_events > 0 || !peer.recent_failures_unix.is_empty()
}

pub async fn get_runtime_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<RuntimeStatusData>> {
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(runtime.started_at_unix);
    let uptime_secs = now.saturating_sub(runtime.started_at_unix);
    let burn_in_target_days: u64 = 30;
    let burn_in_elapsed_days = uptime_secs / 86_400;
    let burn_in_remaining_days = burn_in_target_days.saturating_sub(burn_in_elapsed_days);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let p2p_recovery = state
        .p2p()
        .and_then(|p2p| p2p.status().ok())
        .map(|status| {
            let p2p_last_peer_seen_unix = status
                .peer_recovery
                .iter()
                .filter_map(|peer| peer.last_seen_unix)
                .max();
            let p2p_peers_with_recent_failures = status
                .peer_recovery
                .iter()
                .filter(|peer| !peer.recent_failures_unix.is_empty())
                .count();
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut peer_health_healthy = 0usize;
            let mut peer_health_degraded = 0usize;
            let mut peer_health_recovering = 0usize;
            for peer in &status.peer_recovery {
                if is_peer_recovering(peer, now_unix) {
                    peer_health_recovering = peer_health_recovering.saturating_add(1);
                } else if is_peer_degraded(peer) {
                    peer_health_degraded = peer_health_degraded.saturating_add(1);
                } else {
                    peer_health_healthy = peer_health_healthy.saturating_add(1);
                }
            }
            RuntimeP2pRecoverySummary {
                reconnect_attempts: status.peer_reconnect_attempts,
                recovery_success_count: status.peer_recovery_success_count,
                last_recovery_unix: status.last_peer_recovery_unix,
                cooldown_suppressed_count: status.peer_cooldown_suppressed_count,
                flap_suppressed_count: status.peer_flap_suppressed_count,
                peers_under_cooldown: status.peers_under_cooldown,
                peers_under_flap_guard: status.peers_under_flap_guard,
                last_peer_seen_unix: p2p_last_peer_seen_unix,
                peers_with_recent_failures: p2p_peers_with_recent_failures,
                connected_peers_are_real_network: mode_connected_peers_are_real_network(
                    &status.mode,
                ),
                peer_health_healthy,
                peer_health_degraded,
                peer_health_recovering,
                tx_outbound_duplicates_suppressed: status.tx_outbound_duplicates_suppressed,
                tx_outbound_first_seen_relayed: status.tx_outbound_first_seen_relayed,
                inbound_duplicates_suppressed: status.inbound_duplicates_suppressed,
            }
        })
        .unwrap_or_default();
    Json(ApiResponse::ok(RuntimeStatusData {
        started_at_unix: runtime.started_at_unix,
        uptime_secs,
        burn_in_target_days,
        burn_in_elapsed_days,
        burn_in_remaining_days,
        accepted_p2p_blocks: runtime.accepted_p2p_blocks,
        rejected_p2p_blocks: runtime.rejected_p2p_blocks,
        duplicate_p2p_blocks: runtime.duplicate_p2p_blocks,
        queued_orphan_blocks: runtime.queued_orphan_blocks,
        adopted_orphan_blocks: runtime.adopted_orphan_blocks,
        accepted_p2p_txs: runtime.accepted_p2p_txs,
        rejected_p2p_txs: runtime.rejected_p2p_txs,
        duplicate_p2p_txs: runtime.duplicate_p2p_txs,
        dropped_p2p_txs: runtime.dropped_p2p_txs,
        dropped_p2p_txs_duplicate_mempool: runtime.dropped_p2p_txs_duplicate_mempool,
        dropped_p2p_txs_duplicate_confirmed: runtime.dropped_p2p_txs_duplicate_confirmed,
        dropped_p2p_txs_accept_failed: runtime.dropped_p2p_txs_accept_failed,
        dropped_p2p_txs_persist_failed: runtime.dropped_p2p_txs_persist_failed,
        tx_rebroadcast_attempts: runtime.tx_rebroadcast_attempts,
        tx_rebroadcast_success: runtime.tx_rebroadcast_success,
        tx_rebroadcast_failed: runtime.tx_rebroadcast_failed,
        tx_rebroadcast_skipped_no_p2p: runtime.tx_rebroadcast_skipped_no_p2p,
        tx_rebroadcast_skipped_no_peers: runtime.tx_rebroadcast_skipped_no_peers,
        last_tx_rebroadcast_unix: runtime.last_tx_rebroadcast_unix,
        last_tx_rebroadcast_error: runtime.last_tx_rebroadcast_error.clone(),
        tx_inbound_total: runtime.tx_inbound_total,
        tx_inbound_accepted_total: runtime.tx_inbound_accepted_total,
        tx_inbound_rejected_total: runtime.tx_inbound_rejected_total,
        tx_inbound_dropped_total: runtime.tx_inbound_dropped_total,
        last_tx_accept_unix: runtime.last_tx_accept_unix,
        last_tx_reject_unix: runtime.last_tx_reject_unix,
        last_tx_drop_unix: runtime.last_tx_drop_unix,
        last_tx_drop_reason: runtime.last_tx_drop_reason.clone(),
        last_tx_drop_txid: runtime.last_tx_drop_txid.clone(),
        tx_drop_reasons: runtime.tx_drop_reasons.clone(),
        accepted_mined_blocks: runtime.accepted_mined_blocks,
        rejected_mined_blocks: runtime.rejected_mined_blocks,
        startup_snapshot_exists: runtime.startup_snapshot_exists,
        startup_persisted_block_count: runtime.startup_persisted_block_count,
        startup_persisted_max_height: runtime.startup_persisted_max_height,
        startup_consistency_issue_count: runtime.startup_consistency_issue_count,
        startup_recovery_mode: runtime.startup_recovery_mode.clone(),
        startup_rebuild_reason: runtime.startup_rebuild_reason.clone(),
        last_self_audit_unix: runtime.last_self_audit_unix,
        last_self_audit_ok: runtime.last_self_audit_ok,
        last_self_audit_issue_count: runtime.last_self_audit_issue_count,
        last_self_audit_message: runtime.last_self_audit_message.clone(),
        last_observed_best_height: runtime.last_observed_best_height,
        last_height_change_unix: runtime.last_height_change_unix,
        active_alerts: runtime.active_alerts.clone(),
        snapshot_auto_every_blocks: runtime.snapshot_auto_every_blocks,
        auto_prune_enabled: runtime.auto_prune_enabled,
        auto_prune_every_blocks: runtime.auto_prune_every_blocks,
        prune_keep_recent_blocks: runtime.prune_keep_recent_blocks,
        prune_require_snapshot: runtime.prune_require_snapshot,
        last_snapshot_height: runtime.last_snapshot_height,
        last_snapshot_unix: runtime.last_snapshot_unix,
        last_prune_height: runtime.last_prune_height,
        last_prune_unix: runtime.last_prune_unix,
        sync_phase: runtime.sync_pipeline.phase,
        sync_last_transition_unix: runtime.sync_pipeline.last_transition_unix,
        sync_completed_cycles: runtime.sync_pipeline.completed_cycles,
        sync_restart_count: runtime.sync_pipeline.restart_count,
        sync_last_error: runtime.sync_pipeline.last_error.clone(),
        sync_selected_peer: runtime.sync_pipeline.selected_peer.clone(),
        sync_selection_version: runtime.sync_pipeline.selection_version,
        sync_fallback_count: runtime.sync_pipeline.fallback_count,
        sync_timeout_fallback_count: runtime.sync_pipeline.timeout_fallback_count,
        sync_last_fallback_reason: runtime.sync_pipeline.last_fallback_reason.clone(),
        sync_last_fallback_peer: runtime.sync_pipeline.last_fallback_peer.clone(),
        sync_counters: runtime.sync_pipeline.counters.clone(),
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
        p2p_peer_reconnect_attempts: p2p_recovery.reconnect_attempts,
        p2p_peer_recovery_success_count: p2p_recovery.recovery_success_count,
        p2p_last_peer_recovery_unix: p2p_recovery.last_recovery_unix,
        p2p_peer_cooldown_suppressed_count: p2p_recovery.cooldown_suppressed_count,
        p2p_peer_flap_suppressed_count: p2p_recovery.flap_suppressed_count,
        p2p_peers_under_cooldown: p2p_recovery.peers_under_cooldown,
        p2p_peers_under_flap_guard: p2p_recovery.peers_under_flap_guard,
        p2p_last_peer_seen_unix: p2p_recovery.last_peer_seen_unix,
        p2p_peers_with_recent_failures: p2p_recovery.peers_with_recent_failures,
        p2p_connected_peers_are_real_network: p2p_recovery.connected_peers_are_real_network,
        p2p_peer_health_healthy: p2p_recovery.peer_health_healthy,
        p2p_peer_health_degraded: p2p_recovery.peer_health_degraded,
        p2p_peer_health_recovering: p2p_recovery.peer_health_recovering,
        p2p_tx_outbound_duplicates_suppressed: p2p_recovery.tx_outbound_duplicates_suppressed,
        p2p_tx_outbound_first_seen_relayed: p2p_recovery.tx_outbound_first_seen_relayed,
        p2p_inbound_duplicates_suppressed: p2p_recovery.inbound_duplicates_suppressed,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{extract::State, Json};
    use pulsedag_core::{ChainState, SyncPhase};
    use pulsedag_p2p::{P2pHandle, P2pStatus, PeerRecoveryStatus, P2P_MODE_LIBP2P_REAL};
    use pulsedag_storage::Storage;
    use tokio::sync::RwLock;

    use crate::api::{NodeRuntimeStats, RpcStateLike};

    use super::get_runtime_status;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
        p2p: Option<Arc<dyn P2pHandle>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }

        fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
            self.p2p.clone()
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
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    #[derive(Clone)]
    struct TestP2pHandle {
        status: P2pStatus,
    }

    impl P2pHandle for TestP2pHandle {
        fn broadcast_transaction(
            &self,
            _tx: &pulsedag_core::types::Transaction,
        ) -> Result<(), pulsedag_core::errors::PulseError> {
            Ok(())
        }
        fn broadcast_block(
            &self,
            _block: &pulsedag_core::types::Block,
        ) -> Result<(), pulsedag_core::errors::PulseError> {
            Ok(())
        }
        fn status(&self) -> Result<P2pStatus, pulsedag_core::errors::PulseError> {
            Ok(self.status.clone())
        }
    }

    #[tokio::test]
    async fn runtime_status_surfaces_sync_phase_coherently() {
        let path = temp_db_path("runtime-sync-phase");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.begin_cycle(100);
        runtime.sync_pipeline.observe_headers(5, 101);
        runtime.sync_pipeline.request_blocks(5, 102);
        runtime.sync_pipeline.validate_and_apply_blocks(2, 103);

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("runtime status payload");
        assert_eq!(data.sync_phase, SyncPhase::ValidationApplication);
        assert_eq!(data.sync_counters.headers_discovered, 5);
        assert_eq!(data.sync_counters.blocks_requested, 5);
        assert_eq!(data.sync_counters.blocks_applied, 2);
    }

    #[tokio::test]
    async fn runtime_status_surfaces_p2p_mode_and_peer_health_summary() {
        let path = temp_db_path("runtime-p2p-summary");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let p2p_status = P2pStatus {
            mode: P2P_MODE_LIBP2P_REAL.to_string(),
            peer_id: "self".into(),
            listening: vec![],
            connected_peers: vec!["peer-a".into()],
            topics: vec![],
            mdns: false,
            kademlia: true,
            broadcasted_messages: 0,
            publish_attempts: 0,
            seen_message_ids: 0,
            queued_messages: 0,
            inbound_messages: 0,
            runtime_started: true,
            runtime_mode_detail: "swarm-poll-loop-real".into(),
            swarm_events_seen: 0,
            subscriptions_active: 0,
            last_message_kind: None,
            last_swarm_event: None,
            per_topic_publishes: std::collections::HashMap::new(),
            inbound_decode_failed: 0,
            inbound_chain_mismatch_dropped: 0,
            inbound_duplicates_suppressed: 0,
            tx_outbound_duplicates_suppressed: 0,
            tx_outbound_first_seen_relayed: 0,
            last_drop_reason: None,
            peer_reconnect_attempts: 5,
            peer_recovery_success_count: 1,
            last_peer_recovery_unix: Some(now.saturating_sub(3)),
            peer_cooldown_suppressed_count: 2,
            peer_flap_suppressed_count: 1,
            peers_under_cooldown: 1,
            peers_under_flap_guard: 1,
            peer_recovery: vec![
                PeerRecoveryStatus {
                    peer_id: "healthy".into(),
                    score: 100,
                    fail_streak: 0,
                    connected: true,
                    last_seen_unix: Some(now),
                    last_successful_connect_unix: Some(now),
                    next_retry_unix: 0,
                    reconnect_attempts: 0,
                    recovery_success_count: 0,
                    last_recovery_unix: Some(now.saturating_sub(3)),
                    recent_failures_unix: vec![],
                    cooldown_suppressed_count: 0,
                    flap_suppressed_count: 0,
                    flap_events: 0,
                    suppression_until_unix: None,
                },
                PeerRecoveryStatus {
                    peer_id: "recovering".into(),
                    score: 60,
                    fail_streak: 1,
                    connected: false,
                    last_seen_unix: Some(now.saturating_sub(100)),
                    last_successful_connect_unix: Some(now.saturating_sub(200)),
                    next_retry_unix: now.saturating_add(50),
                    reconnect_attempts: 4,
                    recovery_success_count: 1,
                    last_recovery_unix: Some(now.saturating_sub(40)),
                    recent_failures_unix: vec![now.saturating_sub(20)],
                    cooldown_suppressed_count: 2,
                    flap_suppressed_count: 1,
                    flap_events: 2,
                    suppression_until_unix: Some(now.saturating_add(10)),
                },
            ],
            sync_candidates: vec![],
            selected_sync_peer: Some("peer-a".into()),
        };
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: Some(Arc::new(TestP2pHandle { status: p2p_status })),
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert!(data.p2p_connected_peers_are_real_network);
        assert_eq!(data.p2p_peer_health_healthy, 1);
        assert_eq!(data.p2p_peer_health_degraded, 0);
        assert_eq!(data.p2p_peer_health_recovering, 1);
    }
}

#[derive(Debug, Deserialize)]
pub struct RuntimeEventsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeEventsData {
    pub count: usize,
    pub events: Vec<pulsedag_storage::RuntimeEvent>,
}

pub async fn get_runtime_events<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventsQuery>,
) -> Json<ApiResponse<RuntimeEventsData>> {
    let limit = query.limit.unwrap_or(20).min(200);
    match state.storage().list_runtime_events(limit) {
        Ok(events) => Json(ApiResponse::ok(RuntimeEventsData {
            count: events.len(),
            events,
        })),
        Err(e) => Json(ApiResponse::err("RUNTIME_EVENTS_ERROR", &e.to_string())),
    }
}

#[derive(Debug, Serialize)]
pub struct RuntimeEventsSummaryData {
    pub scanned_event_count: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub by_level: BTreeMap<String, usize>,
}

pub async fn get_runtime_events_summary<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventsQuery>,
) -> Json<ApiResponse<RuntimeEventsSummaryData>> {
    let limit = query.limit.unwrap_or(200).min(2000);
    match state.storage().list_runtime_events(limit) {
        Ok(events) => {
            let mut by_kind = BTreeMap::new();
            let mut by_level = BTreeMap::new();
            for event in &events {
                *by_kind.entry(event.kind.clone()).or_insert(0) += 1;
                *by_level.entry(event.level.clone()).or_insert(0) += 1;
            }
            Json(ApiResponse::ok(RuntimeEventsSummaryData {
                scanned_event_count: events.len(),
                by_kind,
                by_level,
            }))
        }
        Err(e) => Json(ApiResponse::err(
            "RUNTIME_EVENTS_SUMMARY_ERROR",
            &e.to_string(),
        )),
    }
}
