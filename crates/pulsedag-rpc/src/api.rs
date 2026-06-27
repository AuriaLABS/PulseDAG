use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, RwLock as StdRwLock,
    },
};

use pulsedag_core::state::ChainState;
use pulsedag_core::{
    InvalidStateRootClassification, InvalidStateRootDiagnostics, SyncPipelineStatus,
};
use pulsedag_p2p::{P2pHandle, P2pStatus};
use pulsedag_storage::Storage;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{RwLock, RwLockReadGuard, Semaphore},
    time::{timeout, Duration},
};

pub use pulsedag_api::{
    ApiError, ApiMeta, ApiResponse, GetBlockTemplateRequest, MineRequest, SubmitMinedBlockRequest,
};

pub const RPC_SNAPSHOT_STALE_AFTER_MS: u64 = 5_000;
static RPC_SNAPSHOT_STALE_TOTAL: AtomicU64 = AtomicU64::new(0);
static RPC_HANDLER_DEGRADED_TOTAL: AtomicU64 = AtomicU64::new(0);
static RPC_HANDLER_TIMEOUT_AVOIDED_TOTAL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissingParentSnapshotEntry {
    pub parent: String,
    pub waiting_orphans: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRpcSnapshot {
    pub chain_id: String,
    pub height: u64,
    pub tip: Option<String>,
    pub peer_count: usize,
    pub orphan_count: usize,
    pub pending_missing_parents: usize,
    pub missing_parent_entries: Vec<MissingParentSnapshotEntry>,
    #[serde(default)]
    pub terminal_missing_parent_entries: Vec<MissingParentSnapshotEntry>,
    #[serde(default)]
    pub quarantined_missing_parent_entries: Vec<MissingParentSnapshotEntry>,
    pub inv_hashes_requested: u64,
    pub last_updated_ms: u64,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub pending_block_requests: usize,
    #[serde(default)]
    pub inflight_block_requests: usize,
    #[serde(default)]
    pub sync_state: String,
}

impl Default for NodeRpcSnapshot {
    fn default() -> Self {
        Self {
            chain_id: String::new(),
            height: 0,
            tip: None,
            peer_count: 0,
            orphan_count: 0,
            pending_missing_parents: 0,
            missing_parent_entries: Vec::new(),
            terminal_missing_parent_entries: Vec::new(),
            quarantined_missing_parent_entries: Vec::new(),
            inv_hashes_requested: 0,
            last_updated_ms: unix_now_ms(),
            degraded: true,
            degraded_reason: Some("node RPC snapshot has not been captured yet".to_string()),
            stale: true,
            pending_block_requests: 0,
            inflight_block_requests: 0,
            sync_state: "unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeRpcSnapshotStore {
    inner: Arc<StdRwLock<NodeRpcSnapshot>>,
}

impl Default for NodeRpcSnapshotStore {
    fn default() -> Self {
        Self::new(NodeRpcSnapshot::default())
    }
}

impl NodeRpcSnapshotStore {
    pub fn new(snapshot: NodeRpcSnapshot) -> Self {
        Self {
            inner: Arc::new(StdRwLock::new(snapshot)),
        }
    }

    pub fn load(&self) -> NodeRpcSnapshot {
        let mut snapshot = self
            .inner
            .try_read()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| {
                record_rpc_snapshot_stale();
                NodeRpcSnapshot {
                    degraded_reason: Some(
                        "node RPC snapshot read lock was busy; synthesized degraded snapshot"
                            .to_string(),
                    ),
                    ..NodeRpcSnapshot::default()
                }
            });
        mark_node_rpc_snapshot_stale_if_needed(&mut snapshot);
        snapshot
    }

    pub fn store(&self, snapshot: NodeRpcSnapshot) {
        if let Ok(mut guard) = self.inner.try_write() {
            *guard = snapshot;
        }
    }

    pub fn degraded_snapshot(&self, reason: impl Into<String>) -> NodeRpcSnapshot {
        let reason = reason.into();
        record_rpc_snapshot_stale();
        record_rpc_handler_degraded();
        record_rpc_handler_timeout_avoided();
        let mut snapshot = self.load();
        snapshot.degraded = true;
        snapshot.stale = true;
        snapshot.degraded_reason = Some(reason);
        snapshot
    }
}

pub fn mark_node_rpc_snapshot_stale_if_needed(snapshot: &mut NodeRpcSnapshot) {
    if unix_now_ms().saturating_sub(snapshot.last_updated_ms) > RPC_SNAPSHOT_STALE_AFTER_MS {
        snapshot.stale = true;
        snapshot.degraded = true;
        if snapshot.degraded_reason.is_none() {
            snapshot.degraded_reason =
                Some("node RPC snapshot exceeded stale age threshold".to_string());
        }
        record_rpc_snapshot_stale();
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NodeRpcSnapshotMetrics {
    pub rpc_snapshot_age_ms: u64,
    pub rpc_snapshot_stale_total: u64,
    pub rpc_handler_degraded_total: u64,
    pub rpc_handler_timeout_avoided_total: u64,
}

pub fn node_rpc_snapshot_metrics(snapshot: &NodeRpcSnapshot) -> NodeRpcSnapshotMetrics {
    NodeRpcSnapshotMetrics {
        rpc_snapshot_age_ms: unix_now_ms().saturating_sub(snapshot.last_updated_ms),
        rpc_snapshot_stale_total: RPC_SNAPSHOT_STALE_TOTAL.load(Ordering::Relaxed),
        rpc_handler_degraded_total: RPC_HANDLER_DEGRADED_TOTAL.load(Ordering::Relaxed),
        rpc_handler_timeout_avoided_total: RPC_HANDLER_TIMEOUT_AVOIDED_TOTAL
            .load(Ordering::Relaxed),
    }
}

pub fn record_rpc_snapshot_stale() {
    RPC_SNAPSHOT_STALE_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn record_rpc_handler_degraded() {
    RPC_HANDLER_DEGRADED_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn record_rpc_handler_timeout_avoided() {
    RPC_HANDLER_TIMEOUT_AVOIDED_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn build_node_rpc_snapshot(
    chain: &ChainState,
    runtime: &NodeRuntimeStats,
    p2p_status: Option<&P2pStatus>,
) -> NodeRpcSnapshot {
    let mut missing_parent_entries = chain
        .orphan_parent_index
        .iter()
        .map(|(parent, waiting)| MissingParentSnapshotEntry {
            parent: parent.clone(),
            waiting_orphans: waiting.iter().cloned().collect(),
        })
        .collect::<Vec<_>>();
    missing_parent_entries.sort_by(|left, right| left.parent.cmp(&right.parent));
    let mut terminal_missing_parent_entries = chain
        .terminal_missing_parents
        .iter()
        .filter(|(_, entry)| {
            !matches!(
                entry.state,
                pulsedag_core::state::MissingParentState::Quarantined
            )
        })
        .map(|(parent, entry)| MissingParentSnapshotEntry {
            parent: parent.clone(),
            waiting_orphans: entry.waiting_orphans.clone(),
        })
        .collect::<Vec<_>>();
    let mut quarantined_missing_parent_entries = chain
        .terminal_missing_parents
        .iter()
        .filter(|(_, entry)| {
            matches!(
                entry.state,
                pulsedag_core::state::MissingParentState::Quarantined
            )
        })
        .map(|(parent, entry)| MissingParentSnapshotEntry {
            parent: parent.clone(),
            waiting_orphans: entry.waiting_orphans.clone(),
        })
        .collect::<Vec<_>>();
    terminal_missing_parent_entries.sort_by(|left, right| left.parent.cmp(&right.parent));
    quarantined_missing_parent_entries.sort_by(|left, right| left.parent.cmp(&right.parent));
    NodeRpcSnapshot {
        chain_id: chain.chain_id.clone(),
        height: chain.dag.best_height,
        tip: pulsedag_core::preferred_tip_hash(chain),
        peer_count: p2p_status
            .map(|status| status.connected_peers.len())
            .unwrap_or(0),
        orphan_count: chain.orphan_blocks.len(),
        pending_missing_parents: pulsedag_core::pending_missing_parent_count(chain),
        missing_parent_entries,
        terminal_missing_parent_entries,
        quarantined_missing_parent_entries,
        inv_hashes_requested: p2p_status
            .map(|status| status.inv_hashes_requested as u64)
            .unwrap_or(0),
        last_updated_ms: unix_now_ms(),
        degraded: false,
        degraded_reason: None,
        stale: false,
        pending_block_requests: runtime.pending_block_requests,
        inflight_block_requests: runtime.inflight_block_requests,
        sync_state: runtime.sync_state.clone(),
    }
}

pub async fn capture_and_store_node_rpc_snapshot<S: RpcStateLike>(
    state: &S,
) -> Result<NodeRpcSnapshot, String> {
    let p2p_status = p2p_status_for_rpc(state.p2p(), "/rpc/snapshot")
        .await
        .ok()
        .flatten();
    let chain_handle = state.chain();
    let chain = chain_handle.try_read().map_err(|_| {
        "node RPC snapshot capture skipped because chain read lock is busy".to_string()
    })?;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.try_read().map_err(|_| {
        "node RPC snapshot capture skipped because runtime read lock is busy".to_string()
    })?;
    let snapshot =
        build_node_rpc_snapshot(&chain, &runtime, p2p_status.as_ref().map(|p| &p.status));
    state.rpc_snapshot().store(snapshot.clone());
    Ok(snapshot)
}

pub async fn fresh_or_cached_node_rpc_snapshot<S: RpcStateLike>(
    state: &S,
    endpoint: &str,
) -> NodeRpcSnapshot {
    match capture_and_store_node_rpc_snapshot(state).await {
        Ok(mut snapshot) => {
            mark_node_rpc_snapshot_stale_if_needed(&mut snapshot);
            snapshot
        }
        Err(reason) => state.rpc_snapshot().degraded_snapshot(format!(
            "{endpoint} avoided waiting for fresh liveness state: {reason}"
        )),
    }
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
    pub tx_inbound_received: u64,
    pub tx_inbound_accepted: u64,
    pub tx_inbound_duplicate: u64,
    pub tx_inbound_invalid: u64,
    pub tx_relayed: u64,
    pub tx_relay_suppressed_budget: u64,
    pub tx_relay_suppressed_duplicate: u64,
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
    pub external_mining_submit_inflight: u64,
    pub external_mining_submit_started_total: u64,
    pub external_mining_submit_completed_total: u64,
    pub external_mining_submit_timeout_total: u64,
    pub external_mining_submit_actor_queue_len: u64,
    pub external_mining_submit_actor_queue_full_total: u64,
    pub external_mining_submit_actor_timeout_total: u64,
    pub external_mining_submit_actor_completed_total: u64,
    pub external_mining_last_submit_phase: Option<String>,
    pub external_mining_last_submit_duration_ms: u64,
    pub external_mining_max_submit_duration_ms: u64,
    pub external_mining_submit_lock_wait_ms: u64,
    pub external_mining_submit_accept_ms: u64,
    pub external_mining_submit_post_accept_ms: u64,
    pub pulsedag_blocks_accepted_total: u64,
    pub pulsedag_blocks_rejected_total: u64,
    #[serde(default)]
    pub rejected_blocks_by_reason: BTreeMap<String, u64>,
    #[serde(default)]
    pub invalid_state_root_total: u64,
    #[serde(default)]
    pub invalid_state_root_by_supplied_computed_pair_total: BTreeMap<String, u64>,
    #[serde(default)]
    pub invalid_state_root_stale_template_total: u64,
    #[serde(default)]
    pub invalid_state_root_unknown_context_total: u64,
    pub pulsedag_invalid_pow_total: u64,
    pub pulsedag_mining_templates_total: u64,
    pub pulsedag_mining_submits_total: u64,
    pub pulsedag_p2p_blocks_received_total: u64,
    pub pulsedag_sync_missing_parents_total: u64,
    pub block_announces_received: u64,
    pub inventory_announces_sent: u64,
    pub inventory_announces_received: u64,
    pub header_requests_sent: u64,
    pub header_requests_received: u64,
    pub headers_sent: u64,
    pub headers_received: u64,
    pub block_header_requests_received: u64,
    pub block_headers_sent: u64,
    pub block_header_batches_received: u64,
    pub block_headers_received: u64,
    pub block_fetch_parent_deferred: u64,
    pub block_fetch_duplicate_inflight_suppressed: u64,
    pub dependency_fetches_scheduled: u64,
    pub parent_first_fetches: u64,
    pub getblock_sent: u64,
    pub getblock_received: u64,
    pub blockdata_sent: u64,
    pub blockdata_received: u64,
    pub blockdata_accepted: u64,
    pub blockdata_duplicate: u64,
    pub blockdata_missing_parent: u64,
    pub blockdata_invalid_pow: u64,
    #[serde(default)]
    pub blockdata_not_found: u64,
    pub block_request_timeouts: u64,
    #[serde(default)]
    pub block_request_retries: u64,
    #[serde(default)]
    pub block_request_fallbacks: u64,
    #[serde(default)]
    pub block_request_backpressure_suppressed: u64,
    #[serde(default)]
    pub block_request_fetches_queued: u64,
    #[serde(default)]
    pub block_request_fetches_dropped: u64,
    pub duplicate_block_requests_suppressed: u64,
    pub pending_block_requests: usize,
    pub inflight_block_requests: usize,
    #[serde(default)]
    pub block_fetch_scheduler_queue_depth: usize,
    #[serde(default)]
    pub block_fetch_scheduler_inflight_by_peer: BTreeMap<String, usize>,
    #[serde(default)]
    pub pending_block_request_hashes: Vec<String>,
    pub pending_missing_parents: usize,
    #[serde(default)]
    pub orphan_backlog_retryable_ready: usize,
    #[serde(default)]
    pub orphan_backlog_waiting_missing_parent: usize,
    #[serde(default)]
    pub orphan_backlog_stale_missing_parent_entries: usize,
    #[serde(default)]
    pub orphan_backlog_unindexed_missing_parent_entries: usize,
    pub last_accepted_peer_block: Option<String>,
    pub last_rejected_peer_block_reason: Option<String>,
    pub sync_state: String,
    pub tips_requested: u64,
    pub tips_received: u64,
    pub unknown_tips_seen: u64,
    pub missing_parents_detected: u64,
    pub missing_parent_requests_sent: u64,
    #[serde(default)]
    pub missing_parent_request_started_total: u64,
    #[serde(default)]
    pub missing_parent_request_already_pending_total: u64,
    #[serde(default)]
    pub missing_parent_responses_received: u64,
    #[serde(default)]
    pub missing_parent_request_timeouts: u64,
    #[serde(default)]
    pub missing_parent_request_retries: u64,
    #[serde(default)]
    pub missing_parent_request_fallbacks: u64,
    #[serde(default)]
    pub missing_parent_peer_not_found_total: u64,
    #[serde(default)]
    pub missing_parent_peer_timeout_total: u64,
    #[serde(default)]
    pub missing_parent_peer_response_success_total: u64,
    #[serde(default)]
    pub missing_parent_all_peers_exhausted_total: u64,
    #[serde(default)]
    pub missing_parent_terminal_exhausted_total: u64,
    #[serde(default)]
    pub missing_parent_retry_suppressed_exhausted_total: u64,
    #[serde(default)]
    pub missing_parent_retry_next_peer_total: u64,
    #[serde(default)]
    pub missing_parent_retry_peer_total: u64,
    #[serde(default)]
    pub missing_parent_residual_waiting_terminal_total: u64,
    pub orphan_blocks_queued: u64,
    pub orphan_blocks_resolved: u64,
    pub orphan_blocks_retried: u64,
    #[serde(default)]
    pub orphan_reprocess_attempts: u64,
    #[serde(default)]
    pub orphan_reprocess_success: u64,
    #[serde(default)]
    pub orphan_reprocess_failed_missing_parent: u64,
    #[serde(default)]
    pub orphan_reprocess_failed_persist: u64,
    #[serde(default)]
    pub orphan_reprocess_failures_by_reason: BTreeMap<String, u64>,
    #[serde(default)]
    pub last_orphan_reprocess_failure_reason: Option<String>,
    pub orphan_blocks_evicted: u64,
    #[serde(default)]
    pub max_orphan_age_secs: u64,
    #[serde(default)]
    pub oldest_orphan_age_secs: u64,
    #[serde(default)]
    pub oldest_missing_parent_age_secs: u64,
    #[serde(default)]
    pub orphan_roots_discovered_total: u64,
    #[serde(default)]
    pub orphan_roots_requested_total: u64,
    #[serde(default)]
    pub orphan_roots_rate_limited_total: u64,
    #[serde(default)]
    pub orphan_backlog_reindexed_total: u64,
    #[serde(default)]
    pub orphan_backlog_revalidated_total: u64,
    #[serde(default)]
    pub orphan_backlog_evicted_total: u64,
    #[serde(default)]
    pub orphan_backlog_stale_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_forced_reindex_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_unactionable_state_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_classified_after_reindex_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_evicted_after_unactionable_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_stale_evicted_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_recovery_progress_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_terminal_evicted_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_residual_evicted_total: u64,
    #[serde(default)]
    pub orphan_missing_parent_quarantined_total: u64,
    #[serde(default)]
    pub missing_parent_index_active_entries: usize,
    #[serde(default)]
    pub missing_parent_index_terminal_entries: usize,
    #[serde(default)]
    pub orphan_recovery_tick_duration_ms: u64,
    #[serde(default)]
    pub final_quiescence_orphan_reprocess_total: u64,
    #[serde(default)]
    pub final_quiescence_orphan_reprocess_success_total: u64,
    #[serde(default)]
    pub final_quiescence_orphan_terminalized_total: u64,
    #[serde(default)]
    pub final_quiescence_missing_parent_terminalized_total: u64,
    #[serde(default)]
    pub final_quiescence_missing_parent_quarantined_total: u64,
    #[serde(default)]
    pub final_quiescence_tip_reconcile_total: u64,
    #[serde(default)]
    pub final_quiescence_tip_reconcile_success_total: u64,
    #[serde(default)]
    pub final_quiescence_tip_reconcile_blocked_total: u64,
    #[serde(default)]
    pub final_quiescence_tip_reconcile_blocked_reason: Option<String>,
    #[serde(default)]
    pub final_quiescence_height_reconcile_total: u64,
    #[serde(default)]
    pub final_quiescence_height_reconcile_success_total: u64,
    #[serde(default)]
    pub final_quiescence_height_reconcile_blocked_total: u64,
    #[serde(default)]
    pub final_quiescence_height_reconcile_blocked_reason: Option<String>,
    #[serde(default)]
    pub final_quiescence_higher_tip_seen_total: u64,
    #[serde(default)]
    pub final_quiescence_higher_tip_fetch_attempt_total: u64,
    #[serde(default)]
    pub final_quiescence_higher_tip_fetch_success_total: u64,
    #[serde(default)]
    pub final_quiescence_higher_tip_apply_success_total: u64,
    #[serde(default)]
    pub final_quiescence_higher_tip_apply_rejected_total: u64,
    #[serde(default)]
    pub final_quiescence_height_gap_before: u64,
    #[serde(default)]
    pub final_quiescence_height_gap_after: u64,
    #[serde(default)]
    pub final_quiescence_same_height_reconcile_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_reconcile_success_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_reconcile_blocked_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_reconcile_blocked_reason: Option<String>,
    #[serde(default)]
    pub final_quiescence_same_height_competing_tip_seen_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_competing_tip_fetch_attempt_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_competing_tip_fetch_success_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_competing_tip_apply_success_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_competing_tip_apply_rejected_total: u64,
    #[serde(default)]
    pub final_quiescence_distinct_tips_before: u64,
    #[serde(default)]
    pub final_quiescence_distinct_tips_after: u64,
    #[serde(default)]
    pub final_quiescence_selected_sync_total: u64,
    #[serde(default)]
    pub final_quiescence_selected_sync_success_total: u64,
    #[serde(default)]
    pub final_quiescence_selected_sync_blocked_total: u64,
    #[serde(default)]
    pub final_quiescence_selected_sync_blocked_reason: Option<String>,
    #[serde(default)]
    pub final_quiescence_selected_locator_request_total: u64,
    #[serde(default)]
    pub final_quiescence_selected_locator_success_total: u64,
    #[serde(default)]
    pub final_quiescence_selected_locator_empty_total: u64,
    #[serde(default)]
    pub final_quiescence_highest_common_found_total: u64,
    #[serde(default)]
    pub final_quiescence_missing_segment_request_total: u64,
    #[serde(default)]
    pub final_quiescence_missing_segment_apply_success_total: u64,
    #[serde(default)]
    pub final_quiescence_missing_segment_apply_rejected_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_candidate_seen_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_candidate_fetch_total: u64,
    #[serde(default)]
    pub final_quiescence_same_height_candidate_apply_total: u64,
    #[serde(default)]
    pub final_quiescence_worst_lag_before: u64,
    #[serde(default)]
    pub final_quiescence_worst_lag_after: u64,
    #[serde(default)]
    pub rpc_dedicated_runtime_active: bool,
    #[serde(default)]
    pub rpc_dedicated_runtime_worker_threads: usize,
    pub sync_catchup_completed: u64,
    pub sync_failures: u64,
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

impl NodeRuntimeStats {
    pub fn record_rejected_block_reason(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        let normalized = reason.trim();
        let reason = if normalized.is_empty() {
            "unknown".to_string()
        } else {
            let mut out = String::new();
            let mut previous_was_separator = false;
            for (index, ch) in normalized.chars().enumerate() {
                if ch.is_ascii_uppercase() {
                    if index > 0 && !previous_was_separator {
                        out.push('_');
                    }
                    out.push(ch.to_ascii_lowercase());
                    previous_was_separator = false;
                } else if ch.is_ascii_alphanumeric() {
                    out.push(ch.to_ascii_lowercase());
                    previous_was_separator = false;
                } else if !previous_was_separator {
                    out.push('_');
                    previous_was_separator = true;
                }
            }
            out.trim_matches('_').to_string()
        };
        let count = self.rejected_blocks_by_reason.entry(reason).or_insert(0);
        *count = count.saturating_add(1);
    }
    pub fn record_invalid_state_root(&mut self, diagnostics: &InvalidStateRootDiagnostics) {
        self.invalid_state_root_total = self.invalid_state_root_total.saturating_add(1);
        if matches!(
            diagnostics.classification,
            InvalidStateRootClassification::StaleTemplate
        ) {
            self.invalid_state_root_stale_template_total = self
                .invalid_state_root_stale_template_total
                .saturating_add(1);
        }
        if diagnostics.unknown_context {
            self.invalid_state_root_unknown_context_total = self
                .invalid_state_root_unknown_context_total
                .saturating_add(1);
        }

        const MAX_INVALID_STATE_ROOT_PAIRS: usize = 128;
        let pair = format!(
            "{}->{}",
            diagnostics.supplied_state_root, diagnostics.computed_state_root
        );
        if !self
            .invalid_state_root_by_supplied_computed_pair_total
            .contains_key(&pair)
            && self
                .invalid_state_root_by_supplied_computed_pair_total
                .len()
                >= MAX_INVALID_STATE_ROOT_PAIRS
        {
            if let Some(first_key) = self
                .invalid_state_root_by_supplied_computed_pair_total
                .keys()
                .next()
                .cloned()
            {
                self.invalid_state_root_by_supplied_computed_pair_total
                    .remove(&first_key);
            }
        }
        let count = self
            .invalid_state_root_by_supplied_computed_pair_total
            .entry(pair)
            .or_insert(0);
        *count = count.saturating_add(1);
        self.record_rejected_block_reason(format!(
            "invalid_state_root_{}",
            diagnostics.classification.as_str()
        ));
    }
}

const RPC_STATE_LOCK_TIMEOUT: Duration = Duration::from_millis(100);
static P2P_STATUS_SNAPSHOT_PERMITS: Semaphore = Semaphore::const_new(1);
static P2P_STATUS_CACHE: OnceLock<Mutex<Option<CachedP2pStatus>>> = OnceLock::new();
static P2P_STATUS_SNAPSHOT_LIVE_TOTAL: AtomicU64 = AtomicU64::new(0);
static P2P_STATUS_SNAPSHOT_BUSY_TOTAL: AtomicU64 = AtomicU64::new(0);
static P2P_STATUS_SNAPSHOT_TIMEOUT_TOTAL: AtomicU64 = AtomicU64::new(0);
static P2P_STATUS_SNAPSHOT_STALE_TOTAL: AtomicU64 = AtomicU64::new(0);
static RPC_DEGRADED_RESPONSE_TOTAL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
struct CachedP2pStatus {
    status: P2pStatus,
    captured_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct P2pStatusSnapshot {
    pub status: P2pStatus,
    pub stale: bool,
    pub degraded_reason: Option<String>,
    pub captured_at_unix: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct P2pStatusSnapshotMetrics {
    pub live_total: u64,
    pub busy_total: u64,
    pub timeout_total: u64,
    pub stale_total: u64,
    pub rpc_degraded_response_total: u64,
}

pub fn p2p_status_snapshot_metrics() -> P2pStatusSnapshotMetrics {
    P2pStatusSnapshotMetrics {
        live_total: P2P_STATUS_SNAPSHOT_LIVE_TOTAL.load(Ordering::Relaxed),
        busy_total: P2P_STATUS_SNAPSHOT_BUSY_TOTAL.load(Ordering::Relaxed),
        timeout_total: P2P_STATUS_SNAPSHOT_TIMEOUT_TOTAL.load(Ordering::Relaxed),
        stale_total: P2P_STATUS_SNAPSHOT_STALE_TOTAL.load(Ordering::Relaxed),
        rpc_degraded_response_total: RPC_DEGRADED_RESPONSE_TOTAL.load(Ordering::Relaxed),
    }
}

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cached_p2p_status() -> Option<CachedP2pStatus> {
    P2P_STATUS_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|cache| cache.clone())
}

fn update_cached_p2p_status(status: &P2pStatus) {
    if let Ok(mut cache) = P2P_STATUS_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *cache = Some(CachedP2pStatus {
            status: status.clone(),
            captured_at_unix: unix_now_secs(),
        });
    }
}

fn stale_p2p_status(reason: String) -> Result<Option<P2pStatusSnapshot>, String> {
    if let Some(cached) = cached_p2p_status() {
        P2P_STATUS_SNAPSHOT_STALE_TOTAL.fetch_add(1, Ordering::Relaxed);
        record_rpc_degraded_response();
        Ok(Some(P2pStatusSnapshot {
            status: cached.status,
            stale: true,
            degraded_reason: Some(reason),
            captured_at_unix: Some(cached.captured_at_unix),
        }))
    } else {
        record_rpc_degraded_response();
        Err(reason)
    }
}

pub fn record_rpc_degraded_response() {
    RPC_DEGRADED_RESPONSE_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_rpc_handler_degraded();
}

pub async fn read_chain_for_rpc<'a>(
    chain: &'a Arc<RwLock<ChainState>>,
    endpoint: &str,
) -> Result<RwLockReadGuard<'a, ChainState>, String> {
    timeout(RPC_STATE_LOCK_TIMEOUT, chain.read())
        .await
        .map_err(|_| {
            record_rpc_degraded_response();
            format!(
                "{endpoint} could not acquire chain read lock within {}ms; shared state is busy and RPC starvation diagnostics should inspect long-running writers",
                RPC_STATE_LOCK_TIMEOUT.as_millis()
            )
        })
}

pub async fn read_runtime_for_rpc<'a>(
    runtime: &'a Arc<RwLock<NodeRuntimeStats>>,
    endpoint: &str,
) -> Result<RwLockReadGuard<'a, NodeRuntimeStats>, String> {
    timeout(RPC_STATE_LOCK_TIMEOUT, runtime.read())
        .await
        .map_err(|_| {
            record_rpc_degraded_response();
            format!(
                "{endpoint} could not acquire runtime read lock within {}ms; shared state is busy and RPC starvation diagnostics should inspect long-running writers",
                RPC_STATE_LOCK_TIMEOUT.as_millis()
            )
        })
}

pub async fn p2p_status_for_rpc(
    p2p: Option<Arc<dyn P2pHandle>>,
    endpoint: &str,
) -> Result<Option<P2pStatusSnapshot>, String> {
    let Some(p2p) = p2p else {
        return Ok(None);
    };

    let _permit = match timeout(
        RPC_STATE_LOCK_TIMEOUT,
        P2P_STATUS_SNAPSHOT_PERMITS.acquire(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => {
            return Err(format!(
                "{endpoint} could not acquire p2p status snapshot permit because the limiter was closed"
            ));
        }
        Err(_) => {
            P2P_STATUS_SNAPSHOT_BUSY_TOTAL.fetch_add(1, Ordering::Relaxed);
            return stale_p2p_status(format!(
                "{endpoint} could not acquire p2p status snapshot permit within {}ms because a prior snapshot is still running; returning the last-known p2p snapshot when available",
                RPC_STATE_LOCK_TIMEOUT.as_millis()
            ));
        }
    };

    let status = match timeout(
        RPC_STATE_LOCK_TIMEOUT,
        tokio::task::spawn_blocking(move || p2p.status()),
    )
    .await
    {
        Ok(joined) => joined
            .map_err(|e| format!("{endpoint} p2p status snapshot task failed: {e}"))?
            .map_err(|e| format!("{endpoint} p2p status failed: {e}"))?,
        Err(_) => {
            P2P_STATUS_SNAPSHOT_TIMEOUT_TOTAL.fetch_add(1, Ordering::Relaxed);
            return stale_p2p_status(format!(
                "{endpoint} could not complete p2p status snapshot within {}ms; returning the last-known p2p snapshot when available",
                RPC_STATE_LOCK_TIMEOUT.as_millis()
            ));
        }
    };

    P2P_STATUS_SNAPSHOT_LIVE_TOTAL.fetch_add(1, Ordering::Relaxed);
    update_cached_p2p_status(&status);
    Ok(Some(P2pStatusSnapshot {
        status,
        stale: false,
        degraded_reason: None,
        captured_at_unix: Some(unix_now_secs()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        errors::PulseError,
        types::{Block, Transaction},
    };
    use pulsedag_p2p::MemoryP2pHandle;
    use std::sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        Condvar,
    };

    struct BlockingP2pHandle {
        status: P2pStatus,
        started: Arc<AtomicBool>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    struct ReleaseOnDrop(Arc<(Mutex<bool>, Condvar)>);

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let (lock, cvar) = &*self.0;
            if let Ok(mut released) = lock.lock() {
                *released = true;
                cvar.notify_all();
            }
        }
    }

    impl P2pHandle for BlockingP2pHandle {
        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &Block) -> Result<(), PulseError> {
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            self.started.store(true, AtomicOrdering::SeqCst);
            let (lock, cvar) = &*self.release;
            let released = lock
                .lock()
                .map_err(|_| PulseError::Internal("test lock poisoned".into()))?;
            let _released = cvar
                .wait_while(released, |released| !*released)
                .map_err(|_| PulseError::Internal("test condvar poisoned".into()))?;
            Ok(self.status.clone())
        }
    }

    fn memory_status(chain_id: &str, peers: Vec<String>) -> P2pStatus {
        let (handle, _inbound_rx) = MemoryP2pHandle::new(chain_id.into(), peers);
        handle.status().expect("memory p2p status")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn p2p_status_timeout_releases_snapshot_permit() {
        let cached_status = memory_status("cache", vec!["peer-cache".into()]);
        let live_status = memory_status("live", vec!["peer-live".into()]);

        p2p_status_for_rpc(
            Some(Arc::new(
                MemoryP2pHandle::new("cache".into(), vec!["peer-cache".into()]).0,
            )),
            "/test/cache",
        )
        .await
        .expect("cache seed succeeds");

        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let _release_on_drop = ReleaseOnDrop(Arc::clone(&release));
        let blocking = Arc::new(BlockingP2pHandle {
            status: cached_status,
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        });
        let before = p2p_status_snapshot_metrics();

        let stale = p2p_status_for_rpc(Some(blocking), "/test/timeout")
            .await
            .expect("timeout returns stale cached status")
            .expect("cached status exists");
        assert!(stale.stale, "timeout should return a stale cached status");
        assert!(
            started.load(AtomicOrdering::SeqCst),
            "blocking status task should have started"
        );

        let live = p2p_status_for_rpc(
            Some(Arc::new(StaticP2pHandle {
                status: live_status.clone(),
            })),
            "/test/live-after-timeout",
        )
        .await
        .expect("permit should be available after timeout")
        .expect("live status exists");

        assert!(!live.stale, "next snapshot should be live, not busy/stale");
        assert_eq!(live.status.chain_id, live_status.chain_id);
        let after = p2p_status_snapshot_metrics();
        assert_eq!(
            after.busy_total, before.busy_total,
            "timed-out blocking task must not keep the snapshot permit busy"
        );
    }

    struct StaticP2pHandle {
        status: P2pStatus,
    }

    impl P2pHandle for StaticP2pHandle {
        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &Block) -> Result<(), PulseError> {
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            Ok(self.status.clone())
        }
    }
}

pub trait RpcStateLike: Clone + Send + Sync + 'static {
    fn chain(&self) -> Arc<RwLock<ChainState>>;
    fn p2p(&self) -> Option<Arc<dyn P2pHandle>>;
    fn storage(&self) -> Arc<Storage>;
    fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>>;
    fn rpc_snapshot(&self) -> NodeRpcSnapshotStore {
        static DEFAULT_RPC_SNAPSHOT: OnceLock<NodeRpcSnapshotStore> = OnceLock::new();
        DEFAULT_RPC_SNAPSHOT
            .get_or_init(NodeRpcSnapshotStore::default)
            .clone()
    }
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
