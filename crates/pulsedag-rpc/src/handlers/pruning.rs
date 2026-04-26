use std::time::{SystemTime, UNIX_EPOCH};

use axum::{extract::State, Json};
use pulsedag_core::rebuild_state_from_snapshot_and_blocks;
use serde::Deserialize;

use crate::{api::ApiResponse, api::RpcStateLike};

const MIN_SAFE_ROLLBACK_BLOCKS: u64 = 16;

#[derive(Debug, Deserialize)]
pub struct PruneRequest {
    pub keep_recent_blocks: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct PruneData {
    pub pruned_block_count: usize,
    pub keep_from_height: u64,
    pub best_height: u64,
    pub snapshot_required: bool,
    pub snapshot_validated: bool,
    pub replay_verified: bool,
}

pub async fn post_prune_chain<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<PruneRequest>,
) -> Json<ApiResponse<PruneData>> {
    let prune_keep_recent_blocks = {
        let runtime_handle = state.runtime();
        let runtime = runtime_handle.read().await;
        req.keep_recent_blocks
            .unwrap_or(runtime.prune_keep_recent_blocks)
            .max(1)
    };

    let chain_handle = state.chain();
    let chain_guard = chain_handle.read().await;
    let best_height = chain_guard.dag.best_height;
    let requested_keep_from_height =
        best_height.saturating_sub(prune_keep_recent_blocks.saturating_sub(1));
    drop(chain_guard);

    let safety_plan = match state.storage().plan_prune_with_safety(
        requested_keep_from_height,
        best_height,
        MIN_SAFE_ROLLBACK_BLOCKS,
    ) {
        Ok(plan) => plan,
        Err(e) => return Json(ApiResponse::err("SNAPSHOT_READ_ERROR", e.to_string())),
    };

    let keep_from_height = safety_plan.effective_keep_from_height;
    if !safety_plan.can_prune {
        let reason = safety_plan
            .reason
            .unwrap_or_else(|| "prune safety preconditions not met".to_string());
        let _ = state
            .storage()
            .append_runtime_event("warn", "prune_rejected", &reason);
        return Json(ApiResponse::err("PRUNE_REQUIRES_VALID_SNAPSHOT", reason));
    }
    let snapshot = match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return Json(ApiResponse::err(
                "SNAPSHOT_READ_ERROR",
                "snapshot disappeared during prune planning".to_string(),
            ))
        }
        Err(e) => return Json(ApiResponse::err("SNAPSHOT_READ_ERROR", e.to_string())),
    };
    let snapshot_validated = true;

    let pre_prune_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_BLOCKS_READ_ERROR", e.to_string())),
    };
    let retained_blocks: Vec<_> = pre_prune_blocks
        .into_iter()
        .filter(|b| b.header.height >= keep_from_height)
        .collect();

    let precheck_rebuilt = match rebuild_state_from_snapshot_and_blocks(
        snapshot.clone(),
        retained_blocks,
    ) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.storage().append_runtime_event(
                "error",
                "prune_snapshot_delta_precheck_failed",
                &format!(
                    "snapshot+delta precheck failed before prune (snapshot_height={}, keep_from_height={}): {}",
                    snapshot.dag.best_height, keep_from_height, e
                ),
            );
            return Json(ApiResponse::err(
                "PRUNE_SNAPSHOT_DELTA_PRECHECK_FAILED",
                e.to_string(),
            ));
        }
    };

    if let Err(e) = state.storage().persist_chain_state(&precheck_rebuilt) {
        return Json(ApiResponse::err("PRUNE_STATE_PERSIST_ERROR", e.to_string()));
    }

    let pruned_block_count = match state.storage().prune_blocks_below_height(keep_from_height) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_ERROR", e.to_string())),
    };

    let post_prune_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_BLOCKS_READ_ERROR", e.to_string())),
    };
    let rebuilt = match rebuild_state_from_snapshot_and_blocks(snapshot.clone(), post_prune_blocks)
    {
        Ok(v) => v,
        Err(e) => {
            let _ = state.storage().append_runtime_event(
                "error",
                "prune_snapshot_delta_postprune_failed",
                &format!(
                    "snapshot+delta rebuild failed after prune (snapshot_height={}, keep_from_height={}): {}",
                    snapshot.dag.best_height, keep_from_height, e
                ),
            );
            return Json(ApiResponse::err(
                "PRUNE_SNAPSHOT_DELTA_POSTPRUNE_FAILED",
                e.to_string(),
            ));
        }
    };
    if let Err(e) = state.storage().persist_chain_state(&rebuilt) {
        return Json(ApiResponse::err("PRUNE_STATE_PERSIST_ERROR", e.to_string()));
    }

    {
        let chain_handle = state.chain();
        let mut chain = chain_handle.write().await;
        *chain = rebuilt.clone();
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.last_prune_height = Some(rebuilt.dag.best_height);
        runtime.last_prune_unix = Some(now);
        runtime.last_snapshot_height = Some(rebuilt.dag.best_height);
        runtime.last_snapshot_unix = state
            .storage()
            .snapshot_captured_at_unix()
            .ok()
            .flatten()
            .or(Some(now));
    }

    let _ = state.storage().append_runtime_event(
        "info",
        "prune_manual",
        &format!(
            "manual prune removed {} blocks below {}; requested_keep_from={} minimum_safe_keep_from={} min_safe_rollback_blocks={} snapshot+delta verified at height {} (snapshot_height={})",
            pruned_block_count,
            keep_from_height,
            safety_plan.requested_keep_from_height,
            safety_plan.minimum_safe_keep_from_height,
            MIN_SAFE_ROLLBACK_BLOCKS,
            rebuilt.dag.best_height,
            snapshot.dag.best_height
        ),
    );

    Json(ApiResponse::ok(PruneData {
        pruned_block_count,
        keep_from_height,
        best_height: rebuilt.dag.best_height,
        snapshot_required: true,
        snapshot_validated,
        replay_verified: true,
    }))
}
