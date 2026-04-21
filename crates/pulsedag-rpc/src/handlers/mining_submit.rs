use axum::{extract::State, Json};
use crate::{api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest}};
use pulsedag_core::{accept_block, adopt_ready_orphans, dev_pow_accepts, dev_target_u64, preferred_tip_hash, AcceptSource};
use super::mining_template::load_template;

#[derive(Debug, serde::Serialize)]
pub struct MiningSubmitData {
    pub accepted: bool,
    pub block_hash: String,
    pub height: u64,
    pub pow_algorithm: String,
    pub pow_accepted_dev: bool,
    pub target_u64: u64,
    pub stale_template: bool,
    pub selected_tip: Option<String>,
    pub adopted_orphans: usize,
}

pub async fn post_mining_submit<S: RpcStateLike>(State(state): State<S>, Json(req): Json<SubmitMinedBlockRequest>) -> Json<ApiResponse<MiningSubmitData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let pow_accepted_dev = dev_pow_accepts(&req.block.header);
    let target_u64 = dev_target_u64(req.block.header.difficulty as u64);
    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;

    if !pow_accepted_dev {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.rejected_mined_blocks += 1;
        return Json(ApiResponse::err("INVALID_POW", "submitted block does not satisfy current dev pow check".to_string()));
    }

    if height <= chain.dag.best_height {
        return Json(ApiResponse::err("STALE_TEMPLATE", format!("stale template: current best height is {} and submitted block height is {}", chain.dag.best_height, height)));
    }

    let mut current_parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    current_parents.sort();
    let current_selected_tip = preferred_tip_hash(&chain);
    let expected_template_id = format!("{}:{}", chain.dag.best_height + 1, current_parents.join(","));

    if let Some(template_id) = req.template_id.as_ref() {
        if template_id != &expected_template_id {
            if let Some(stored) = load_template(template_id) {
                if stored.height != chain.dag.best_height + 1 {
                    return Json(ApiResponse::err("STALE_TEMPLATE", format!("template height {} is stale; current next height is {}", stored.height, chain.dag.best_height + 1)));
                }
                if stored.parent_hashes != current_parents {
                    return Json(ApiResponse::err("STALE_TEMPLATE", "template parents no longer match current tips"));
                }
                if stored.selected_tip != current_selected_tip {
                    return Json(ApiResponse::err("STALE_TEMPLATE", "template selected_tip no longer matches current preferred tip"));
                }
            } else {
                return Json(ApiResponse::err("UNKNOWN_TEMPLATE", format!("template_id {} not found", template_id)));
            }
        }
    }

    let mut submitted_parents = req.block.header.parents.clone();
    submitted_parents.sort();
    if submitted_parents != current_parents {
        return Json(ApiResponse::err("STALE_TEMPLATE", "submitted block parents no longer match current tip set"));
    }

    match accept_block(req.block.clone(), &mut chain, AcceptSource::Rpc) {
        Ok(_) => {
            let adopted_orphans = adopt_ready_orphans(&mut chain, AcceptSource::Rpc);
            {
                let runtime_handle = state.runtime();
                let mut runtime = runtime_handle.write().await;
                runtime.accepted_mined_blocks += 1;
                runtime.adopted_orphan_blocks += adopted_orphans as u64;
            }
            if let Err(e) = state.storage().persist_block(&req.block) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Err(e) = state.storage().persist_chain_state(&chain) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            Json(ApiResponse::ok(MiningSubmitData {
                accepted: true,
                block_hash,
                height,
                pow_algorithm: pulsedag_core::selected_pow_name().to_string(),
                pow_accepted_dev,
                target_u64,
                stale_template: false,
                selected_tip: preferred_tip_hash(&chain),
                adopted_orphans,
            }))
        }
        Err(e) => {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_mined_blocks += 1;
            Json(ApiResponse::err("SUBMIT_BLOCK_ERROR", e.to_string()))
        },
    }
}
