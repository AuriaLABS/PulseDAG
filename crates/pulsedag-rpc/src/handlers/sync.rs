use axum::{extract::State, Json};
use std::collections::BTreeMap;

use crate::api::{
    p2p_status_for_rpc, read_chain_for_rpc, read_runtime_for_rpc, ApiResponse, RpcStateLike,
    SyncRebuildRequest,
};
use pulsedag_core::reconcile_mempool;

#[derive(Debug, serde::Serialize)]
pub struct SyncStatusData {
    pub chain_id: String,
    pub p2p_enabled: bool,
    pub p2p_mode: String,
    pub snapshot_exists: bool,
    pub persisted_block_count: usize,
    pub in_memory_block_count: usize,
    pub best_height: u64,
    pub selected_tip: Option<String>,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub orphan_count: usize,
    pub pending_block_requests: usize,
    pub inflight_block_requests: usize,
    pub pending_block_request_hashes: Vec<String>,
    pub duplicate_block_requests_suppressed: u64,
    pub pending_missing_parents: usize,
    pub can_replay_from_blocks: bool,
    pub replay_gap: i64,
    pub rebuild_recommended: bool,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
    pub catchup_stage: String,
    pub lag_blocks: u64,
    pub lag_band: String,
    pub catchup_progress_bps: u64,
    pub catchup_summary: String,
    pub recovery_reason: Option<String>,
    pub sync_state: String,
    pub selected_sync_peer: Option<String>,
    pub last_accepted_peer_block: Option<String>,
    pub last_rejected_peer_block_reason: Option<String>,
    pub chain_id_mismatch_drops: usize,
    pub duplicate_suppression_counters: SyncDuplicateSuppressionCounters,
    pub blocks_requested: u64,
    pub blocks_received: u64,
    pub invalid_blocks_received: u64,
    pub orphan_blocks_received: u64,
    pub missing_parent_requests_sent: u64,
    pub missing_parent_responses_received: u64,
    pub missing_parent_request_timeouts: u64,
    pub missing_parent_request_retries: u64,
    pub missing_parent_request_fallbacks: u64,
    pub orphan_blocks_queued: u64,
    pub orphan_blocks_retried: u64,
    pub orphan_blocks_resolved: u64,
    pub orphan_blocks_evicted: u64,
    pub blockdata_not_found: u64,
    pub block_request_retries: u64,
    pub block_request_fallbacks: u64,
    pub block_request_backpressure_suppressed: u64,
    pub block_request_fetches_queued: u64,
    pub block_request_fetches_dropped: u64,
    pub scheduler_queue_depth: usize,
    pub max_orphan_age_secs: u64,
    pub oldest_orphan_age_secs: u64,
    pub oldest_missing_parent_age_secs: u64,
    pub orphan_reprocess_attempts: u64,
    pub orphan_reprocess_success: u64,
    pub orphan_reprocess_failed_missing_parent: u64,
    pub orphan_reprocess_failed_persist: u64,
    pub orphan_reprocess_failures_by_reason: BTreeMap<String, u64>,
    pub last_orphan_reprocess_failure_reason: Option<String>,
    pub duplicate_blocks_received: u64,
    pub peer_penalties: u64,
    pub p2p_ready_for_private_rehearsal: bool,
    pub readiness_reasons: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncReconcileMempoolData {
    pub removed_count: usize,
    pub kept_count: usize,
    pub removed_txids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncDuplicateSuppressionCounters {
    pub inbound_messages: usize,
    pub outbound_messages: usize,
    pub tx_outbound: usize,
    pub block_outbound: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncRebuildData {
    pub rebuilt: bool,
    pub block_count: usize,
    pub best_height: u64,
    pub selected_tip: Option<String>,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
    pub mempool_reconciled: bool,
    pub snapshot_persisted: bool,
    pub partial_replay_used: bool,
    pub accepted_blocks: usize,
    pub skipped_blocks: usize,
    pub skipped_hashes: Vec<String>,
}

pub async fn get_sync_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncStatusData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_block_count = match state.storage().block_count() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let chain_handle = state.chain();
    let chain = match read_chain_for_rpc(&chain_handle, "/sync/status").await {
        Ok(chain) => chain,
        Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
    };
    let runtime_handle = state.runtime();
    let runtime = match read_runtime_for_rpc(&runtime_handle, "/sync/status").await {
        Ok(runtime) => runtime,
        Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
    };
    let consistency_issues = pulsedag_core::dag_consistency_issues(&chain);
    let lag_blocks = (persisted_block_count as u64).saturating_sub(chain.dag.blocks.len() as u64);
    let lag_band = match lag_blocks {
        0 => "aligned",
        1..=2 => "near_tip",
        3..=10 => "catching_up",
        11..=100 => "lagging",
        _ => "severely_lagging",
    }
    .to_string();
    let catchup_progress_bps = if persisted_block_count == 0 {
        10_000
    } else {
        (chain.dag.blocks.len() as u64)
            .saturating_mul(10_000)
            .saturating_div(persisted_block_count as u64)
            .min(10_000)
    };
    let counters_coherent = runtime.sync_pipeline.counters.blocks_applied
        <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stalled = runtime.sync_pipeline.phase != pulsedag_core::SyncPhase::Idle
        && (lag_blocks > 0
            || runtime.sync_pipeline.counters.blocks_requested
                > runtime.sync_pipeline.counters.blocks_applied)
        && runtime
            .sync_pipeline
            .last_transition_unix
            .map(|ts| now_unix.saturating_sub(ts) > 120)
            .unwrap_or(false);
    let catchup_stage = if runtime.sync_pipeline.last_error.is_some() || !counters_coherent {
        "degraded"
    } else {
        match runtime.sync_pipeline.phase {
            pulsedag_core::SyncPhase::Idle if lag_blocks == 0 => "steady",
            pulsedag_core::SyncPhase::Idle => "recovering",
            pulsedag_core::SyncPhase::PeerSelection => "discovering",
            pulsedag_core::SyncPhase::HeaderDiscovery => "discovering",
            pulsedag_core::SyncPhase::BlockAcquisition => "recovering",
            pulsedag_core::SyncPhase::ValidationApplication => "validating",
            pulsedag_core::SyncPhase::CatchUpCompletion => "steady",
        }
    }
    .to_string();
    let p2p_status = match p2p_status_for_rpc(state.p2p(), "/sync/status").await {
        Ok(status) => status,
        Err(e) => return Json(ApiResponse::err("P2P_STATUS_BUSY", e)),
    };
    let p2p_enabled = p2p_status.is_some();
    let p2p_mode = p2p_status
        .as_ref()
        .map(|snapshot| snapshot.status.mode.clone())
        .unwrap_or_else(|| "disabled".to_string());
    let pending_missing_parents = pulsedag_core::pending_missing_parent_count(&chain);
    let mut readiness_reasons = Vec::new();
    if !p2p_enabled {
        readiness_reasons.push("p2p is disabled".to_string());
    }
    if p2p_status
        .as_ref()
        .is_some_and(|snapshot| snapshot.status.connected_peers.is_empty())
    {
        readiness_reasons.push("no connected peers".to_string());
    }
    if runtime.pending_block_requests > 0 {
        readiness_reasons.push(format!(
            "{} pending block request(s)",
            runtime.pending_block_requests
        ));
    }
    if pending_missing_parents > 0 {
        readiness_reasons.push(format!(
            "{} pending missing parent(s)",
            pending_missing_parents
        ));
    }
    if !chain.orphan_blocks.is_empty() {
        readiness_reasons.push(format!(
            "{} orphan block(s) queued",
            chain.orphan_blocks.len()
        ));
    }

    let recovery_reason = if let Some(err) = runtime.sync_pipeline.last_error.clone() {
        Some(format!("sync error: {err}"))
    } else if !counters_coherent {
        Some("sync counter incoherence detected; verify sync pipeline accounting".to_string())
    } else if stalled {
        Some(format!(
            "no-progress escalation: sync stalled in {:?} with lag_band={lag_band}; bounded remediation active (fallbacks={}, timeouts={}, restarts={})",
            runtime.sync_pipeline.phase,
            runtime.sync_pipeline.fallback_count,
            runtime.sync_pipeline.timeout_fallback_count,
            runtime.sync_pipeline.restart_count
        ))
    } else if catchup_stage != "steady" {
        Some(format!(
            "catch-up in progress: stage={catchup_stage}, lag_band={lag_band}, replay_gap={}",
            persisted_block_count as i64 - chain.dag.blocks.len() as i64
        ))
    } else {
        None
    };
    let catchup_summary = format!(
        "stage={catchup_stage} lag_blocks={lag_blocks} lag_band={lag_band} progress_bps={catchup_progress_bps}"
    );
    Json(ApiResponse::ok(SyncStatusData {
        chain_id: chain.chain_id.clone(),
        p2p_enabled,
        p2p_mode,
        snapshot_exists,
        persisted_block_count,
        in_memory_block_count: chain.dag.blocks.len(),
        best_height: chain.dag.best_height,
        selected_tip: pulsedag_core::preferred_tip_hash(&chain),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        orphan_count: chain.orphan_blocks.len(),
        pending_block_requests: runtime.pending_block_requests,
        inflight_block_requests: runtime.inflight_block_requests,
        pending_block_request_hashes: runtime.pending_block_request_hashes.clone(),
        duplicate_block_requests_suppressed: runtime.duplicate_block_requests_suppressed,
        pending_missing_parents,
        can_replay_from_blocks: persisted_block_count > 0,
        replay_gap: persisted_block_count as i64 - chain.dag.blocks.len() as i64,
        rebuild_recommended: !snapshot_exists || persisted_block_count > chain.dag.blocks.len(),
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
        catchup_stage,
        lag_blocks,
        lag_band,
        catchup_progress_bps,
        catchup_summary,
        recovery_reason,
        sync_state: runtime.sync_state.clone(),
        selected_sync_peer: runtime.sync_pipeline.selected_peer.clone().or_else(|| {
            p2p_status
                .as_ref()
                .and_then(|snapshot| snapshot.status.selected_sync_peer.clone())
        }),
        last_accepted_peer_block: runtime.last_accepted_peer_block.clone(),
        last_rejected_peer_block_reason: runtime.last_rejected_peer_block_reason.clone(),
        chain_id_mismatch_drops: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.inbound_chain_mismatch_dropped)
            .unwrap_or(0),
        duplicate_suppression_counters: SyncDuplicateSuppressionCounters {
            inbound_messages: p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.inbound_duplicates_suppressed)
                .unwrap_or(0),
            outbound_messages: p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.outbound_duplicates_suppressed)
                .unwrap_or(0),
            tx_outbound: p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.tx_outbound_duplicates_suppressed)
                .unwrap_or(0),
            block_outbound: p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.block_outbound_duplicates_suppressed)
                .unwrap_or(0),
        },
        blocks_requested: runtime
            .sync_pipeline
            .counters
            .blocks_requested
            .max(runtime.getblock_sent)
            .max(
                p2p_status
                    .as_ref()
                    .map(|snapshot| snapshot.status.blocks_requested)
                    .unwrap_or(0),
            ),
        blocks_received: runtime.blockdata_received.max(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.blocks_received)
                .unwrap_or(0),
        ),
        invalid_blocks_received: runtime.rejected_p2p_blocks.max(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.invalid_blocks_received)
                .unwrap_or(0),
        ),
        orphan_blocks_received: runtime.blockdata_missing_parent,
        missing_parent_requests_sent: runtime.missing_parent_requests_sent,
        missing_parent_responses_received: runtime.missing_parent_responses_received,
        missing_parent_request_timeouts: runtime.missing_parent_request_timeouts,
        missing_parent_request_retries: runtime.missing_parent_request_retries,
        missing_parent_request_fallbacks: runtime.missing_parent_request_fallbacks,
        orphan_blocks_queued: runtime.orphan_blocks_queued,
        orphan_blocks_retried: runtime.orphan_blocks_retried,
        orphan_blocks_resolved: runtime.orphan_blocks_resolved,
        orphan_blocks_evicted: runtime.orphan_blocks_evicted,
        blockdata_not_found: runtime.blockdata_not_found,
        block_request_retries: runtime.block_request_retries,
        block_request_fallbacks: runtime.block_request_fallbacks,
        block_request_backpressure_suppressed: runtime.block_request_backpressure_suppressed,
        block_request_fetches_queued: runtime.block_request_fetches_queued,
        block_request_fetches_dropped: runtime.block_request_fetches_dropped,
        scheduler_queue_depth: runtime.block_fetch_scheduler_queue_depth,
        max_orphan_age_secs: runtime.max_orphan_age_secs,
        oldest_orphan_age_secs: runtime.oldest_orphan_age_secs,
        oldest_missing_parent_age_secs: runtime.oldest_missing_parent_age_secs,
        orphan_reprocess_attempts: runtime.orphan_reprocess_attempts,
        orphan_reprocess_success: runtime.orphan_reprocess_success,
        orphan_reprocess_failed_missing_parent: runtime.orphan_reprocess_failed_missing_parent,
        orphan_reprocess_failed_persist: runtime.orphan_reprocess_failed_persist,
        orphan_reprocess_failures_by_reason: runtime.orphan_reprocess_failures_by_reason.clone(),
        last_orphan_reprocess_failure_reason: runtime.last_orphan_reprocess_failure_reason.clone(),
        duplicate_blocks_received: runtime.duplicate_p2p_blocks.max(
            p2p_status
                .as_ref()
                .map(|snapshot| snapshot.status.duplicate_blocks_received)
                .unwrap_or(0),
        ),
        peer_penalties: p2p_status
            .as_ref()
            .map(|snapshot| snapshot.status.peer_penalties)
            .unwrap_or(0),
        p2p_ready_for_private_rehearsal: readiness_reasons.is_empty(),
        readiness_reasons,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct MissingBlockEntry {
    pub hash: String,
    pub missing_parents: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct MissingParentIndexEntry {
    pub parent: String,
    pub waiting_orphans: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncMissingData {
    pub pending_block_requests: usize,
    pub pending_missing_parents: usize,
    pub orphan_count: usize,
    pub orphans: Vec<MissingBlockEntry>,
    pub missing_parent_index: Vec<MissingParentIndexEntry>,
}

pub async fn get_sync_missing<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncMissingData>> {
    let chain_handle = state.chain();
    let chain = match read_chain_for_rpc(&chain_handle, "/sync/missing").await {
        Ok(chain) => chain,
        Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
    };
    let runtime_handle = state.runtime();
    let runtime = match read_runtime_for_rpc(&runtime_handle, "/sync/missing").await {
        Ok(runtime) => runtime,
        Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
    };
    let mut orphan_hashes = chain.orphan_blocks.keys().cloned().collect::<Vec<_>>();
    orphan_hashes.sort();
    let orphans = orphan_hashes
        .into_iter()
        .map(|hash| MissingBlockEntry {
            missing_parents: chain
                .orphan_missing_parents
                .get(&hash)
                .cloned()
                .unwrap_or_default(),
            hash,
        })
        .collect::<Vec<_>>();
    let pending_missing_parents = pulsedag_core::pending_missing_parent_count(&chain);
    let mut missing_parent_index = chain
        .orphan_parent_index
        .iter()
        .map(|(parent, waiting)| MissingParentIndexEntry {
            parent: parent.clone(),
            waiting_orphans: waiting.iter().cloned().collect(),
        })
        .collect::<Vec<_>>();
    missing_parent_index.sort_by(|left, right| left.parent.cmp(&right.parent));
    Json(ApiResponse::ok(SyncMissingData {
        pending_block_requests: runtime.pending_block_requests,
        pending_missing_parents,
        orphan_count: chain.orphan_blocks.len(),
        orphans,
        missing_parent_index,
    }))
}

pub async fn post_sync_rebuild<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SyncRebuildRequest>,
) -> Json<ApiResponse<SyncRebuildData>> {
    let current_chain_id = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        chain.chain_id.clone()
    };

    if !req.force {
        return Json(ApiResponse::err(
            "REBUILD_REQUIRES_FORCE",
            "set force=true to rebuild state from persisted blocks",
        ));
    }

    let allow_partial_replay = req.allow_partial_replay.unwrap_or(false);
    let persist_after_rebuild = req.persist_after_rebuild.unwrap_or(true);
    let reconcile_mempool_after = req.reconcile_mempool.unwrap_or(true);

    let (mut rebuilt, partial_replay_used, accepted_blocks, skipped_blocks, skipped_hashes) =
        if allow_partial_replay {
            let persisted_blocks = match state.storage().list_blocks() {
                Ok(v) => v,
                Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
            };
            let report = pulsedag_core::rebuild_state_from_blocks_defensive(
                current_chain_id,
                persisted_blocks,
            );
            (
                report.state,
                true,
                report.accepted_blocks,
                report.skipped_blocks,
                report.skipped_hashes,
            )
        } else {
            let rebuilt = match state.storage().replay_blocks_or_init(current_chain_id) {
                Ok(v) => v,
                Err(e) => return Json(ApiResponse::err("REBUILD_ERROR", e.to_string())),
            };
            let accepted_blocks = rebuilt.dag.blocks.len().saturating_sub(1);
            (rebuilt, false, accepted_blocks, 0, Vec::new())
        };

    let mempool_reconciled = if reconcile_mempool_after {
        let _ = reconcile_mempool(&mut rebuilt);
        true
    } else {
        false
    };

    let block_count = rebuilt.dag.blocks.len();
    let best_height = rebuilt.dag.best_height;
    let selected_tip = pulsedag_core::preferred_tip_hash(&rebuilt);
    let consistency_issues = pulsedag_core::dag_consistency_issues(&rebuilt);

    {
        let chain_handle = state.chain();
        let mut chain = chain_handle.write().await;
        *chain = rebuilt.clone();
    }

    let snapshot_persisted = if persist_after_rebuild {
        if let Err(e) = state.storage().persist_chain_state(&rebuilt) {
            return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
        }
        true
    } else {
        false
    };

    Json(ApiResponse::ok(SyncRebuildData {
        rebuilt: true,
        block_count,
        best_height,
        selected_tip,
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
        mempool_reconciled,
        snapshot_persisted,
        partial_replay_used,
        accepted_blocks,
        skipped_blocks,
        skipped_hashes,
    }))
}

