use axum::{extract::State, Json};
use pulsedag_core::genesis::init_chain_state;
use std::collections::BTreeMap;

use crate::{
    api::RpcStateLike,
    api::{
        node_rpc_snapshot_metrics_excluding, p2p_status_for_rpc, p2p_status_snapshot_metrics,
        ApiResponse, NodeRpcSnapshotMetrics, P2pStatusSnapshotMetrics,
    },
    api::{
        record_rpc_handler_degraded, record_rpc_handler_timeout_avoided, record_rpc_snapshot_stale,
    },
};

#[derive(Debug, serde::Serialize)]
pub struct MetricsData {
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub circulating_supply: u64,
    pub last_block_hash: Option<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
    pub blocks_accepted_total: u64,
    pub blocks_rejected_total: u64,
    pub invalid_state_root_total: u64,
    pub invalid_state_root_by_supplied_computed_pair_total: BTreeMap<String, u64>,
    pub invalid_state_root_stale_template_total: u64,
    pub invalid_state_root_unknown_context_total: u64,
    pub parent_state_context_rebuild_total: u64,
    pub parent_state_context_unavailable_total: u64,
    pub peer_block_stale_template_misclassification_total: u64,
    pub side_dag_block_validated_total: u64,
    pub side_dag_block_invalid_state_root_total: u64,
    pub invalid_pow_total: u64,
    pub mining_templates_total: u64,
    pub mining_submits_total: u64,
    pub template_selected_parent: Option<String>,
    pub template_parent_count: u64,
    pub template_blue_score: u64,
    pub template_merge_set_size: u64,
    pub template_stale_reject_total: u64,
    pub duplicate_tx_filtered_total: u64,
    pub external_mining_submit_actor_queue_len: u64,
    pub external_mining_submit_actor_queue_full_total: u64,
    pub external_mining_submit_actor_timeout_total: u64,
    pub external_mining_submit_actor_completed_total: u64,
    pub external_mining_submit_actor_oldest_pending_age_ms: u64,
    pub p2p_blocks_received_total: u64,
    pub tx_inbound_received: u64,
    pub tx_inbound_accepted: u64,
    pub tx_inbound_duplicate: u64,
    pub tx_inbound_invalid: u64,
    pub tx_relayed: u64,
    pub tx_relay_suppressed_budget: u64,
    pub tx_relay_suppressed_duplicate: u64,
    pub sync_missing_parents_total: u64,
    pub orphan_current_count: usize,
    pub oldest_orphan_age_secs: u64,
    pub oldest_missing_parent_age_secs: u64,
    pub orphan_reprocess_attempts: u64,
    pub orphan_reprocess_success: u64,
    pub orphan_reprocess_failed_missing_parent: u64,
    pub orphan_reprocess_failed_persist: u64,
    pub orphan_reprocess_failures_by_reason: BTreeMap<String, u64>,
    pub last_orphan_reprocess_failure_reason: Option<String>,
    pub orphan_roots_discovered_total: u64,
    pub orphan_roots_requested_total: u64,
    pub orphan_roots_rate_limited_total: u64,
    pub orphan_backlog_reindexed_total: u64,
    pub orphan_backlog_revalidated_total: u64,
    pub orphan_backlog_evicted_total: u64,
    pub orphan_backlog_stale_total: u64,
    pub orphan_missing_parent_forced_reindex_total: u64,
    pub orphan_missing_parent_unactionable_state_total: u64,
    pub orphan_missing_parent_classified_after_reindex_total: u64,
    pub orphan_missing_parent_evicted_after_unactionable_total: u64,
    pub missing_parent_request_started_total: u64,
    pub missing_parent_request_already_pending_total: u64,
    pub missing_parent_retry_peer_total: u64,
    pub missing_parent_all_peers_exhausted_total: u64,
    pub missing_parent_terminal_exhausted_total: u64,
    pub missing_parent_residual_waiting_terminal_total: u64,
    pub missing_parent_retry_suppressed_exhausted_total: u64,
    pub orphan_missing_parent_stale_evicted_total: u64,
    pub orphan_missing_parent_recovery_progress_total: u64,
    pub orphan_missing_parent_terminal_evicted_total: u64,
    pub orphan_missing_parent_residual_evicted_total: u64,
    pub orphan_missing_parent_quarantined_total: u64,
    pub missing_parent_index_active_entries: usize,
    pub missing_parent_index_terminal_entries: usize,
    pub terminal_missing_parent_historical_total: u64,
    pub terminal_missing_parent_active_blocking_total: u64,
    pub terminal_missing_parent_pruned_total: u64,
    pub sync_degraded_due_to_terminal_history_total: u64,
    pub missing_parent_index_quarantined_entries: usize,
    pub missing_parent_active_entries: usize,
    pub missing_parent_terminal_entries: usize,
    pub missing_parent_quarantined_entries: usize,
    pub orphan_recovery_tick_duration_ms: u64,
    pub final_quiescence_orphan_reprocess_total: u64,
    pub final_quiescence_orphan_reprocess_success_total: u64,
    pub final_quiescence_orphan_terminalized_total: u64,
    pub final_quiescence_missing_parent_terminalized_total: u64,
    pub final_quiescence_missing_parent_quarantined_total: u64,
    pub final_quiescence_tip_reconcile_total: u64,
    pub final_quiescence_tip_reconcile_success_total: u64,
    pub final_quiescence_tip_reconcile_blocked_total: u64,
    pub final_quiescence_tip_reconcile_blocked_reason: Option<String>,
    pub final_quiescence_height_reconcile_total: u64,
    pub final_quiescence_height_reconcile_success_total: u64,
    pub final_quiescence_height_reconcile_blocked_total: u64,
    pub final_quiescence_height_reconcile_blocked_reason: Option<String>,
    pub final_quiescence_higher_tip_seen_total: u64,
    pub final_quiescence_higher_tip_fetch_attempt_total: u64,
    pub final_quiescence_higher_tip_fetch_success_total: u64,
    pub final_quiescence_higher_tip_apply_success_total: u64,
    pub final_quiescence_higher_tip_apply_rejected_total: u64,
    pub final_quiescence_height_gap_before: u64,
    pub final_quiescence_height_gap_after: u64,
    pub final_quiescence_same_height_reconcile_total: u64,
    pub final_quiescence_same_height_reconcile_success_total: u64,
    pub final_quiescence_same_height_reconcile_blocked_total: u64,
    pub final_quiescence_same_height_reconcile_blocked_reason: Option<String>,
    pub final_quiescence_same_height_missing_parent_request_pending_total: u64,
    pub final_quiescence_same_height_missing_parent_request_sent_total: u64,
    pub final_quiescence_same_height_missing_parent_unavailable_total: u64,
    pub final_quiescence_same_height_candidate_resolved_total: u64,
    pub final_quiescence_same_height_competing_tip_seen_total: u64,
    pub final_quiescence_same_height_competing_tip_fetch_attempt_total: u64,
    pub final_quiescence_same_height_competing_tip_fetch_success_total: u64,
    pub final_quiescence_same_height_competing_tip_apply_success_total: u64,
    pub final_quiescence_same_height_competing_tip_apply_rejected_total: u64,
    pub final_quiescence_distinct_tips_before: u64,
    pub final_quiescence_distinct_tips_after: u64,
    pub final_quiescence_selected_sync_total: u64,
    pub final_quiescence_selected_sync_success_total: u64,
    pub final_quiescence_selected_sync_blocked_total: u64,
    pub final_quiescence_selected_sync_blocked_reason: Option<String>,
    pub final_quiescence_selected_locator_request_total: u64,
    pub final_quiescence_selected_locator_success_total: u64,
    pub final_quiescence_selected_locator_empty_total: u64,
    pub dag_sync_selected_chain_locator_total: u64,
    pub dag_sync_frontier_tips_total: u64,
    pub dag_sync_missing_parent_recovery_total: u64,
    pub dag_sync_merge_set_block_recovery_total: u64,
    pub dag_sync_selected_chain_gate_blocked_total: u64,
    pub dag_sync_selected_chain_gate_blocked_reason: Option<String>,
    pub final_quiescence_highest_common_found_total: u64,
    pub final_quiescence_missing_segment_request_total: u64,
    pub final_quiescence_missing_segment_apply_success_total: u64,
    pub final_quiescence_missing_segment_apply_rejected_total: u64,
    pub selected_segment_gap_blocks: u64,
    pub selected_segment_header_requests_total: u64,
    pub selected_segment_headers_received_total: u64,
    pub selected_segment_block_requests_total: u64,
    pub selected_segment_blocks_applied_total: u64,
    pub selected_segment_chunks_completed_total: u64,
    pub selected_segment_restarts_total: u64,
    pub selected_segment_failure_total: BTreeMap<String, u64>,
    pub selected_segment_peer_rotations_total: u64,
    pub final_quiescence_same_height_candidate_seen_total: u64,
    pub final_quiescence_same_height_candidate_fetch_total: u64,
    pub final_quiescence_same_height_candidate_apply_total: u64,
    pub final_quiescence_worst_lag_before: u64,
    pub final_quiescence_worst_lag_after: u64,
    pub peer_count: usize,
    pub peer_effective_count: usize,
    pub peer_min_target_missed_total: u64,
    pub peer_min_target_reconnect_attempt_total: u64,
    pub peer_min_target_reconnect_success_total: u64,
    pub peer_below_target_duration_seconds: u64,
    pub peer_below_target_blocked_reason: Option<String>,
    pub peer_recovery_state: String,
    pub peer_cooldown_bypassed_for_connectivity_total: u64,
    pub peer_rate_limit_recovery_suppressed_total: u64,
    pub peer_rate_limit_by_kind_total: BTreeMap<String, u64>,
    pub message_rate_limited_total: BTreeMap<String, u64>,
    pub recovery_queue_depth: usize,
    pub recovery_queue_dropped_total: u64,
    pub recovery_queue_delayed_total: u64,
    pub requested_blockdata_rate_limited_total: u64,
    pub unsolicited_blockdata_rate_limited_total: u64,
    pub peer_health_penalty_total: BTreeMap<String, u64>,
    pub peer_addressed_getblock_sent_total: u64,
    pub peer_addressed_getblock_response_total: u64,
    pub peer_addressed_getblock_timeout_total: u64,
    pub peer_addressed_getblock_transport_error_total: u64,
    pub direct_request_capable_peers: Vec<String>,
    pub connected_request_capable_session_peers: Vec<String>,
    pub peer_retention_active_total: usize,
    pub peer_retention_recovering_total: usize,
    pub peer_retention_cooldown_total: usize,
    pub peer_sync_eligible_total: usize,
    pub peer_sync_suppressed_total: usize,
    pub bootnode_reconnect_scheduled_total: u64,
    pub bootnode_reconnect_skipped_cooldown_total: u64,
    pub bootnode_reconnect_forced_from_cooldown_total: u64,
    pub bootnode_reconnect_success_total: u64,
    pub isolated_bootnode_reconnect_active: bool,
    pub peer_zero_count_duration_seconds: u64,
    pub peer_zero_reconnect_attempt_total: u64,
    pub peer_zero_reconnect_success_total: u64,
    pub peer_reconnect_suppressed_by_cooldown_total: u64,
    pub peer_reconnect_suppressed_by_rate_limit_total: u64,
    pub peer_min_target_recovered_total: u64,
    pub last_peer_reconnect_blocked_reason: Option<String>,
    pub p2p_status_snapshot: P2pStatusSnapshotMetrics,
    pub rpc_degraded_response_total: u64,
    pub rpc_snapshot_age_ms: u64,
    pub rpc_snapshot_age_ms_by_endpoint: BTreeMap<String, u64>,
    pub rpc_snapshot_stale_total: u64,
    pub rpc_handler_degraded_total: u64,
    pub rpc_handler_timeout_avoided_total: u64,
    pub rpc_handler_timeout_total: BTreeMap<String, u64>,
    pub rpc_alive_listener_timeout_total: u64,
    pub rpc_handler_inflight_total: BTreeMap<String, u64>,
    pub runtime_lock_busy_total: BTreeMap<String, u64>,
    pub degraded_snapshot_returned_total: BTreeMap<String, u64>,
    pub rpc_accept_backlog_observed: u64,
    pub oldest_inflight_rpc_handler_age_ms: u64,
    pub node_rpc_snapshot: NodeRpcSnapshotMetrics,
    pub rpc_liveness_current_degraded: bool,
    pub rpc_liveness_last_failure_unix: Option<u64>,
    pub rpc_liveness_consecutive_successes: u64,
    pub rpc_liveness_historical_degraded_total: u64,
    pub rpc_dedicated_runtime_active: bool,
    pub rpc_dedicated_runtime_worker_threads: usize,
    pub limitations: Vec<String>,
}

