use std::time::{SystemTime, UNIX_EPOCH};

use axum::{extract::State, Json};
use std::collections::{HashMap, HashSet};

use pulsedag_core::{rebuild_state_from_blocks, state::ChainState};
use serde::Deserialize;

use crate::{api::ApiResponse, api::RpcStateLike};

fn prune_rebuilt_state(mut rebuilt: ChainState, keep_from_height: u64) -> ChainState {
    let genesis_hash = rebuilt.dag.genesis_hash.clone();
    rebuilt
        .dag
        .blocks
        .retain(|hash, block| hash == &genesis_hash || block.header.height >= keep_from_height);

    let mut children = HashMap::new();
    for block in rebuilt.dag.blocks.values() {
        for parent in &block.header.parents {
            if rebuilt.dag.blocks.contains_key(parent) {
                children
                    .entry(parent.clone())
                    .or_insert_with(Vec::new)
                    .push(block.hash.clone());
            }
        }
    }

    let mut tips = HashSet::new();
    for block in rebuilt.dag.blocks.values() {
        let has_retained_child = children
            .get(&block.hash)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !has_retained_child {
            tips.insert(block.hash.clone());
        }
    }

    rebuilt.dag.children = children;
    rebuilt.dag.tips = tips;
    rebuilt.dag.best_height = rebuilt
        .dag
        .blocks
        .values()
        .map(|b| b.header.height)
        .max()
        .unwrap_or(0);

    rebuilt
}

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
    let (prune_keep_recent_blocks, prune_require_snapshot) = {
        let runtime = state.runtime().read().await;
        (
            req.keep_recent_blocks
                .unwrap_or(runtime.prune_keep_recent_blocks)
                .max(1),
            runtime.prune_require_snapshot,
        )
    };

    let chain_guard = state.chain().read().await;
    let best_height = chain_guard.dag.best_height;
    let keep_from_height = best_height.saturating_sub(prune_keep_recent_blocks.saturating_sub(1));
    let chain_id = chain_guard.chain_id.clone();
    drop(chain_guard);

    let (snapshot_validated, snapshot_height) = match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => (
            snapshot.dag.best_height >= keep_from_height,
            Some(snapshot.dag.best_height),
        ),
        Ok(None) => (false, None),
        Err(e) => return Json(ApiResponse::err("SNAPSHOT_READ_ERROR", e.to_string())),
    };

    if prune_require_snapshot && !snapshot_validated {
        let reason = match snapshot_height {
            Some(h) => format!(
                "snapshot height {} below required keep_from_height {}",
                h, keep_from_height
            ),
            None => "snapshot missing".to_string(),
        };
        let _ = state
            .storage()
            .append_runtime_event("warn", "prune_rejected", &reason);
        return Json(ApiResponse::err("PRUNE_REQUIRES_VALID_SNAPSHOT", reason));
    }

    let pre_prune_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_BLOCKS_READ_ERROR", e.to_string())),
    };
    let precheck_rebuilt = match rebuild_state_from_blocks(chain_id.clone(), pre_prune_blocks) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.storage().append_runtime_event(
                "error",
                "prune_replay_precheck_failed",
                &format!(
                    "replay precheck failed before prune at keep_from_height {}: {}",
                    keep_from_height, e
                ),
            );
            return Json(ApiResponse::err(
                "PRUNE_REPLAY_PRECHECK_FAILED",
                e.to_string(),
            ));
        }
    };

    let pruned_block_count = match state.storage().prune_blocks_below_height(keep_from_height) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_ERROR", e.to_string())),
    };

    let post_prune_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_BLOCKS_READ_ERROR", e.to_string())),
    };

    let rebuilt = prune_rebuilt_state(precheck_rebuilt, keep_from_height);

    let post_prune_count = post_prune_blocks.len();
    let rebuilt_count = rebuilt.dag.blocks.len();
    if post_prune_count != rebuilt_count {
        let _ = state.storage().append_runtime_event(
            "warn",
            "prune_postcheck_mismatch",
            &format!(
                "post-prune block count {} differs from rebuilt state count {} at keep_from_height {}",
                post_prune_count, rebuilt_count, keep_from_height
            ),
        );
    }

    if let Err(e) = state.storage().persist_chain_state(&rebuilt) {
        return Json(ApiResponse::err("PRUNE_STATE_PERSIST_ERROR", e.to_string()));
    }

    {
        let mut chain = state.chain().write().await;
        *chain = rebuilt.clone();
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    {
        let mut runtime = state.runtime().write().await;
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
            "manual prune removed {} blocks below {}; persisted pruned state at height {}",
            pruned_block_count, keep_from_height, rebuilt.dag.best_height
        ),
    );

    Json(ApiResponse::ok(PruneData {
        pruned_block_count,
        keep_from_height,
        best_height: rebuilt.dag.best_height,
        snapshot_required: prune_require_snapshot,
        snapshot_validated,
        replay_verified: true,
    }))
}