pub async fn post_sync_reconcile_mempool<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncReconcileMempoolData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let result = reconcile_mempool(&mut chain);
    let snapshot = chain.clone();
    drop(chain);

    if let Err(e) = state.storage().persist_chain_state(&snapshot) {
        return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
    }

    Json(ApiResponse::ok(SyncReconcileMempoolData {
        removed_count: result.removed_txids.len(),
        kept_count: result.kept_txids.len(),
        removed_txids: result.removed_txids,
    }))
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{extract::State, Json};
    use pulsedag_core::{ChainState, SyncPhase};
    use pulsedag_storage::Storage;
    use tokio::sync::RwLock;

    use crate::api::{NodeRuntimeStats, RpcStateLike};

    use super::{get_sync_missing, get_sync_status};

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
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
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    #[tokio::test]
    async fn sync_status_derives_catchup_stage_and_recovery_reason_coherently() {
        let path = temp_db_path("sync-status-stage");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::BlockAcquisition;
        runtime.sync_pipeline.counters.blocks_requested = 20;
        runtime.sync_pipeline.counters.blocks_acquired = 10;
        runtime.sync_pipeline.counters.blocks_validated = 8;
        runtime.sync_pipeline.counters.blocks_applied = 6;
        runtime.oldest_orphan_age_secs = 42;
        runtime.oldest_missing_parent_age_secs = 41;
        runtime.orphan_reprocess_attempts = 7;
        runtime.orphan_reprocess_success = 5;
        runtime.orphan_reprocess_failed_missing_parent = 2;
        runtime.orphan_reprocess_failed_persist = 1;
        runtime
            .orphan_reprocess_failures_by_reason
            .insert("missing_parent".to_string(), 2);
        runtime.last_orphan_reprocess_failure_reason = Some("missing_parent".to_string());

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp) = get_sync_status(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("sync status payload");
        assert_eq!(data.catchup_stage, "recovering");
        assert!(data.recovery_reason.is_some());
        assert_eq!(data.lag_band, "aligned");
        assert_eq!(data.catchup_progress_bps, 10_000);
        assert!(!data.p2p_enabled);
        assert_eq!(data.p2p_mode, "disabled");
        assert_eq!(data.pending_block_requests, 0);
        assert_eq!(data.pending_missing_parents, 0);
        assert_eq!(data.oldest_orphan_age_secs, 42);
        assert_eq!(data.oldest_missing_parent_age_secs, 41);
        assert_eq!(data.orphan_reprocess_attempts, 7);
        assert_eq!(data.orphan_reprocess_success, 5);
        assert_eq!(data.orphan_reprocess_failed_missing_parent, 2);
        assert_eq!(data.orphan_reprocess_failed_persist, 1);
        assert_eq!(
            data.orphan_reprocess_failures_by_reason
                .get("missing_parent"),
            Some(&2)
        );
        assert_eq!(
            data.last_orphan_reprocess_failure_reason.as_deref(),
            Some("missing_parent")
        );
        assert_eq!(data.sync_state, "");
        assert_eq!(data.chain_id_mismatch_drops, 0);
        assert_eq!(data.duplicate_suppression_counters.inbound_messages, 0);
        assert_eq!(data.duplicate_suppression_counters.outbound_messages, 0);
        assert_eq!(data.duplicate_suppression_counters.tx_outbound, 0);
        assert_eq!(data.duplicate_suppression_counters.block_outbound, 0);
        assert!(data
            .readiness_reasons
            .iter()
            .any(|reason| reason == "p2p is disabled"));
    }

    #[tokio::test]
    async fn sync_missing_returns_orphan_and_pending_request_state() {
        let path = temp_db_path("sync-missing");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let child_block = "child-block".to_string();
        let missing_parents = vec![
            "missing-parent-a".to_string(),
            "missing-parent-b".to_string(),
        ];
        chain
            .orphan_missing_parents
            .insert(child_block.clone(), missing_parents.clone());
        for parent in missing_parents {
            chain
                .orphan_parent_index
                .entry(parent)
                .or_default()
                .insert(child_block.clone());
        }
        let mut runtime = NodeRuntimeStats::default();
        runtime.pending_block_requests = 3;
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp) = get_sync_missing(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("sync missing payload");
        assert_eq!(data.pending_block_requests, 3);
        assert_eq!(data.pending_missing_parents, 2);
        assert_eq!(data.orphans.len(), 0);
        assert_eq!(data.missing_parent_index.len(), 2);
        assert_eq!(data.missing_parent_index[0].parent, "missing-parent-a");
        assert_eq!(
            data.missing_parent_index[0].waiting_orphans,
            vec!["child-block".to_string()]
        );
    }

    #[tokio::test]
    async fn sync_status_lag_band_and_progress_are_bounded_and_deterministic() {
        let path = temp_db_path("sync-status-bounds");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 1;
        runtime.sync_pipeline.counters.blocks_acquired = 1;
        runtime.sync_pipeline.counters.blocks_validated = 1;
        runtime.sync_pipeline.counters.blocks_applied = 5;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp1) = get_sync_status(State(state.clone())).await;
        let Json(resp2) = get_sync_status(State(state)).await;
        let data1 = resp1.data.expect("sync status payload #1");
        let data2 = resp2.data.expect("sync status payload #2");
        assert_eq!(data1.lag_blocks, 0);
        assert_eq!(data1.lag_band, "aligned");
        assert_eq!(data1.catchup_progress_bps, 10_000);
        assert_eq!(data1.catchup_summary, data2.catchup_summary);
    }

    #[tokio::test]
    async fn sync_status_no_progress_escalation_is_explicit_and_bounded() {
        let path = temp_db_path("sync-status-no-progress");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 30;
        runtime.sync_pipeline.counters.blocks_acquired = 20;
        runtime.sync_pipeline.counters.blocks_validated = 20;
        runtime.sync_pipeline.counters.blocks_applied = 10;
        runtime.sync_pipeline.last_transition_unix = Some(now.saturating_sub(240));
        runtime.sync_pipeline.fallback_count = 2;
        runtime.sync_pipeline.timeout_fallback_count = 1;
        runtime.sync_pipeline.restart_count = 1;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp) = get_sync_status(State(state)).await;
        let data = resp.data.expect("sync status payload");
        assert_eq!(data.catchup_stage, "validating");
        let reason = data.recovery_reason.expect("recovery reason");
        assert!(reason.contains("no-progress escalation"));
        assert!(reason.contains("bounded remediation active"));
        assert!(reason.contains("fallbacks=2"));
        assert!(reason.contains("timeouts=1"));
        assert!(reason.contains("restarts=1"));
    }
}
