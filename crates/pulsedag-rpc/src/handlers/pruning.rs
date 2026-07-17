use std::{
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{extract::State, Json};
use pulsedag_core::{
    compact_snapshot_to_retained_blocks, preferred_tip_hash, rebuild_state_from_snapshot_and_blocks,
};
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

    // The write lock is the mutation barrier for block acceptance and pruning.
    // Storage generation checks provide a second fail-closed guard.
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let best_height = chain.dag.best_height;
    let requested_keep_from_height =
        best_height.saturating_sub(prune_keep_recent_blocks.saturating_sub(1));
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

    let persisted_snapshot = match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return Json(ApiResponse::err(
                "SNAPSHOT_READ_ERROR",
                "snapshot disappeared during prune planning".to_string(),
            ))
        }
        Err(e) => return Json(ApiResponse::err("SNAPSHOT_READ_ERROR", e.to_string())),
    };
    let persisted_state_root = match persisted_snapshot.utxo.compute_state_root() {
        Ok(root) => root,
        Err(e) => {
            return Json(ApiResponse::err(
                "PRUNE_SNAPSHOT_STATE_ROOT_ERROR",
                e.to_string(),
            ))
        }
    };
    let live_state_root = match chain.utxo.compute_state_root() {
        Ok(root) => root,
        Err(e) => return Json(ApiResponse::err("PRUNE_STATE_ROOT_ERROR", e.to_string())),
    };
    if persisted_snapshot.dag.best_height != chain.dag.best_height
        || preferred_tip_hash(&persisted_snapshot) != preferred_tip_hash(&chain)
        || persisted_state_root != live_state_root
    {
        return Json(ApiResponse::err(
            "PRUNE_SNAPSHOT_STALE",
            "persisted snapshot does not match the mutation-locked live chain".to_string(),
        ));
    }

    let expected_generation = match state.storage().accepted_storage_generation() {
        Ok(generation) => generation,
        Err(e) => {
            return Json(ApiResponse::err(
                "PRUNE_GENERATION_READ_ERROR",
                e.to_string(),
            ))
        }
    };
    let pre_prune_blocks = match state.storage().list_blocks() {
        Ok(blocks) => blocks,
        Err(e) => return Json(ApiResponse::err("PRUNE_BLOCKS_READ_ERROR", e.to_string())),
    };
    let retained_blocks = pre_prune_blocks
        .iter()
        .filter(|block| block.header.height >= keep_from_height)
        .cloned()
        .collect::<Vec<_>>();
    let retained_hashes = retained_blocks
        .iter()
        .map(|block| block.hash.clone())
        .collect::<BTreeSet<_>>();
    let before_tip = preferred_tip_hash(&chain);
    let before_state_root = live_state_root;
    let compact = match compact_snapshot_to_retained_blocks(chain.clone(), &retained_blocks) {
        Ok(compact) => compact,
        Err(e) => {
            return Json(ApiResponse::err(
                "PRUNE_RETAINED_SET_COMPACTION_FAILED",
                e.to_string(),
            ))
        }
    };
    if compact.dag.blocks.len() != retained_hashes.len()
        || preferred_tip_hash(&compact) != before_tip
        || compact.utxo.compute_state_root().ok().as_deref() != Some(before_state_root.as_str())
    {
        return Json(ApiResponse::err(
            "PRUNE_COMPACT_STATE_INVARIANT_FAILED",
            "compact snapshot changed retained count, selected tip, or state root".to_string(),
        ));
    }
    if let Err(e) = rebuild_state_from_snapshot_and_blocks(compact.clone(), retained_blocks.clone())
    {
        return Json(ApiResponse::err(
            "PRUNE_SNAPSHOT_DELTA_PRECHECK_FAILED",
            e.to_string(),
        ));
    }

    let pruned_block_count =
        match state
            .storage()
            .commit_compact_prune(&compact, &retained_hashes, expected_generation)
        {
            Ok(count) => count,
            Err(e) => return Json(ApiResponse::err("PRUNE_ATOMIC_COMMIT_ERROR", e.to_string())),
        };
    let invariant_report = match state.storage().verify_accepted_storage_invariants(&compact) {
        Ok(report) => report,
        Err(e) => {
            return Json(ApiResponse::err(
                "PRUNE_RETAINED_SET_CHECK_ERROR",
                e.to_string(),
            ))
        }
    };
    if !invariant_report.is_ok() {
        return Json(ApiResponse::err(
            "PRUNE_RETAINED_SET_MISMATCH",
            format!(
                "storage retained {} blocks but compact snapshot retained {}",
                invariant_report.accepted_storage_count, invariant_report.in_memory_dag_count
            ),
        ));
    }
    *chain = compact.clone();
    drop(chain);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.last_prune_height = Some(compact.dag.best_height);
        runtime.last_prune_unix = Some(now);
        runtime.last_snapshot_height = Some(compact.dag.best_height);
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
            "atomic compact prune removed {} blocks below {}; retained={} requested_keep_from={} minimum_safe_keep_from={} min_safe_rollback_blocks={} height={}",
            pruned_block_count,
            keep_from_height,
            retained_hashes.len(),
            safety_plan.requested_keep_from_height,
            safety_plan.minimum_safe_keep_from_height,
            MIN_SAFE_ROLLBACK_BLOCKS,
            compact.dag.best_height,
        ),
    );

    Json(ApiResponse::ok(PruneData {
        pruned_block_count,
        keep_from_height,
        best_height: compact.dag.best_height,
        snapshot_required: true,
        snapshot_validated: true,
        replay_verified: true,
    }))
}
