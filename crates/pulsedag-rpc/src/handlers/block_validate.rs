use axum::{extract::State, Json};
use crate::{api::{ApiResponse, RpcStateLike, SubmitBlockRequest}};
use pulsedag_core::{dev_pow_accepts, dev_surrogate_pow_hash, dev_target_u64};

#[derive(Debug, serde::Serialize)]
pub struct BlockValidateData {
    pub valid: bool,
    pub block_hash: String,
    pub height: u64,
    pub parent_count: usize,
    pub reason: Option<String>,
    pub pow_hash: String,
    pub pow_target_u64: u64,
    pub pow_accepted_dev: bool,
}

pub async fn post_block_validate<S: RpcStateLike>(State(state): State<S>, Json(req): Json<SubmitBlockRequest>) -> Json<ApiResponse<BlockValidateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut simulated = chain.clone();
    drop(chain);

    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;
    let parent_count = req.block.header.parents.len();
    let pow_hash = dev_surrogate_pow_hash(&req.block.header);
    let pow_target_u64 = dev_target_u64(req.block.header.difficulty as u64);
    let pow_accepted_dev = dev_pow_accepts(&req.block.header);

    match pulsedag_core::accept_block(req.block.clone(), &mut simulated, pulsedag_core::AcceptSource::Rpc) {
        Ok(_) => Json(ApiResponse::ok(BlockValidateData {
            valid: true,
            block_hash,
            height,
            parent_count,
            reason: None,
            pow_hash,
            pow_target_u64,
            pow_accepted_dev,
        })),
        Err(e) => Json(ApiResponse::ok(BlockValidateData {
            valid: false,
            block_hash,
            height,
            parent_count,
            reason: Some(e.to_string()),
            pow_hash,
            pow_target_u64,
            pow_accepted_dev,
        })),
    }
}
