use std::time::{SystemTime, UNIX_EPOCH};

use axum::{extract::State, Json};
use pulsedag_core::rebuild_state_from_blocks;
use serde::Deserialize;

use crate::{api::ApiResponse, api::RpcStateLike};

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
    let retained_blocks: Vec<_> = pre_prune_blocks
        .into_iter()
        .filter(|b| b.header.height >= keep_from_height)
        .collect();
    if let Err(e) = rebuild_state_from_blocks(chain_id.clone(), retained_blocks) {
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

    let pruned_block_count = match state.storage().prune_blocks_below_height(keep_from_height) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("PRUNE_ERROR", e.to_string())),
    };

    let rebuilt = match state.storage().replay_blocks_or_init(chain_id) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.storage().append_runtime_event(
                "error",
                "prune_replay_failed",
                &format!("failed replay after prune: {}", e),
            );
            return Json(ApiResponse::err(
                "PRUNE_REPLAY_VERIFY_FAILED",
                e.to_string(),
            ));
        }
    };

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
            "manual prune removed {} blocks below {}; replay verified at height {}",
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
