use axum::{extract::State, Json};
use std::collections::BTreeMap;

use crate::{
    api::RpcStateLike,
    api::{
        node_rpc_snapshot_metrics, p2p_status_for_rpc, p2p_status_snapshot_metrics, ApiResponse,
        NodeRpcSnapshotMetrics, P2pStatusSnapshotMetrics,
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
    pub invalid_pow_total: u64,
    pub mining_templates_total: u64,
    pub mining_submits_total: u64,
    pub external_mining_submit_actor_queue_len: u64,
    pub external_mining_submit_actor_queue_full_total: u64,
    pub external_mining_submit_actor_timeout_total: u64,
    pub external_mining_submit_actor_completed_total: u64,
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
    pub orphan_recovery_tick_duration_ms: u64,
    pub peer_count: usize,
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
    pub p2p_status_snapshot: P2pStatusSnapshotMetrics,
    pub rpc_degraded_response_total: u64,
    pub rpc_snapshot_age_ms: u64,
    pub rpc_snapshot_stale_total: u64,
    pub rpc_handler_degraded_total: u64,
    pub rpc_handler_timeout_avoided_total: u64,
    pub node_rpc_snapshot: NodeRpcSnapshotMetrics,
    pub rpc_dedicated_runtime_active: bool,
    pub rpc_dedicated_runtime_worker_threads: usize,
    pub limitations: Vec<String>,
}

pub async fn get_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MetricsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let runtime = state.runtime();
    let runtime = runtime.read().await;
    let p2p_status = p2p_status_for_rpc(state.p2p(), "/metrics")
        .await
        .ok()
        .flatten();
    let snapshot_metrics = p2p_status_snapshot_metrics();
    let node_snapshot = state.rpc_snapshot().load();
    let node_snapshot_metrics = node_rpc_snapshot_metrics(&node_snapshot);
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
        invalid_pow_total: runtime.pulsedag_invalid_pow_total,
        mining_templates_total: runtime.pulsedag_mining_templates_total,
        mining_submits_total: runtime.pulsedag_mining_submits_total,
        external_mining_submit_actor_queue_len: runtime.external_mining_submit_actor_queue_len,
        external_mining_submit_actor_queue_full_total: runtime
            .external_mining_submit_actor_queue_full_total,
        external_mining_submit_actor_timeout_total: runtime
            .external_mining_submit_actor_timeout_total,
        external_mining_submit_actor_completed_total: runtime
            .external_mining_submit_actor_completed_total,
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
        orphan_recovery_tick_duration_ms: runtime.orphan_recovery_tick_duration_ms,
        peer_count,
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
        p2p_status_snapshot: snapshot_metrics,
        rpc_degraded_response_total: snapshot_metrics.rpc_degraded_response_total,
        rpc_snapshot_age_ms: node_snapshot_metrics.rpc_snapshot_age_ms,
        rpc_snapshot_stale_total: node_snapshot_metrics.rpc_snapshot_stale_total,
        rpc_handler_degraded_total: node_snapshot_metrics.rpc_handler_degraded_total,
        rpc_handler_timeout_avoided_total: node_snapshot_metrics.rpc_handler_timeout_avoided_total,
        node_rpc_snapshot: node_snapshot_metrics,
        rpc_dedicated_runtime_active: runtime.rpc_dedicated_runtime_active,
        rpc_dedicated_runtime_worker_threads: runtime.rpc_dedicated_runtime_worker_threads,
        limitations: vec![
            "Counters reset on node restart.".to_string(),
            "Peer and orphan counts are point-in-time snapshots.".to_string(),
        ],
    }))
}
