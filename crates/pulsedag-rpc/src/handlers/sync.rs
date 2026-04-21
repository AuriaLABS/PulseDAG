use axum::{extract::State, Json};

use crate::api::{ApiResponse, RpcStateLike, SyncRebuildRequest};
use pulsedag_core::reconcile_mempool;

#[derive(Debug, serde::Serialize)]
pub struct SyncStatusData {
    pub chain_id: String,
    pub snapshot_exists: bool,
    pub persisted_block_count: usize,
    pub in_memory_block_count: usize,
    pub best_height: u64,
    pub selected_tip: Option<String>,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub orphan_count: usize,
    pub can_replay_from_blocks: bool,
    pub replay_gap: i64,
    pub rebuild_recommended: bool,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncReconcileMempoolData {
    pub removed_count: usize,
    pub kept_count: usize,
    pub removed_txids: Vec<String>,
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

pub async fn get_sync_status<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<SyncStatusData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let consistency_issues = pulsedag_core::dag_consistency_issues(&chain);
    Json(ApiResponse::ok(SyncStatusData {
        chain_id: chain.chain_id.clone(),
        snapshot_exists,
        persisted_block_count: persisted_blocks.len(),
        in_memory_block_count: chain.dag.blocks.len(),
        best_height: chain.dag.best_height,
        selected_tip: pulsedag_core::preferred_tip_hash(&chain),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        orphan_count: chain.orphan_blocks.len(),
        can_replay_from_blocks: !persisted_blocks.is_empty(),
        replay_gap: persisted_blocks.len() as i64 - chain.dag.blocks.len() as i64,
        rebuild_recommended: !snapshot_exists || persisted_blocks.len() > chain.dag.blocks.len(),
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
    }))
}

pub async fn post_sync_rebuild<S: RpcStateLike>(State(state): State<S>, Json(req): Json<SyncRebuildRequest>) -> Json<ApiResponse<SyncRebuildData>> {
    let current_chain_id = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        chain.chain_id.clone()
    };

    if !req.force {
        return Json(ApiResponse::err("REBUILD_REQUIRES_FORCE", "set force=true to rebuild state from persisted blocks"));
    }

    let allow_partial_replay = req.allow_partial_replay.unwrap_or(false);
    let persist_after_rebuild = req.persist_after_rebuild.unwrap_or(true);
    let reconcile_mempool_after = req.reconcile_mempool.unwrap_or(true);

    let (mut rebuilt, partial_replay_used, accepted_blocks, skipped_blocks, skipped_hashes) = if allow_partial_replay {
        let persisted_blocks = match state.storage().list_blocks() {
            Ok(v) => v,
            Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
        };
        let report = pulsedag_core::rebuild_state_from_blocks_defensive(current_chain_id, persisted_blocks);
        (report.state, true, report.accepted_blocks, report.skipped_blocks, report.skipped_hashes)
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


pub async fn post_sync_reconcile_mempool<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<SyncReconcileMempoolData>> {
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