pub async fn get_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MetricsData>> {
    let node_snapshot = state.rpc_snapshot().load();
    let chain_handle = state.chain();
    let chain_guard = match chain_handle.try_read() {
        Ok(chain) => Some(chain),
        Err(_) => {
            record_rpc_snapshot_stale();
            record_rpc_handler_degraded();
            record_rpc_handler_timeout_avoided();
            crate::api::record_runtime_lock_busy("/metrics");
            crate::api::record_degraded_snapshot_returned("/metrics");
            None
        }
    };
    let fallback_chain;
    let chain = if let Some(chain) = chain_guard.as_deref() {
        chain
    } else {
        fallback_chain = init_chain_state(if node_snapshot.chain_id.is_empty() {
            "unknown".to_string()
        } else {
            node_snapshot.chain_id.clone()
        });
        &fallback_chain
    };
    let snapshot = pulsedag_core::dev_difficulty_snapshot(chain);
    let runtime_handle = state.runtime();
    let runtime_guard = match runtime_handle.try_read() {
        Ok(runtime) => Some(runtime),
        Err(_) => {
            record_rpc_snapshot_stale();
            record_rpc_handler_degraded();
            record_rpc_handler_timeout_avoided();
            crate::api::record_runtime_lock_busy("/metrics");
            crate::api::record_degraded_snapshot_returned("/metrics");
            None
        }
    };
    let fallback_runtime;
    let runtime = if let Some(runtime) = runtime_guard.as_deref() {
        runtime
    } else {
        fallback_runtime = crate::api::NodeRuntimeStats::default();
        &fallback_runtime
    };
    let p2p_status = p2p_status_for_rpc(state.p2p(), "/metrics")
        .await
        .ok()
        .flatten();
    let snapshot_metrics = p2p_status_snapshot_metrics();
    let node_snapshot_metrics =
        node_rpc_snapshot_metrics_excluding(&node_snapshot, Some("/metrics"));
    let peer_count = p2p_status
        .as_ref()
        .map(|snapshot| snapshot.status.connected_peers.len())
        .unwrap_or(0);
    let circulating_supply = chain.utxo.utxos.values().map(|u| u.amount).sum();
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());

    Json(ApiResponse::ok(MetricsData {
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        circulating_supply,
        last_block_hash,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
        blocks_accepted_total: runtime.pulsedag_blocks_accepted_total,
        blocks_rejected_total: runtime.pulsedag_blocks_rejected_total,
        invalid_state_root_total: runtime.invalid_state_root_total,
        invalid_state_root_by_supplied_computed_pair_total: runtime
            .invalid_state_root_by_supplied_computed_pair_total
            .clone(),
        invalid_state_root_stale_template_total: runtime.invalid_state_root_stale_template_total,
        invalid_state_root_unknown_context_total: runtime.invalid_state_root_unknown_context_total,
        parent_state_context_rebuild_total: runtime.parent_state_context_rebuild_total,
        parent_state_context_unavailable_total: runtime.parent_state_context_unavailable_total,
        peer_block_stale_template_misclassification_total: runtime
            .peer_block_stale_template_misclassification_total,
        side_dag_block_validated_total: runtime.side_dag_block_validated_total,
        side_dag_block_invalid_state_root_total: runtime.side_dag_block_invalid_state_root_total,
        invalid_pow_total: runtime.pulsedag_invalid_pow_total,
        mining_templates_total: runtime.pulsedag_mining_templates_total,
        mining_submits_total: runtime.pulsedag_mining_submits_total,
        template_selected_parent: runtime.template_selected_parent.clone(),
        template_parent_count: runtime.template_parent_count,
        template_blue_score: runtime.template_blue_score,
        template_merge_set_size: runtime.template_merge_set_size,
        template_stale_reject_total: runtime.template_stale_reject_total,
        duplicate_tx_filtered_total: runtime.duplicate_tx_filtered_total,
        external_mining_submit_actor_queue_len: runtime.external_mining_submit_actor_queue_len,
        external_mining_submit_actor_queue_full_total: runtime
            .external_mining_submit_actor_queue_full_total,
        external_mining_submit_actor_timeout_total: runtime
            .external_mining_submit_actor_timeout_total,
        external_mining_submit_actor_completed_total: runtime
            .external_mining_submit_actor_completed_total,
        external_mining_submit_actor_oldest_pending_age_ms: runtime
            .external_mining_submit_actor_oldest_pending_age_ms,
        p2p_blocks_received_total: runtime.pulsedag_p2p_blocks_received_total,
        tx_inbound_received: runtime.tx_inbound_received,
        tx_inbound_accepted: runtime.tx_inbound_accepted,
        tx_inbound_duplicate: runtime.tx_inbound_duplicate,
        tx_inbound_invalid: runtime.tx_inbound_invalid,
        tx_relayed: runtime.tx_relayed.saturating_add(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.tx_relayed as u64)
                .unwrap_or(0),
        ),
        tx_relay_suppressed_budget: runtime.tx_relay_suppressed_budget.saturating_add(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.tx_relay_suppressed_budget as u64)
                .unwrap_or(0),
        ),
        tx_relay_suppressed_duplicate: runtime.tx_relay_suppressed_duplicate.saturating_add(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.tx_relay_suppressed_duplicate as u64)
                .unwrap_or(0),
        ),
        sync_missing_parents_total: runtime.pulsedag_sync_missing_parents_total,
        orphan_current_count: chain.orphan_blocks.len(),
        oldest_orphan_age_secs: runtime.oldest_orphan_age_secs,
        oldest_missing_parent_age_secs: runtime.oldest_missing_parent_age_secs,
        orphan_reprocess_attempts: runtime.orphan_reprocess_attempts,
        orphan_reprocess_success: runtime.orphan_reprocess_success,
        orphan_reprocess_failed_missing_parent: runtime.orphan_reprocess_failed_missing_parent,
        orphan_reprocess_failed_persist: runtime.orphan_reprocess_failed_persist,
        orphan_reprocess_failures_by_reason: runtime.orphan_reprocess_failures_by_reason.clone(),
        last_orphan_reprocess_failure_reason: runtime.last_orphan_reprocess_failure_reason.clone(),
        orphan_roots_discovered_total: runtime.orphan_roots_discovered_total,
        orphan_roots_requested_total: runtime.orphan_roots_requested_total,
        orphan_roots_rate_limited_total: runtime.orphan_roots_rate_limited_total,
        orphan_backlog_reindexed_total: runtime.orphan_backlog_reindexed_total,
        orphan_backlog_revalidated_total: runtime.orphan_backlog_revalidated_total,
        orphan_backlog_evicted_total: runtime.orphan_backlog_evicted_total,
        orphan_backlog_stale_total: runtime.orphan_backlog_stale_total,
        orphan_missing_parent_forced_reindex_total: runtime
            .orphan_missing_parent_forced_reindex_total,
        orphan_missing_parent_unactionable_state_total: runtime
            .orphan_missing_parent_unactionable_state_total,
        orphan_missing_parent_classified_after_reindex_total: runtime
            .orphan_missing_parent_classified_after_reindex_total,
        orphan_missing_parent_evicted_after_unactionable_total: runtime
            .orphan_missing_parent_evicted_after_unactionable_total,
        missing_parent_request_started_total: runtime.missing_parent_request_started_total,
        missing_parent_request_already_pending_total: runtime
            .missing_parent_request_already_pending_total,
        missing_parent_retry_peer_total: runtime.missing_parent_retry_peer_total,
        missing_parent_all_peers_exhausted_total: runtime.missing_parent_all_peers_exhausted_total,
        missing_parent_terminal_exhausted_total: runtime.missing_parent_terminal_exhausted_total,
        missing_parent_residual_waiting_terminal_total: runtime
            .missing_parent_residual_waiting_terminal_total,
        missing_parent_retry_suppressed_exhausted_total: runtime
            .missing_parent_retry_suppressed_exhausted_total,
        orphan_missing_parent_stale_evicted_total: runtime
            .orphan_missing_parent_stale_evicted_total,
        orphan_missing_parent_recovery_progress_total: runtime
            .orphan_missing_parent_recovery_progress_total,
        orphan_missing_parent_terminal_evicted_total: runtime
            .orphan_missing_parent_terminal_evicted_total,
        orphan_missing_parent_residual_evicted_total: runtime
            .orphan_missing_parent_residual_evicted_total,
        orphan_missing_parent_quarantined_total: runtime.orphan_missing_parent_quarantined_total,
        missing_parent_index_active_entries: chain.orphan_parent_index.len(),
        missing_parent_index_terminal_entries: chain.terminal_missing_parents.len(),
        terminal_missing_parent_historical_total:
            pulsedag_core::terminal_missing_parent_historical_count(chain) as u64,
        terminal_missing_parent_active_blocking_total:
            pulsedag_core::terminal_missing_parent_active_blocking_count(chain) as u64,
        terminal_missing_parent_pruned_total: runtime.terminal_missing_parent_pruned_total,
        sync_degraded_due_to_terminal_history_total: runtime
            .sync_degraded_due_to_terminal_history_total,
        missing_parent_index_quarantined_entries: pulsedag_core::quarantined_missing_parent_count(
            chain,
        ),
        missing_parent_active_entries: chain.orphan_parent_index.len(),
        missing_parent_terminal_entries: chain
            .terminal_missing_parents
            .len()
            .saturating_sub(pulsedag_core::quarantined_missing_parent_count(chain)),
        missing_parent_quarantined_entries: pulsedag_core::quarantined_missing_parent_count(chain),
        orphan_recovery_tick_duration_ms: runtime.orphan_recovery_tick_duration_ms,
        final_quiescence_orphan_reprocess_total: runtime.final_quiescence_orphan_reprocess_total,
        final_quiescence_orphan_reprocess_success_total: runtime
            .final_quiescence_orphan_reprocess_success_total,
        final_quiescence_orphan_terminalized_total: runtime
            .final_quiescence_orphan_terminalized_total,
        final_quiescence_missing_parent_terminalized_total: runtime
            .final_quiescence_missing_parent_terminalized_total,
        final_quiescence_missing_parent_quarantined_total: runtime
            .final_quiescence_missing_parent_quarantined_total,
        final_quiescence_tip_reconcile_total: runtime.final_quiescence_tip_reconcile_total,
        final_quiescence_tip_reconcile_success_total: runtime
            .final_quiescence_tip_reconcile_success_total,
        final_quiescence_tip_reconcile_blocked_total: runtime
            .final_quiescence_tip_reconcile_blocked_total,
        final_quiescence_tip_reconcile_blocked_reason: runtime
            .final_quiescence_tip_reconcile_blocked_reason
            .clone(),
        final_quiescence_height_reconcile_total: runtime.final_quiescence_height_reconcile_total,
        final_quiescence_height_reconcile_success_total: runtime
            .final_quiescence_height_reconcile_success_total,
        final_quiescence_height_reconcile_blocked_total: runtime
            .final_quiescence_height_reconcile_blocked_total,
        final_quiescence_height_reconcile_blocked_reason: runtime
            .final_quiescence_height_reconcile_blocked_reason
            .clone(),
        final_quiescence_higher_tip_seen_total: runtime.final_quiescence_higher_tip_seen_total,
        final_quiescence_higher_tip_fetch_attempt_total: runtime
            .final_quiescence_higher_tip_fetch_attempt_total,
        final_quiescence_higher_tip_fetch_success_total: runtime
            .final_quiescence_higher_tip_fetch_success_total,
        final_quiescence_higher_tip_apply_success_total: runtime
            .final_quiescence_higher_tip_apply_success_total,
        final_quiescence_higher_tip_apply_rejected_total: runtime
            .final_quiescence_higher_tip_apply_rejected_total,
        final_quiescence_height_gap_before: runtime.final_quiescence_height_gap_before,
        final_quiescence_height_gap_after: runtime.final_quiescence_height_gap_after,
        final_quiescence_same_height_reconcile_total: runtime
            .final_quiescence_same_height_reconcile_total,
        final_quiescence_same_height_reconcile_success_total: runtime
            .final_quiescence_same_height_reconcile_success_total,
        final_quiescence_same_height_reconcile_blocked_total: runtime
            .final_quiescence_same_height_reconcile_blocked_total,
        final_quiescence_same_height_reconcile_blocked_reason: runtime
            .final_quiescence_same_height_reconcile_blocked_reason
            .clone(),
        final_quiescence_same_height_missing_parent_request_pending_total: runtime
            .final_quiescence_same_height_missing_parent_request_pending_total,
        final_quiescence_same_height_missing_parent_request_sent_total: runtime
            .final_quiescence_same_height_missing_parent_request_sent_total,
        final_quiescence_same_height_missing_parent_unavailable_total: runtime
            .final_quiescence_same_height_missing_parent_unavailable_total,
        final_quiescence_same_height_candidate_resolved_total: runtime
            .final_quiescence_same_height_candidate_resolved_total,
        final_quiescence_same_height_competing_tip_seen_total: runtime
            .final_quiescence_same_height_competing_tip_seen_total,
        final_quiescence_same_height_competing_tip_fetch_attempt_total: runtime
            .final_quiescence_same_height_competing_tip_fetch_attempt_total,
        final_quiescence_same_height_competing_tip_fetch_success_total: runtime
            .final_quiescence_same_height_competing_tip_fetch_success_total,
        final_quiescence_same_height_competing_tip_apply_success_total: runtime
            .final_quiescence_same_height_competing_tip_apply_success_total,
        final_quiescence_same_height_competing_tip_apply_rejected_total: runtime
            .final_quiescence_same_height_competing_tip_apply_rejected_total,
        final_quiescence_distinct_tips_before: runtime.final_quiescence_distinct_tips_before,
        final_quiescence_distinct_tips_after: runtime.final_quiescence_distinct_tips_after,
        final_quiescence_selected_sync_total: runtime.final_quiescence_selected_sync_total,
        final_quiescence_selected_sync_success_total: runtime
            .final_quiescence_selected_sync_success_total,
        final_quiescence_selected_sync_blocked_total: runtime
            .final_quiescence_selected_sync_blocked_total,
        final_quiescence_selected_sync_blocked_reason: runtime
            .final_quiescence_selected_sync_blocked_reason
            .clone(),
        final_quiescence_selected_locator_request_total: runtime
            .final_quiescence_selected_locator_request_total,
        final_quiescence_selected_locator_success_total: runtime
            .final_quiescence_selected_locator_success_total,
        final_quiescence_selected_locator_empty_total: runtime
            .final_quiescence_selected_locator_empty_total,
        dag_sync_selected_chain_locator_total: runtime.dag_sync_selected_chain_locator_total,
        dag_sync_frontier_tips_total: runtime.dag_sync_frontier_tips_total,
        dag_sync_missing_parent_recovery_total: runtime.dag_sync_missing_parent_recovery_total,
        dag_sync_merge_set_block_recovery_total: runtime.dag_sync_merge_set_block_recovery_total,
        dag_sync_selected_chain_gate_blocked_total: runtime
            .dag_sync_selected_chain_gate_blocked_total,
        dag_sync_selected_chain_gate_blocked_reason: runtime
            .dag_sync_selected_chain_gate_blocked_reason
            .clone(),
        final_quiescence_highest_common_found_total: runtime
            .final_quiescence_highest_common_found_total,
        final_quiescence_missing_segment_request_total: runtime
            .final_quiescence_missing_segment_request_total,
        final_quiescence_missing_segment_apply_success_total: runtime
            .final_quiescence_missing_segment_apply_success_total,
        final_quiescence_missing_segment_apply_rejected_total: runtime
            .final_quiescence_missing_segment_apply_rejected_total,
        selected_segment_gap_blocks: runtime.selected_segment_gap_blocks,
        selected_segment_header_requests_total: runtime.selected_segment_header_requests_total,
        selected_segment_headers_received_total: runtime.selected_segment_headers_received_total,
        selected_segment_block_requests_total: runtime.selected_segment_block_requests_total,
        selected_segment_blocks_applied_total: runtime.selected_segment_blocks_applied_total,
        selected_segment_chunks_completed_total: runtime.selected_segment_chunks_completed_total,
        selected_segment_restarts_total: runtime.selected_segment_restarts_total,
        selected_segment_failure_total: runtime.selected_segment_failure_total.clone(),
        selected_segment_peer_rotations_total: runtime.selected_segment_peer_rotations_total,
        final_quiescence_same_height_candidate_seen_total: runtime
            .final_quiescence_same_height_candidate_seen_total,
        final_quiescence_same_height_candidate_fetch_total: runtime
            .final_quiescence_same_height_candidate_fetch_total,
        final_quiescence_same_height_candidate_apply_total: runtime
            .final_quiescence_same_height_candidate_apply_total,
        final_quiescence_worst_lag_before: runtime.final_quiescence_worst_lag_before,
        final_quiescence_worst_lag_after: runtime.final_quiescence_worst_lag_after,
        peer_count,
        peer_effective_count: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_effective_count)
            .unwrap_or(peer_count),
        peer_min_target_missed_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_min_target_missed_total)
            .unwrap_or(0),
        peer_min_target_reconnect_attempt_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_min_target_reconnect_attempt_total)
            .unwrap_or(0),
        peer_min_target_reconnect_success_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_min_target_reconnect_success_total)
            .unwrap_or(0),
        peer_below_target_duration_seconds: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_below_target_duration_seconds)
            .unwrap_or(0),
        peer_below_target_blocked_reason: p2p_status
            .as_ref()
            .and_then(|snapshot| snapshot.status.peer_below_target_blocked_reason.clone()),
        peer_recovery_state: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_recovery_state.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        peer_cooldown_bypassed_for_connectivity_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .peer_cooldown_bypassed_for_connectivity_total
            })
            .unwrap_or(0),
        peer_rate_limit_recovery_suppressed_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_rate_limit_recovery_suppressed_total)
            .unwrap_or(0),
        peer_rate_limit_by_kind_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .peer_rate_limit_by_kind_total
                    .iter()
                    .map(|(kind, total)| (kind.clone(), *total))
                    .collect()
            })
            .unwrap_or_default(),
        message_rate_limited_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .message_rate_limited_total
                    .clone()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
        recovery_queue_depth: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.recovery_queue_depth)
            .unwrap_or(0),
        recovery_queue_dropped_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.recovery_queue_dropped_total)
            .unwrap_or(0),
        recovery_queue_delayed_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.recovery_queue_delayed_total)
            .unwrap_or(0),
        requested_blockdata_rate_limited_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.requested_blockdata_rate_limited_total)
            .unwrap_or(0),
        unsolicited_blockdata_rate_limited_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.unsolicited_blockdata_rate_limited_total)
            .unwrap_or(0),
        peer_health_penalty_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .peer_health_penalty_total
                    .clone()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
        peer_addressed_getblock_sent_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_addressed_getblock_sent_total)
            .unwrap_or(0),
        peer_addressed_getblock_response_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_addressed_getblock_response_total)
            .unwrap_or(0),
        peer_addressed_getblock_timeout_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_addressed_getblock_timeout_total)
            .unwrap_or(0),
        peer_addressed_getblock_transport_error_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .peer_addressed_getblock_transport_error_total
            })
            .unwrap_or(0),
        direct_request_capable_peers: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.direct_request_capable_peers.clone())
            .unwrap_or_default(),
        connected_request_capable_session_peers: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .connected_request_capable_session_peers
                    .clone()
            })
            .unwrap_or_default(),
        peer_retention_active_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_retention_active_total)
            .unwrap_or(0),
        peer_retention_recovering_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_retention_recovering_total)
            .unwrap_or(0),
        peer_retention_cooldown_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_retention_cooldown_total)
            .unwrap_or(0),
        peer_sync_eligible_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_sync_eligible_total)
            .unwrap_or(0),
        peer_sync_suppressed_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_sync_suppressed_total)
            .unwrap_or(0),
        bootnode_reconnect_scheduled_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.bootnode_reconnect_scheduled_total)
            .unwrap_or(0),
        bootnode_reconnect_skipped_cooldown_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.bootnode_reconnect_skipped_cooldown_total)
            .unwrap_or(0),
        bootnode_reconnect_forced_from_cooldown_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .bootnode_reconnect_forced_from_cooldown_total
            })
            .unwrap_or(0),
        bootnode_reconnect_success_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.bootnode_reconnect_success_total)
            .unwrap_or(0),
        isolated_bootnode_reconnect_active: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.isolated_bootnode_reconnect_active)
            .unwrap_or(false),
        peer_zero_count_duration_seconds: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_zero_count_duration_seconds)
            .unwrap_or(0),
        peer_zero_reconnect_attempt_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_zero_reconnect_attempt_total)
            .unwrap_or(0),
        peer_zero_reconnect_success_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_zero_reconnect_success_total)
            .unwrap_or(0),
        peer_reconnect_suppressed_by_cooldown_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_reconnect_suppressed_by_cooldown_total)
            .unwrap_or(0),
        peer_reconnect_suppressed_by_rate_limit_total: p2p_status
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .status
                    .peer_reconnect_suppressed_by_rate_limit_total
            })
            .unwrap_or(0),
        peer_min_target_recovered_total: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_min_target_recovered_total)
            .unwrap_or(0),
        last_peer_reconnect_blocked_reason: p2p_status
            .as_ref()
            .and_then(|snapshot| snapshot.status.last_peer_reconnect_blocked_reason.clone()),
        p2p_status_snapshot: snapshot_metrics,
        rpc_degraded_response_total: snapshot_metrics.rpc_degraded_response_total,
        rpc_snapshot_age_ms: node_snapshot_metrics.rpc_snapshot_age_ms,
        rpc_snapshot_age_ms_by_endpoint: node_snapshot_metrics
            .rpc_snapshot_age_ms_by_endpoint
            .clone(),
        rpc_snapshot_stale_total: node_snapshot_metrics.rpc_snapshot_stale_total,
        rpc_handler_degraded_total: node_snapshot_metrics.rpc_handler_degraded_total,
        rpc_handler_timeout_avoided_total: node_snapshot_metrics.rpc_handler_timeout_avoided_total,
        rpc_handler_timeout_total: node_snapshot_metrics.rpc_handler_timeout_total.clone(),
        rpc_alive_listener_timeout_total: node_snapshot_metrics.rpc_alive_listener_timeout_total,
        rpc_handler_inflight_total: node_snapshot_metrics.rpc_handler_inflight_total.clone(),
        runtime_lock_busy_total: node_snapshot_metrics.runtime_lock_busy_total.clone(),
        degraded_snapshot_returned_total: node_snapshot_metrics
            .degraded_snapshot_returned_total
            .clone(),
        rpc_accept_backlog_observed: node_snapshot_metrics.rpc_accept_backlog_observed,
        oldest_inflight_rpc_handler_age_ms: node_snapshot_metrics
            .oldest_inflight_rpc_handler_age_ms,
        node_rpc_snapshot: node_snapshot_metrics,
        rpc_liveness_current_degraded: runtime.rpc_liveness_current_degraded,
        rpc_liveness_last_failure_unix: runtime.rpc_liveness_last_failure_unix,
        rpc_liveness_consecutive_successes: runtime.rpc_liveness_consecutive_successes,
        rpc_liveness_historical_degraded_total: runtime.rpc_liveness_historical_degraded_total,
        rpc_dedicated_runtime_active: runtime.rpc_dedicated_runtime_active,
        rpc_dedicated_runtime_worker_threads: runtime.rpc_dedicated_runtime_worker_threads,
        limitations: vec![
            "Counters reset on node restart.".to_string(),
            "Peer and orphan counts are point-in-time snapshots.".to_string(),
        ],
    }))
}
